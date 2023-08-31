use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use super::CommandExecute;

const NEWLINE: &str = "\n";

/// Adds a flake input to your flake.nix.
#[derive(Parser)]
pub(crate) struct AddSubcommand {
    /// The flake.nix to modify.
    #[clap(long, default_value = "./flake.nix")]
    pub(crate) flake_path: PathBuf,
    /// The name of the flake input.
    ///
    /// If not provided, it will be inferred from the provided input URL (if possible).
    #[clap(long)]
    pub(crate) input_name: Option<String>,
    /// The flake reference to add as an input.
    ///
    /// A reference in the form of `NixOS/nixpkgs` or `NixOS/nixpkgs/0.2305.*` (without a URL
    /// scheme) will be inferred as a FlakeHub input.
    pub(crate) input_ref: String,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for AddSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let flake_contents = tokio::fs::read_to_string(&self.flake_path).await?;
        let parsed = nixel::parse(flake_contents.clone());
        let (flake_input_name, flake_input_url) =
            infer_flake_input_name_url(self.api_addr, self.input_ref, self.input_name).await?;
        let input_url_attr_path: VecDeque<String> = [
            String::from("inputs"),
            flake_input_name.clone(),
            String::from("url"),
        ]
        .into();

        let new_flake_contents = upsert_flake_input(
            *parsed.expression,
            flake_input_name,
            flake_input_url,
            flake_contents,
            input_url_attr_path,
        )?;

        tokio::fs::write(self.flake_path, new_flake_contents).await?;

        Ok(ExitCode::SUCCESS)
    }
}

async fn infer_flake_input_name_url(
    api_addr: url::Url,
    flake_ref: String,
    input_name: Option<String>,
) -> color_eyre::Result<(String, url::Url)> {
    let url_result = flake_ref.parse::<url::Url>();

    match url_result {
        // A URL like `github:nixos/nixpkgs`
        Ok(parsed_url) if parsed_url.host().is_none() => {
            // TODO: validate that the format of all Nix-supported schemes allows us to do this;
            // else, have an allowlist of schemes
            let mut path_parts = parsed_url.path().split('/');
            path_parts.next(); // e.g. in `fh:` or `github:`, the org name

            match (input_name, path_parts.next()) {
                (Some(input_name), _) => Ok((input_name, parsed_url)),
                (None, Some(input_name)) => Ok((input_name.to_string(), parsed_url)),
                (None, _) =>  Err(color_eyre::eyre::eyre!(
                    "cannot infer an input name for {parsed_url}; please specify one with the `--input-name` flag"
                ))
            }
        }
        // A URL like `nixos/nixpkgs` or `nixos/nixpkgs/0.2305`
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let (org, repo, version) = match flake_ref.split('/').collect::<Vec<_>>()[..] {
                // `nixos/nixpkgs/0.2305`
                [org, repo, version] => {
                    let version = version.strip_suffix(".tar.gz").unwrap_or(version);
                    let version = version.strip_prefix('v').unwrap_or(version);

                    (org, repo, Some(version))
                }
                // `nixos/nixpkgs`
                [org, repo] => {
                    (org, repo, None)
                }
                _ => Err(color_eyre::eyre::eyre!(
                    "flakehub input did not match the expected format of `org/repo` or `org/repo/version`"
                ))?,
            };

            let (flakehub_input, url) =
                get_flakehub_repo_and_url(api_addr, org, repo, version).await?;

            if let Some(input_name) = input_name {
                Ok((input_name, url))
            } else {
                Ok((flakehub_input, url))
            }
        }
        // A URL like `https://flakehub.com/f/NixOS/nixpkgs/*.tar.gz`
        Ok(parsed_url) => {
            if let Some(input_name) = input_name {
                Ok((input_name, parsed_url))
            } else {
                Err(color_eyre::eyre::eyre!(
                    "cannot infer an input name for `{flake_ref}`; please specify one with the `--input-name` flag"
                ))?
            }
        }
        Err(e) => Err(e)?,
    }
}

async fn get_flakehub_repo_and_url(
    api_addr: url::Url,
    org: &str,
    repo: &str,
    version: Option<&str>,
) -> color_eyre::Result<(String, url::Url)> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Accept",
        reqwest::header::HeaderValue::from_static("application/json"),
    );

    let client = reqwest::Client::builder()
        .user_agent(crate::APP_USER_AGENT)
        .default_headers(headers)
        .build()?;

    let mut flakehub_json_url = api_addr.clone();
    {
        let mut path_segments_mut = flakehub_json_url
            .path_segments_mut()
            .expect("flakehub url cannot be base (this should never happen)");

        match version {
            Some(version) => {
                path_segments_mut
                    .push("version")
                    .push(org)
                    .push(repo)
                    .push(version);
            }
            None => {
                path_segments_mut.push("f").push(org).push(repo);
            }
        }
    }

    #[derive(Debug, serde_derive::Deserialize)]
    struct ProjectCanonicalNames {
        project: String,
        // FIXME: detect Nix version and strip .tar.gz if it supports it
        pretty_download_url: url::Url,
    }

    let res = client.get(&flakehub_json_url.to_string()).send().await?;

    if res.status().is_success() {
        let res = res.json::<ProjectCanonicalNames>().await?;

        Ok((res.project, res.pretty_download_url))
    } else {
        Err(color_eyre::eyre::eyre!(res.text().await?))
    }
}

