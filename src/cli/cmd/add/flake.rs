use std::collections::VecDeque;

const NEWLINE: &str = "\n";

#[tracing::instrument(skip_all)]
pub(crate) fn upsert_flake_input(
    expr: nixel::Expression,
    flake_input_name: String,
    flake_input_value: url::Url,
    flake_contents: String,
    input_attr_path: VecDeque<String>,
) -> color_eyre::Result<String> {
    match find_attrset(&expr, Some(input_attr_path))? {
        Some(attr) => {
            let nixel::Expression::String(existing_input_value) = *attr.to else {
                return Err(color_eyre::eyre::eyre!(
                    "`inputs.{flake_input_name}.url` was not a string" // this is enforced by Nix itself
                ))?;
            };
            replace_input_value(
                &existing_input_value.parts,
                &flake_input_value,
                &flake_contents,
            )
        }
        None => {
            let inputs_attr_path: VecDeque<String> = [String::from("inputs")].into();
            let outputs_attr_path: VecDeque<String> = [String::from("outputs")].into();

            let inputs_attr = find_attrset(&expr, Some(inputs_attr_path))?;
            let outputs_attr = find_attrset(&expr, Some(outputs_attr_path))?;

            upsert_into_inputs_and_outputs(
                flake_input_name,
                flake_input_value,
                flake_contents,
                expr.span(),
                inputs_attr,
                outputs_attr,
            )
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn find_attrset(
    expr: &nixel::Expression,
    attr_path: Option<VecDeque<String>>,
) -> color_eyre::Result<Option<nixel::BindingKeyValue>> {
    match expr {
        nixel::Expression::Map(map) => {
            for binding in map.bindings.iter() {
                match binding {
                    nixel::Binding::KeyValue(kv) => {
                        if let Some(ref attr_path) = attr_path {
                            // Transform `inputs.nixpkgs.url` into `["inputs", "nixpkgs", "url"]`
                            let mut this_attr_path: VecDeque<(String, &nixel::PartRaw)> = kv
                                .from
                                .iter()
                                .filter_map(|attr| match attr {
                                    nixel::Part::Raw(raw) => Some((raw.content.to_string(), raw)),
                                    _ => None,
                                })
                                .collect();

                            let mut search_attr_path = attr_path.clone();
                            let mut most_recent_attr_matched = false;

                            // Find the correct attr path to modify
                            while let Some(attr1) = search_attr_path.pop_front() {
                                if let Some((attr2, attr2_raw)) = this_attr_path.pop_front() {
                                    // For every key in the attr path we're searching for we check that
                                    // we have a matching attr key in the current attrset.
                                    if attr1 != attr2 {
                                        most_recent_attr_matched = false;

                                        // We want `this_attr_path` to contain all the attr path keys
                                        // that didn't match the attr path we're looking for, so we can
                                        // know when it matched as many of the attr paths as possible
                                        // (when `this_attr_path` is empty).
                                        this_attr_path.push_front((attr2, attr2_raw));
                                    } else {
                                        most_recent_attr_matched = true;
                                    }
                                } else {
                                    most_recent_attr_matched = false;

                                    // If it doesn't match, that means this isn't the correct attr path,
                                    // so we re-add the unmatched attr to `search_attr_path`...
                                    search_attr_path.push_front(attr1);

                                    // ...and break out to preserve all unmatched attrs.
                                    break;
                                }
                            }

                            // If `most_recent_attr_matched` is true, that means we've found the
                            // attr we want! Probably.
                            if most_recent_attr_matched {
                                return Ok(Some(kv.to_owned()));
                            }

                            // If `this_attr_path` is empty, that means we've matched as much of the
                            // attr path as we can of this key node, and thus we need to recurse into
                            // its value node to continue checking if we want this input or not.
                            if this_attr_path.is_empty() {
                                return find_attrset(&kv.to, Some(search_attr_path));
                            }
                        } else {
                            return Ok(Some(kv.to_owned()));
                        }
                    }
                    nixel::Binding::Inherit(inherit) => {
                        let start = &inherit.span.start;
                        return Err(color_eyre::eyre::eyre!(
                            "`inherit` not supported (at {}:{})",
                            start.line,
                            start.column
                        ));
                    }
                }
            }
        }
        t => {
            let start = t.start();
            return Err(color_eyre::eyre::eyre!(
                "unsupported expression type {} (at {}:{})",
                t.variant_name(),
                start.line,
                start.column
            ));
        }
    }

    Ok(None)
}

pub(crate) enum AttrType {
    Inputs(nixel::BindingKeyValue),
    Outputs(nixel::BindingKeyValue),
    MissingInputs((nixel::Span, nixel::Span)),
    MissingOutputs((nixel::Span, nixel::Span)),
    MissingInputsAndOutputs(nixel::Span),
}

impl AttrType {
    pub(crate) fn process(
        self,
        flake_contents: &str,
        flake_input_name: &str,
        flake_input_value: &url::Url,
    ) -> color_eyre::Result<String> {
        match self {
            AttrType::Inputs(ref inputs_attr) => {
                match inputs_attr.from.len() {
                    // inputs = { nixpkgs.url = ""; };
                    1 => {
                        let first_input = find_attrset(&inputs_attr.to, None)?.expect("");
                        let (from_span, _to_span) = Self::kv_to_span(&first_input);
                        let flake_input =
                            format!(r#"{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#);
                        AttrType::insert_input(from_span, flake_contents, &flake_input, false)
                    }

                    // inputs.nixpkgs = { url = ""; inputs.something.follows = ""; };
                    // OR
                    // inputs.nixpkgs.url = "";
                    2 | 3 => {
                        let (from_span, _to_span) = self.span();

                        let flake_input = format!(
                            r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#
                        );
                        AttrType::insert_input(from_span, flake_contents, &flake_input, false)
                    }

                    // I don't think this is possible, but just in case.
                    len => {
                        tracing::warn!(
                            "input's attrpath had unexpected length {len}; \
                            please file a bug report at https://github.com/DeterminateSystems/fh with as much detail as possible"
                        );

                        let (from_span, _to_span) = self.span();

                        let flake_input = format!(
                            r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#
                        );
                        AttrType::insert_input(from_span, flake_contents, &flake_input, false)
                    }
                }
            }
            AttrType::Outputs(ref outputs_attr) => {
                let (from_span, to_span) = self.span();
                match &*outputs_attr.to {
                    nixel::Expression::Function(f) => match &f.head {
                        // outputs = { self, ... } @ inputs: { }
                        nixel::FunctionHead::Destructured(head) => {
                            AttrType::insert_input_name_into_outputs_function(
                                flake_input_name,
                                head,
                                from_span,
                                to_span,
                                flake_contents,
                            )
                        }
                        // outputs = inputs: { }
                        nixel::FunctionHead::Simple(_) => {
                            // Nothing to do
                            Ok(flake_contents.to_string())
                        }
                    },
                    t => {
                        let start = t.start();
                        Err(color_eyre::eyre::eyre!(
                            "unsupported `outputs` expression type {} (at {}:{})",
                            t.variant_name(),
                            start.line,
                            start.column
                        ))
                    }
                }
            }
            AttrType::MissingInputs((outputs_span_from, _outputs_span_to)) => {
                // If we're not adding our new input above an existing `inputs` construct, let's add
                // another newline so that it looks nicer.
                let flake_input = format!(
                    r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}{NEWLINE}"#
                );

                AttrType::insert_input(
                    outputs_span_from,
                    flake_contents,
                    &flake_input,
                    true, // we need to adjust the line numbers since we added an extra newline above
                )
            }
            AttrType::MissingOutputs((_inputs_span_from, _inputs_span_to)) => {
                // I don't really want to give them an `outputs` if it doesn't already exist, but
                // I've laid out the groundwork that it would be possible...
                Err(color_eyre::eyre::eyre!(
                    "flake was missing an `outputs` attribute"
                ))?
            }
            AttrType::MissingInputsAndOutputs(_root_span) => {
                // I don't really want to deal with a flake that has no `inputs` or `outputs`
                // either, but again, I've laid the groundwork to do so...
                // If we do decide to suppor this, the simplest way would be: insert at the root
                // span (\\n, then 2 spaces, then write inputs, don't care about outputs for now?)
                Err(color_eyre::eyre::eyre!(
                    "flake was missing both the `inputs` and `outputs` attributes"
                ))?
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert_input(
        span: nixel::Span,
        flake_contents: &str,
        flake_input: &str,
        added_cosmetic_newline: bool,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();
        let (start, _) = span_to_start_end_offsets(flake_contents, &span)?;

        new_flake_contents.insert_str(start, flake_input);

        let old_content_start_of_indentation_pos = nixel::Position {
            line: span.start.line,
            column: 1,
        };
        let old_content_pos = nixel::Position {
            // we moved the old contents to the next line...
            line: span.start.line + 1 + if added_cosmetic_newline { 1 } else { 0 },
            // ...at the very beginning
            column: 1,
        };
        let old_content_end_of_indentation_pos = span.start;

        let indentation_span = nixel::Span {
            start: Box::new(old_content_start_of_indentation_pos),
            end: old_content_end_of_indentation_pos,
        };
        let (indentation_start, indentation_end) =
            span_to_start_end_offsets(flake_contents, &indentation_span)?;
        let indentation = &flake_contents[indentation_start..indentation_end];

        let offset = position_to_offset(&new_flake_contents, &old_content_pos)?;

        new_flake_contents.insert_str(offset, indentation);

        Ok(new_flake_contents)
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert_input_name_into_outputs_function(
        flake_input_name: &str,
        head: &nixel::FunctionHeadDestructured,
        from_span: nixel::Span,
        to_span: nixel::Span,
        flake_contents: &str,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();

        if head
            .arguments
            .iter()
            .any(|arg| &*arg.identifier == flake_input_name)
        {
            tracing::warn!("input {flake_input_name} was already in the `outputs` function args, not adding it again");
            return Ok(new_flake_contents);
        }

        let final_named_arg = head.arguments.last();

        // TODO: try to match the style of multiline function args (will be difficult because we
        // don't get span information for each input arg...)
        // let multiline_args = from_span.start.line != to_span.end.line;

        let start = position_to_offset(flake_contents, &from_span.start)?;
        let end = position_to_offset(flake_contents, &to_span.end)?;
        let mut span_text = String::from(&flake_contents[start..end]);

        new_flake_contents.replace_range(start..end, "");

        match final_named_arg {
            Some(arg) => {
                let final_arg_identifier = &arg.identifier;
                let re = regex::Regex::new(&format!(
                    "[^[:space:],]*{final_arg_identifier}[^[:space:],]*"
                ))?; // final_arg_identifier made pattern invalid?

                if let Some(found) = re.find(&span_text) {
                    span_text.insert_str(found.end(), &format!(", {flake_input_name}"));
                    new_flake_contents.insert_str(start, &span_text);
                } else {
                    return Err(color_eyre::eyre::eyre!(
                    "could not find `{final_arg_identifier}` in the outputs function, but it existed when parsing it"
                ))?;
                }
            }
            None => {
                if head.ellipsis {
                    // The ellipsis can _ONLY_ appear at the end of the set,
                    // never the beginning, so it's safe to insert `<name>, `
                    let re = regex::Regex::new(r"[^[:space:],]*\.\.\.[^[:space:],]*")?;

                    if let Some(found) = re.find(&span_text) {
                        span_text.insert_str(found.start(), &format!("{flake_input_name}, "));
                        new_flake_contents.insert_str(start, &span_text);
                    } else {
                        return Err(color_eyre::eyre::eyre!(
                        "could not find the ellipsis (`...`) in the outputs function, but it existed when parsing it"
                    ))?;
                    }
                } else {
                    // unfortunately this is legal, but I don't wanna support it
                    return Err(color_eyre::eyre::eyre!("the `outputs` function doesn't take any arguments, and fh add doesn't support that yet. Replace it with: outputs = {{ ... }}: and try again."))?;
                }
            }
        }

        Ok(new_flake_contents)
    }

    #[tracing::instrument(skip_all)]
    fn kv_to_span(kv: &nixel::BindingKeyValue) -> (nixel::Span, nixel::Span) {
        (
            kv.from
                .iter()
                .next()
                .map(|v| v.span())
                .expect("attr existed, thus it must have a span"),
            kv.to.span(),
        )
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn span(&self) -> (nixel::Span, nixel::Span) {
        match self {
            AttrType::Inputs(kv) | AttrType::Outputs(kv) => Self::kv_to_span(kv),
            AttrType::MissingInputs(_)
            | AttrType::MissingOutputs(_)
            | AttrType::MissingInputsAndOutputs(_) => todo!(),
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn upsert_into_inputs_and_outputs(
    flake_input_name: String,
    flake_input_value: url::Url,
    mut flake_contents: String,
    root_span: nixel::Span,
    inputs_attr: Option<nixel::BindingKeyValue>,
    outputs_attr: Option<nixel::BindingKeyValue>,
) -> color_eyre::Result<String> {
    let inputs_attr = inputs_attr.map(AttrType::Inputs);
    let outputs_attr = outputs_attr.map(AttrType::Outputs);
    let (first_attr_to_process, second_attr_to_process) = match (inputs_attr, outputs_attr) {
        (Some(inputs_attr), Some(outputs_attr)) => {
            let (inputs_span, _) = inputs_attr.span();
            let (outputs_span, _) = outputs_attr.span();

            // If the `inputs` attrset occurs earlier in the file, we want to edit it later, so that
            // we don't have to re-parse the entire expression (if we instead started with the
            // `inputs` attrset, the offset into the file to later edit the `outputs` function would
            // be wrong).
            if inputs_span.start.line < outputs_span.start.line {
                (Some(outputs_attr), Some(inputs_attr))
            } else {
                (Some(inputs_attr), Some(outputs_attr))
            }
        }
        (Some(inputs_attr), None) => {
            let other = AttrType::MissingOutputs(inputs_attr.span());
            (Some(inputs_attr), Some(other))
        }
        (None, Some(outputs_attr)) => {
            let other = AttrType::MissingInputs(outputs_attr.span());
            (Some(outputs_attr), Some(other))
        }
        _ => (Some(AttrType::MissingInputsAndOutputs(root_span)), None),
    };

    if let Some(first_attr_to_process) = first_attr_to_process {
        flake_contents = first_attr_to_process.process(
            &flake_contents,
            &flake_input_name,
            &flake_input_value,
        )?;
    }
    if let Some(second_attr_to_process) = second_attr_to_process {
        flake_contents = second_attr_to_process.process(
            &flake_contents,
            &flake_input_name,
            &flake_input_value,
        )?;
    }

    Ok(flake_contents)
}

#[tracing::instrument(skip_all)]
pub(crate) fn replace_input_value(
    parts: &[nixel::Part],
    flake_input_value: &url::Url,
    flake_contents: &str,
) -> color_eyre::Result<String> {
    let mut parts_iter = parts.iter();
    let mut new_flake_contents = flake_contents.to_string();

    if let Some(part) = parts_iter.next() {
        match part {
            nixel::Part::Raw(raw) => {
                let (start, end) = span_to_start_end_offsets(flake_contents, &raw.span)?;

                // Replace the current contents with nothingness
                new_flake_contents.replace_range(start..end, "");
                // Insert the new contents
                new_flake_contents.insert_str(start, flake_input_value.as_ref());
            }
            part => {
                let start = part.start();
                return Err(color_eyre::eyre::eyre!(
                    "unexpected expression or interpolation (at {}:{})",
                    start.line,
                    start.column
                ));
            }
        }
    }

    // idk when this list of parts could have more than 1.... (maybe just a side-effect of the
    // bindgen code generation?)
    if parts_iter.next().is_some() {
        return Err(color_eyre::eyre::eyre!(
            "Nix string had multiple parts -- please report this and include the flake.nix that triggered this!"
        ));
    }

    Ok(new_flake_contents)
}

#[tracing::instrument(skip_all)]
pub(crate) fn span_to_start_end_offsets(
    flake_contents: &str,
    span: &nixel::Span,
) -> color_eyre::Result<(usize, usize)> {
    let start = &*span.start;
    let end = &*span.end;

    Ok((
        position_to_offset(flake_contents, start)?,
        position_to_offset(flake_contents, end)?,
    ))
}

#[tracing::instrument(skip_all)]
pub(crate) fn position_to_offset(
    flake_contents: &str,
    position: &nixel::Position,
) -> color_eyre::Result<usize> {
    let mut column = 1;
    let mut line = 1;

    for (idx, ch) in flake_contents.char_indices() {
        if column == position.column && line == position.line {
            return Ok(idx);
        }

        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    Err(color_eyre::eyre::eyre!(
        "could not find {}:{} in input",
        position.line,
        position.column
    ))
}

#[cfg(test)]
mod test {
    #[test]
    fn test_flake_1_rewrite_less_simple_flake_input() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake1.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            *parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        let updated_nixpkgs_input = res.lines().find(|line| line.contains(input_value.as_str()));
        assert!(updated_nixpkgs_input.is_some());

        let updated_nixpkgs_input = updated_nixpkgs_input.unwrap().trim();
        assert_eq!(
            updated_nixpkgs_input,
            "nixpkgs.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );
    }

    #[test]
    fn test_flake_2_rewrite_simple_flake_input() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake2.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            *parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        let updated_nixpkgs_input = res.lines().find(|line| line.contains(input_value.as_str()));
        assert!(updated_nixpkgs_input.is_some());

        let updated_nixpkgs_input = updated_nixpkgs_input.unwrap().trim();
        assert_eq!(
            updated_nixpkgs_input,
            "inputs.nixpkgs.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );
    }

    #[test]
    fn test_flake_3_rewriting_various_input_formats() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake3.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let parsed = nixel::parse(flake_contents.clone());

        for input in ["nixpkgs1", "nixpkgs2", "nixpkgs3"] {
            let input_name = input.to_string();
            let input_value =
                url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();

            let res = super::upsert_flake_input(
                *parsed.expression.clone(),
                input_name.clone(),
                input_value.clone(),
                flake_contents.clone(),
                ["inputs", &input_name, "url"]
                    .map(ToString::to_string)
                    .into(),
            );
            assert!(res.is_ok());

            let res = res.unwrap();
            let updated_nixpkgs_input =
                res.lines().find(|line| line.contains(input_value.as_str()));
            assert!(updated_nixpkgs_input.is_some());
        }
    }

    #[test]
    fn test_flake_4_add_new_input_before_existing_outputs() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake4.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            *parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        let updated_nixpkgs_input = res.lines().find(|line| line.contains(input_value.as_str()));
        assert!(updated_nixpkgs_input.is_some());

        let updated_nixpkgs_input = updated_nixpkgs_input.unwrap().trim();
        assert_eq!(
            updated_nixpkgs_input,
            "inputs.nixpkgs.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );

        let inputs_line_idx = res
            .lines()
            .enumerate()
            .find_map(|(idx, line)| {
                if line.contains("inputs") {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap();
        let outputs_line_idx = res
            .lines()
            .enumerate()
            .find_map(|(idx, line)| {
                if line.contains("outputs") {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(
            inputs_line_idx < outputs_line_idx,
            "`inputs` should have been inserted above `outputs`"
        );

        // (there's no other `inputs`, so we insert a cosmetic newline, instead of having it right
        // on top of `outputs`)
        assert_eq!(
            inputs_line_idx + 2,
            outputs_line_idx,
            "`inputs` should have been inserted exactly 2 lines above `outputs`"
        );
    }

    #[test]
    fn test_flake_5_insert_input_into_stylized_inputs_attrs() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake5.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs-new");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            *parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        let updated_nixpkgs_input = res.lines().find(|line| line.contains(input_value.as_str()));
        assert!(updated_nixpkgs_input.is_some());

        let updated_nixpkgs_input = updated_nixpkgs_input.unwrap().trim();
        assert_eq!(
            updated_nixpkgs_input,
            "nixpkgs-new.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );

        let updated_outputs = res.lines().find(|line| line.contains("outputs"));
        assert!(updated_outputs.is_some());

        let updated_outputs = updated_outputs.unwrap().trim();
        assert_eq!(
            updated_outputs,
            "outputs = { self, nixpkgs-new, ... } @ tes: { };"
        );
    }

    #[test]
    fn test_flake_6_doesnt_duplicate_outputs_args() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake6.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            *parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        let updated_nixpkgs_input = res.lines().find(|line| line.contains(input_value.as_str()));
        assert!(updated_nixpkgs_input.is_some());

        let updated_nixpkgs_input = updated_nixpkgs_input.unwrap().trim();
        assert_eq!(
            updated_nixpkgs_input,
            "inputs.nixpkgs.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );

        let updated_outputs = res.lines().find(|line| line.contains("outputs"));
        assert!(updated_outputs.is_some());

        let updated_outputs = updated_outputs.unwrap().trim();
        assert_eq!(
            updated_outputs,
            "outputs = { self, nixpkgs, ... } @ tes: { };"
        );
    }
}
