use std::collections::VecDeque;

use tracing::{span, Level};

const NEWLINE: &str = "\n";

#[tracing::instrument(skip_all)]
pub(crate) fn upsert_flake_input(
    expr: &nixel::Expression,
    flake_input_name: String,
    flake_input_value: url::Url,
    flake_contents: String,
    input_attr_path: VecDeque<String>,
    inputs_insertion_location: InputsInsertionLocation,
) -> color_eyre::Result<String> {
    match find_first_attrset_by_path(expr, Some(input_attr_path))? {
        Some(attr) => update_flake_input(attr, flake_input_name, flake_input_value, flake_contents),
        None => insert_flake_input(
            expr,
            flake_input_name,
            flake_input_value,
            flake_contents,
            inputs_insertion_location,
        ),
    }
}

pub(crate) fn update_flake_input(
    attr: nixel::BindingKeyValue,
    flake_input_name: String,
    flake_input_value: url::Url,
    flake_contents: String,
) -> color_eyre::Result<String> {
    match *attr.to {
        nixel::Expression::String(existing_input_value) => replace_input_value_string(
            &existing_input_value.parts,
            &flake_input_value,
            &flake_contents,
        ),
        nixel::Expression::IndentedString(existing_input_value) => replace_input_value_string(
            &existing_input_value.parts,
            &flake_input_value,
            &flake_contents,
        ),
        nixel::Expression::Uri(existing_input_value) => {
            replace_input_value_uri(&existing_input_value, &flake_input_value, &flake_contents)
        }
        otherwise => {
            // a boolean, a number, or even another attrset, etc.
            Err(color_eyre::eyre::eyre!(
                "`inputs.{flake_input_name}.url` was not a String, Indented String, or URI. Instead: {:?}", // this is enforced by Nix itself
                otherwise
            ))
        }
    }
}

pub(crate) fn insert_flake_input(
    expr: &nixel::Expression,
    flake_input_name: String,
    flake_input_value: url::Url,
    flake_contents: String,
    inputs_insertion_location: InputsInsertionLocation,
) -> color_eyre::Result<String> {
    let inputs_attr_path: VecDeque<String> = [String::from("inputs")].into();
    let outputs_attr_path: VecDeque<String> = [String::from("outputs")].into();

    let inputs_attr = match inputs_insertion_location {
        InputsInsertionLocation::Top => find_first_attrset_by_path(expr, Some(inputs_attr_path))?,
        InputsInsertionLocation::Bottom => {
            let all_toplevel_inputs = find_all_attrsets_by_path(expr, Some(inputs_attr_path))?;
            let all_inputs = collect_all_inputs(all_toplevel_inputs)?;
            all_inputs.into_iter().last()
        }
    };

    let outputs_attr = find_first_attrset_by_path(expr, Some(outputs_attr_path))?;

    upsert_into_inputs_and_outputs(
        flake_input_name,
        flake_input_value,
        flake_contents,
        expr.span(),
        inputs_attr,
        outputs_attr,
        inputs_insertion_location,
    )
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_all_inputs(
    all_toplevel_inputs: Vec<nixel::BindingKeyValue>,
) -> color_eyre::Result<Vec<nixel::BindingKeyValue>> {
    let mut all_inputs = Vec::new();

    for v in all_toplevel_inputs {
        let span = span!(Level::DEBUG, "collecting_input");
        let _guard = span.enter();

        let name_parts = v
            .from
            .iter()
            // Deliberately not filter_map, because if any of the values aren't Raw, we want to skip the whole "input"
            .map(|from| match from {
                nixel::Part::Raw(p) => Some(&*p.content),
                _ => None,
            })
            .collect::<Option<Vec<&str>>>();
        let name_parts = match name_parts {
            Some(n) => n,
            None => {
                tracing::trace!("skipped input because we didn't get Raw parts");
                continue;
            }
        };

        let _match_guard = span!(
            parent: &span,
            Level::DEBUG,
            "examining input",
            "{:?}",
            name_parts
        )
        .entered();

        match name_parts[..] {
            ["inputs"] => {
                all_inputs.extend(find_all_attrsets_by_path(&v.to, None)?);
            }
            ["inputs", name] => {
                tracing::trace!("Identified input.{name} = ...");
                all_inputs.push(v);
            }
            ["inputs", name, "url"] => {
                tracing::trace!("Identified input.{name}.url = ...");
                all_inputs.push(v);
            }
            _ => {
                tracing::debug!("Skipping processing: {:?}", name_parts);
            }
        }
    }

    Ok(all_inputs)
}

#[tracing::instrument(skip_all)]
pub(crate) fn find_first_attrset_by_path(
    expr: &nixel::Expression,
    attr_path: Option<VecDeque<String>>,
) -> color_eyre::Result<Option<nixel::BindingKeyValue>> {
    // While this may be more expensive when we only care about the first thing it returns, it
    // decreases maintenance burden by keeping these two functions using the same implementation
    // under the hood.
    Ok(find_all_attrsets_by_path(expr, attr_path)?
        .into_iter()
        .next())
}

#[tracing::instrument(skip_all)]
pub(crate) fn find_all_attrsets_by_path(
    expr: &nixel::Expression,
    attr_path: Option<VecDeque<String>>,
) -> color_eyre::Result<Vec<nixel::BindingKeyValue>> {
    let mut found_kvs = Vec::new();

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
                                found_kvs.push(kv.to_owned());
                                continue;
                            }

                            // If `this_attr_path` is empty, that means we've matched as much of the
                            // attr path as we can of this key node, and thus we need to recurse into
                            // its value node to continue checking if we want this input or not.
                            if this_attr_path.is_empty() {
                                found_kvs.extend(find_all_attrsets_by_path(
                                    &kv.to,
                                    Some(search_attr_path),
                                )?);
                                continue;
                            }
                        } else {
                            found_kvs.push(kv.to_owned());
                            continue;
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

    Ok(found_kvs)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum InputsInsertionLocation {
    /// The new input will be inserted at the top (either above all other `inputs`, or as the first input inside of `inputs = { ... }`)
    Top,
    /// The new input will be inserted at the bottom (either below all other `inputs`, or as the last input inside of `inputs = { ... }`)
    Bottom,
}

impl std::fmt::Display for InputsInsertionLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputsInsertionLocation::Top => f.write_str("top"),
            InputsInsertionLocation::Bottom => f.write_str("bottom"),
        }
    }
}