fn upsert_flake_input(
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

fn find_attrset<'a>(
    expr: &'a nixel::Expression,
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

enum AttrType {
    Inputs(nixel::BindingKeyValue),
    Outputs(nixel::BindingKeyValue),
    MissingInputs((nixel::Span, nixel::Span)),
    MissingOutputs((nixel::Span, nixel::Span)),
    MissingInputsAndOutputs(nixel::Span),
}

impl AttrType {
    fn process(
        self,
        flake_contents: &str,
        flake_input_name: &str,
        flake_input_value: &url::Url,
    ) -> color_eyre::Result<String> {
        let mut added_cosmetic_newline = false;

        // We don't do anything fancy like trying to match the existing format of e.g.
        // `inputs = { <input_name>.url = "<input_value>"; };`
        let mut flake_input =
            format!(r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#);

        match self {
            AttrType::Inputs(_) => {
                let (from_span, _to_span) = self.span();

                AttrType::insert_input_above_span(
                    from_span,
                    flake_contents,
                    &flake_input,
                    added_cosmetic_newline,
                )
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
                        return Err(color_eyre::eyre::eyre!(
                            "unsupported `outputs` expression type {} (at {}:{})",
                            t.variant_name(),
                            start.line,
                            start.column
                        ));
                    }
                }
            }
            AttrType::MissingInputs((outputs_span_from, _outputs_span_to)) => {
                // If we're not adding our new input above an existing `inputs` construct, let's add
                // another newline so that it looks nicer.
                flake_input.push_str(NEWLINE);
                added_cosmetic_newline = true;

                AttrType::insert_input_above_span(
                    outputs_span_from,
                    flake_contents,
                    &flake_input,
                    added_cosmetic_newline,
                )
            }
            AttrType::MissingOutputs((_inputs_span_from, _inputs_span_to)) => {
                // I don't really want to give them an `outputs` if it doesn't already exist, but
                // I've laid out the groundwork that it would be possible...
                Err(color_eyre::eyre::eyre!("flake had no `outputs`"))?
            }
            AttrType::MissingInputsAndOutputs(_root_span) => {
                // I don't really want to deal with a flake that has no `inputs` or `outputs`
                // either, but again, I've laid the groundwork to do so...
                // If we do decide to suppor this, the simplest way would be: insert at the root
                // span (\\n, then 2 spaces, then write inputs, don't care about outputs for now?)
                Err(color_eyre::eyre::eyre!(
                    "flake had neither `inputs` nor `outputs`"
                ))?
            }
        }
    }

    fn insert_input_above_span(
        span: nixel::Span,
        flake_contents: &str,
        flake_input: &str,
        added_cosmetic_newline: bool,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();
        let (start, _) = span_to_start_end_offsets(&flake_contents, &span)?;

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
            span_to_start_end_offsets(&flake_contents, &indentation_span)?;
        let indentation = &flake_contents[indentation_start..indentation_end];

        let offset = position_to_offset(&new_flake_contents, &old_content_pos)?;

        new_flake_contents.insert_str(offset, indentation);

        Ok(new_flake_contents)
    }

    fn insert_input_name_into_outputs_function(
        flake_input_name: &str,
        head: &nixel::FunctionHeadDestructured,
        from_span: nixel::Span,
        to_span: nixel::Span,
        flake_contents: &str,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();
        let final_named_arg = head.arguments.last();
        let multiline_args = from_span.start.line != to_span.end.line;

        if multiline_args {
            // TODO: try to match the style
        } else {
            // TODO: don't need to match the style because it's all on the same line
        }

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
                    let re = regex::Regex::new(&format!(r#"[^[:space:],]*\.\.\.[^[:space:],]*"#))?;

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
                    return Err(color_eyre::eyre::eyre!("empty set is kinda cringe"))?;
                }
            }
        }

        Ok(new_flake_contents)
    }

    fn span(&self) -> (nixel::Span, nixel::Span) {
        match self {
            AttrType::Inputs(attr) | AttrType::Outputs(attr) => (
                attr.from
                    .iter()
                    .next()
                    .map(|v| v.span())
                    .expect("attr existed, thus it must have a span"),
                attr.to.span(),
            ),
            AttrType::MissingInputs(_)
            | AttrType::MissingOutputs(_)
            | AttrType::MissingInputsAndOutputs(_) => todo!(),
        }
    }
}

fn upsert_into_inputs_and_outputs(
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

fn replace_input_value(
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

fn span_to_start_end_offsets(
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

fn position_to_offset(
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
