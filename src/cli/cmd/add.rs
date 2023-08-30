// TODO: query flakehub api if it exists, error if not; also use org/repo name as returned by the api (so it includes proper caps)

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
        let input = tokio::fs::read_to_string(&self.flake_path).await?;
        let mut output = input.clone();
        let parsed = nixel::parse(input.clone());
        let (flake_input_name, flake_input_url) =
            infer_flake_input_name_url(self.api_addr, self.input_ref, self.input_name).await?;
        let input_url_attr_path: VecDeque<String> = [
            String::from("inputs"),
            flake_input_name.clone(),
            String::from("url"),
        ]
        .into();

        upsert_flake_input(
            *parsed.expression,
            flake_input_name,
            flake_input_url,
            input,
            &mut output,
            input_url_attr_path,
        )?;

        // tokio::fs::write(self.flake_path, output).await?;
        println!("{output}");

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
    input: String,
    output: &mut String,
    input_attr_path: VecDeque<String>,
) -> color_eyre::Result<()> {
    match find_attr(&expr, Some(input_attr_path))? {
        Some(input_attr) => {
            let existing_input_value: nixel::String_ =
                todo!("match .to expression type to string or error");
            replace_input_value(
                &existing_input_value.parts,
                &flake_input_value,
                &input,
                output,
            )?;
        }
        None => {
            let inputs_attr_path: VecDeque<String> = [String::from("inputs")].into();
            let outputs_attr_path: VecDeque<String> = [String::from("outputs")].into();

            let first_attr = find_attr(&expr, None);
            let first_attr = todo!("make binding key value into partraw?");
            let inputs_attr = find_attr(&expr, Some(inputs_attr_path))?;
            let outputs_attr = find_attr(&expr, Some(outputs_attr_path))?;

            // We don't do anything fancy like trying to match the existing format of e.g.
            // `inputs = { <input_name>.url = "<input_value>"; };`
            let flake_input =
                format!(r#"inputs.{flake_input_name}.url = "{flake_input_value}";{NEWLINE}"#);
            insert_flake_input(
                flake_input,
                input,
                output,
                first_attr,
                outputs_attr,
                inputs_attr,
            )?;
        }
    }

    // TODO: handle empty flake.nix file
    match find_existing_flake_input(&expr, &input_attr_path)? {
        A {
            existing_input_value: Some(existing_input_value),
            ..
        } => {
        }
        a @ A {
            // existing_input_value: None,
            // first_attr_raw,
            // outputs_attr,
            // inputs_attr,
            ..
        } => {
            dbg!(&a);
            panic!();
            // TODO: DON'T MAKE THE CHANGES YET, JUST RECORD WHERE THEY SHOULD BE MADE.
            // TODO: FIND OUTPUTS
        }
        _ => {
            todo!();
        }
    }

    println!("Added: {flake_input_name} -> {flake_input_value}");

    Ok(())
}

#[derive(Debug)]
struct A {
    existing_input_value: Option<nixel::String_>,
    first_attr_raw: Option<nixel::PartRaw>,
    inputs_attr: Option<nixel::PartRaw>,
    outputs_attr: Option<nixel::PartRaw>,
}

fn find_existing_flake_input<'a>(
    expr: &'a nixel::Expression,
    attr_path: &VecDeque<String>,
) -> color_eyre::Result<A> {
    let mut first_raw = None;
    let mut inputs_raw = None;
    let mut outputs_raw = None;

    find_existing_flake_input_impl(
        expr,
        attr_path,
        &mut first_raw,
        &mut inputs_raw,
        &mut outputs_raw,
    )
}