impl std::str::FromStr for InputsInsertionLocation {
    type Err = color_eyre::Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "top" => InputsInsertionLocation::Top,
            "bottom" | "🥺" => InputsInsertionLocation::Bottom,
            _ => {
                return Err(color_eyre::eyre::eyre!(
                    "only `top` and `bottom` are valid insertion locations"
                ))
            }
        })
    }
}

#[derive(Debug)]
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
        insertion_location: InputsInsertionLocation,
    ) -> color_eyre::Result<String> {
        match self {
            AttrType::Inputs(ref inputs_attr) => {
                match inputs_attr.from.len() {
                    // inputs = { nixpkgs.url = ""; };
                    1 => {
                        let flake_input =
                            format!(r#"{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#);

                        match insertion_location {
                            InputsInsertionLocation::Top => {
                                let first_input =
                                    find_first_attrset_by_path(&inputs_attr.to, None)?
                                        .expect("there must be a first input");
                                let (from_span, _to_span) = kv_to_span(&first_input);

                                self.insert_input(from_span, None, flake_contents, &flake_input)
                            }
                            InputsInsertionLocation::Bottom => {
                                let (from_span, to_span) = self.span();

                                self.insert_input(
                                    from_span,
                                    Some(to_span),
                                    flake_contents,
                                    &flake_input,
                                )
                            }
                        }
                    }

                    // inputs.nixpkgs = { url = ""; inputs.something.follows = ""; };
                    // OR
                    // inputs.nixpkgs.url = "";
                    // OR
                    // inputs.nixpkgs.inputs.something.follows = "";
                    // etc...
                    _len => {
                        let (from_span, to_span) = self.span();
                        let flake_input = format!(
                            r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#
                        );

                        match insertion_location {
                            InputsInsertionLocation::Top => {
                                self.insert_input(from_span, None, flake_contents, &flake_input)
                            }
                            InputsInsertionLocation::Bottom => self.insert_input(
                                from_span,
                                Some(to_span),
                                flake_contents,
                                &flake_input,
                            ),
                        }
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
            AttrType::MissingInputs((ref outputs_span_from, ref _outputs_span_to)) => {
                let flake_input =
                    format!(r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#);

                self.insert_input(
                    outputs_span_from.clone(),
                    None,
                    flake_contents,
                    &flake_input,
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
                // If we do decide to support this, the simplest way would be: insert at the root
                // span (\\n, then 2 spaces, then write inputs, don't care about outputs for now?)
                Err(color_eyre::eyre::eyre!(
                    "flake was missing both the `inputs` and `outputs` attributes"
                ))?
            }
        }
    }

    #[tracing::instrument(skip_all)]
    pub(crate) fn insert_input(
        &self,
        from_span: nixel::Span,
        to_span: Option<nixel::Span>,
        flake_contents: &str,
        flake_input: &str,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();

        let indentation = indentation_from_from_span(flake_contents, &from_span)?;

        let line = if let Some(to_span) = to_span {
            to_span.end.line + 1
        } else {
            from_span.start.line
        };
        let old_content_pos = nixel::Position { line, column: 1 };
        let offset = position_to_offset(&new_flake_contents, &old_content_pos)?;

        let mut input = format!("{indentation}{flake_input}");

        // If we're not adding our new input above or below an existing `inputs` construct, let's
        // add another newline so that it looks nicer.
        let add_cosmetic_newline = matches!(self, AttrType::MissingInputs(_));
        if add_cosmetic_newline {
            input.push_str(NEWLINE);
        }

        new_flake_contents.insert_str(offset, &input);

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
            tracing::debug!("input {flake_input_name} was already in the `outputs` function args, not adding it again");
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
    pub(crate) fn span(&self) -> (nixel::Span, nixel::Span) {
        match self {
            AttrType::Inputs(kv) | AttrType::Outputs(kv) => kv_to_span(kv),
            AttrType::MissingInputs(_)
            | AttrType::MissingOutputs(_)
            | AttrType::MissingInputsAndOutputs(_) => todo!(),
        }
    }
}

pub(crate) fn indentation_from_from_span<'a>(
    flake_contents: &'a str,
    from_span: &nixel::Span,
) -> color_eyre::Result<&'a str> {
    let old_content_start_of_indentation_pos = nixel::Position {
        line: from_span.start.line,
        column: 1,
    };
    let old_content_end_of_indentation_pos = from_span.start.clone();

    let indentation_span = nixel::Span {
        start: Box::new(old_content_start_of_indentation_pos),
        end: old_content_end_of_indentation_pos,
    };
    let (indentation_start, indentation_end) =
        span_to_start_end_offsets(flake_contents, &indentation_span)?;
    let indentation = &flake_contents[indentation_start..indentation_end];

    Ok(indentation)
}

#[tracing::instrument(skip_all)]
pub(crate) fn kv_to_span(kv: &nixel::BindingKeyValue) -> (nixel::Span, nixel::Span) {
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
pub(crate) fn upsert_into_inputs_and_outputs(
    flake_input_name: String,
    flake_input_value: url::Url,
    mut flake_contents: String,
    root_span: nixel::Span,
    inputs_attr: Option<nixel::BindingKeyValue>,
    outputs_attr: Option<nixel::BindingKeyValue>,
    insertion_location: InputsInsertionLocation,
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
            insertion_location,
        )?;
    }
    if let Some(second_attr_to_process) = second_attr_to_process {
        flake_contents = second_attr_to_process.process(
            &flake_contents,
            &flake_input_name,
            &flake_input_value,
            insertion_location,
        )?;
    }

    Ok(flake_contents)
}

#[tracing::instrument(skip_all)]
pub(crate) fn replace_input_value_string(
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
pub(crate) fn replace_input_value_uri(
    uri: &nixel::Uri,
    flake_input_value: &url::Url,
    flake_contents: &str,
) -> color_eyre::Result<String> {
    let mut new_flake_contents = flake_contents.to_string();

    let (start, end) = span_to_start_end_offsets(flake_contents, &uri.span)?;
    // Replace the current contents with nothingness
    new_flake_contents.replace_range(start..end, "");
    // Insert the new contents
    new_flake_contents.insert_str(start, &format!(r#""{}""#, flake_input_value.as_ref()));

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
    use super::InputsInsertionLocation;

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
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Top,
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
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Top,
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
                &parsed.expression.clone(),
                input_name.clone(),
                input_value.clone(),
                flake_contents.clone(),
                ["inputs", &input_name, "url"]
                    .map(ToString::to_string)
                    .into(),
                InputsInsertionLocation::Top,
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
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Top,
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
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Top,
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
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Top,
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

    #[test]
    fn test_flake_7_inserts_at_the_bottom() {
        let flake_contents = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/samples/flake7.test.nix"
        ));
        let flake_contents = flake_contents.to_string();
        let input_name = String::from("nixpkgs-new");
        let input_value =
            url::Url::parse("https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz").unwrap();
        let parsed = nixel::parse(flake_contents.clone());

        let res = super::upsert_flake_input(
            &parsed.expression,
            input_name.clone(),
            input_value.clone(),
            flake_contents,
            ["inputs", &input_name, "url"]
                .map(ToString::to_string)
                .into(),
            InputsInsertionLocation::Bottom,
        );
        assert!(res.is_ok());

        let res = res.unwrap();
        eprintln!("{res}");
        let nixpkgs_input = res.lines().enumerate().find_map(|(idx, line)| {
            if line.contains(input_value.as_str()) {
                Some((idx, line))
            } else {
                None
            }
        });
        assert!(nixpkgs_input.is_some());
        let Some((nixpkgs_input_idx, nixpkgs_input)) = nixpkgs_input else {
            unreachable!();
        };

        let updated_nixpkgs_input = nixpkgs_input.trim();
        assert_eq!(
            updated_nixpkgs_input,
            "nixpkgs-new.url = \"https://flakehub.com/f/NixOS/nixpkgs/0.2305.*.tar.gz\";"
        );

        let wezterm_line_idx = res
            .lines()
            .enumerate()
            .find_map(|(idx, line)| {
                if line.contains("wezterm") {
                    Some(idx)
                } else {
                    None
                }
            })
            .unwrap();

        assert!(wezterm_line_idx < nixpkgs_input_idx, "when inserting at the bottom, the new nixpkgs input should have come after the wezterm input");
    }
}