fn find_existing_flake_input_impl<'a>(
    expr: &'a nixel::Expression,
    attr_path: &VecDeque<String>,
    first_raw: &mut Option<&'a nixel::PartRaw>,
    inputs_raw: &mut Option<&'a nixel::PartRaw>,
    outputs_raw: &mut Option<&'a nixel::PartRaw>,
) -> color_eyre::Result<A> {
    match expr {
        nixel::Expression::Map(map) => {
            for binding in map.bindings.iter() {
                match binding {
                    nixel::Binding::KeyValue(kv) => {
                        // Transform `inputs.nixpkgs.url` into `["inputs", "nixpkgs", "url"]`
                        let mut this_attr_path: VecDeque<(String, &nixel::PartRaw)> = kv
                            .from
                            .iter()
                            .filter_map(|attr| match attr {
                                nixel::Part::Raw(raw) => Some((raw.content.to_string(), raw)),
                                _ => None,
                            })
                            .collect();

                        // We record the first PartRaw we see, because if we don't find a same-named
                        // input, we'll insert the input with the specified input name right above
                        // this attr.
                        if first_raw.is_none() {
                            if let Some((attr, raw)) = this_attr_path.pop_front() {
                                *first_raw = Some(raw);
                                this_attr_path.push_front((attr, raw));
                            }
                        }

                        let mut search_attr_path = attr_path.clone();

                        // Find the correct attr path to modify
                        while let Some(attr1) = search_attr_path.pop_front() {
                            if let Some((attr2, attr2_raw)) = this_attr_path.pop_front() {
                                // We want to record the first `inputs` attr we find so that we
                                // can put a new input (if necessary) just above it.
                                if attr2 == "inputs" {
                                    if inputs_raw.is_none() {
                                        *inputs_raw = Some(attr2_raw);
                                    }
                                }

                                // We also want to record the first `outputs` attr we find so that
                                // we can insert the new input into the `{ ... }` (if necessary).
                                if attr2 == "outputs" {
                                    if outputs_raw.is_none() {
                                        *outputs_raw = Some(attr2_raw);
                                    }
                                }

                                // For every key in the attr path we're searching for we check that
                                // we have a matching attr key in the current attrset.
                                if attr1 != attr2 {
                                    // We want `this_attr_path` to contain all the attr path keys
                                    // that didn't match the attr path we're looking for, so we can
                                    // know when it matched as many of the attr paths as possible
                                    // (when `this_attr_path` is empty).
                                    this_attr_path.push_front((attr2, attr2_raw));
                                }
                            } else {
                                // If it doesn't match, that means this isn't the correct attr path,
                                // so we re-add the unmatched attr to `search_attr_path`...
                                search_attr_path.push_front(attr1);

                                // ...and break out to preserve all unmatched attrs.
                                break;
                            }
                        }

                        // If `this_attr_path` is empty, that means we've matched as much of the
                        // attr path as we can of this key node, and thus we need to recurse into
                        // its value node to continue checking if we want this input or not.
                        if this_attr_path.is_empty() {
                            return find_existing_flake_input_impl(
                                &kv.to,
                                &search_attr_path,
                                first_raw,
                                inputs_raw,
                                outputs_raw,
                            );
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
        nixel::Expression::String(s) => {
            return Ok(A {
                existing_input_value: Some(s.clone()),
                first_attr_raw: first_raw.map(ToOwned::to_owned),
                inputs_attr: inputs_raw.map(ToOwned::to_owned),
                outputs_attr: outputs_raw.map(ToOwned::to_owned),
            });
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

    Ok(A {
        existing_input_value: None,
        first_attr_raw: first_raw.map(ToOwned::to_owned),
        inputs_attr: inputs_raw.map(ToOwned::to_owned),
        outputs_attr: outputs_raw.map(ToOwned::to_owned),
    })
}

fn find_attr<'a>(
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
                            let mut most_recent_attr = None;

                            // Find the correct attr path to modify
                            while let Some(attr1) = search_attr_path.pop_front() {
                                if let Some((attr2, attr2_raw)) = this_attr_path.pop_front() {
                                    // For every key in the attr path we're searching for we check that
                                    // we have a matching attr key in the current attrset.
                                    if attr1 != attr2 {
                                        most_recent_attr = None;

                                        // We want `this_attr_path` to contain all the attr path keys
                                        // that didn't match the attr path we're looking for, so we can
                                        // know when it matched as many of the attr paths as possible
                                        // (when `this_attr_path` is empty).
                                        this_attr_path.push_front((attr2, attr2_raw));
                                    } else {
                                        most_recent_attr = Some(attr2_raw);
                                    }
                                } else {
                                    most_recent_attr = None;

                                    // If it doesn't match, that means this isn't the correct attr path,
                                    // so we re-add the unmatched attr to `search_attr_path`...
                                    search_attr_path.push_front(attr1);

                                    // ...and break out to preserve all unmatched attrs.
                                    break;
                                }
                            }

                            // If `search_attr_path` is empty, that means we've found the attr we want!
                            // Probably.
                            if most_recent_attr.is_some() {
                                return Ok(Some(kv.clone()));
                            }

                            // If `this_attr_path` is empty, that means we've matched as much of the
                            // attr path as we can of this key node, and thus we need to recurse into
                            // its value node to continue checking if we want this input or not.
                            if this_attr_path.is_empty() {
                                return find_attr(&kv.to, Some(search_attr_path));
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

fn insert_flake_input(
    mut flake_input: String,
    input: String,
    output: &mut String,
    first_raw: Option<nixel::PartRaw>,
    inputs_attr: Option<nixel::BindingKeyValue>,
    outputs_attr: Option<nixel::BindingKeyValue>,
) -> Result<(), color_eyre::Report> {
    // TODO: use inputs pos to insert the inputs
    // TODO: if there is no inputs, then what? insert as first in file? or maybe after the first in the file...?
    // TODO: use outputs to insert the new input name
    if let Some(first_raw) = first_raw {
        // If we're not adding our new input above an existing `inputs` construct, let's add
        // another newline so that it looks nicer.
        let mut added_cosmetic_newline = false;
        if &*first_raw.content != "inputs" {
            flake_input.push_str(NEWLINE);
            added_cosmetic_newline = true;
        }

        let (start, _) = span_to_start_end_offsets(&input, &first_raw.span)?;
        // Insert the new contents
        output.insert_str(start, &flake_input);

        // Preserve the exact indentation of the old contents
        let old_content_start_of_indentation_pos = nixel::Position {
            line: first_raw.span.start.line,
            column: 1,
        };
        let old_content_end_of_indentation_pos = first_raw.span.start.clone();
        let indentation_span = nixel::Span {
            start: Box::new(old_content_start_of_indentation_pos),
            end: old_content_end_of_indentation_pos,
        };
        let (indentation_start, indentation_end) =
            span_to_start_end_offsets(&input, &indentation_span)?;
        let indentation = &input[indentation_start..indentation_end];

        let old_content_pos = nixel::Position {
            // we moved the old contents to the next line...
            line: first_raw.span.start.line + 1 + if added_cosmetic_newline { 1 } else { 0 },
            // ...at the very beginning
            column: 1,
        };
        let offset = position_to_offset(output, &old_content_pos)?;

        // Re-align the indentation using the exact indentation that was
        // used for the line we bumped out of the way.
        output.insert_str(offset, indentation);
    }

    Ok(())
}

fn replace_input_value(
    parts: &[nixel::Part],
    flake_input_value: &url::Url,
    input: &str,
    output: &mut String,
) -> color_eyre::Result<()> {
    let mut parts_iter = parts.iter();

    if let Some(part) = parts_iter.next() {
        match part {
            nixel::Part::Raw(raw) => {
                let (start, end) = span_to_start_end_offsets(input, &raw.span)?;

                // Replace the current contents with nothingness
                output.replace_range(start..end, "");
                // Insert the new contents
                output.insert_str(start, flake_input_value.as_ref());
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

    Ok(())
}

fn span_to_start_end_offsets(
    input: &str,
    span: &nixel::Span,
) -> color_eyre::Result<(usize, usize)> {
    let start = &*span.start;
    let end = &*span.end;

    Ok((
        position_to_offset(input, start)?,
        position_to_offset(input, end)?,
    ))
}

fn position_to_offset(input: &str, position: &nixel::Position) -> color_eyre::Result<usize> {
    let mut column = 1;
    let mut line = 1;

    for (idx, ch) in input.char_indices() {
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
