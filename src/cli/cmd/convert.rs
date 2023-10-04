use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use once_cell::sync::Lazy;

use super::CommandExecute;

// match {nixos,nixpkgs}-YY.MM branches
const RELEASE_BRANCH_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(nixos|nixpkgs)-(?<year>[[:digit:]]{2})\.(?<month>[[:digit:]]{2})").unwrap()
});

/// Convert flake inputs to FlakeHub when possible.
#[derive(Debug, Parser)]
pub(crate) struct ConvertSubcommand {
    /// The flake.nix to convert.
    #[clap(long, default_value = "./flake.nix")]
    pub(crate) flake_path: PathBuf,

    /// Print to stdout the new flake.nix contents instead of writing it to disk.
    #[clap(long)]
    pub(crate) dry_run: bool,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for ConvertSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        if !self.flake_path.exists() {
            return Err(color_eyre::eyre::eyre!(
                "the flake at {} did not exist",
                self.flake_path.display()
            ));
        }

        let (flake_contents, parsed) = crate::cli::cmd::add::load_flake(&self.flake_path).await?;
        let mut new_flake_contents = flake_contents.clone();

        let all_toplevel_inputs = crate::cli::cmd::add::flake::find_all_attrsets_by_path(
            &parsed.expression,
            Some(["inputs".into()].into()),
        )?;
        let all_inputs = crate::cli::cmd::add::flake::collect_all_inputs(all_toplevel_inputs)?;
        let mut flake_compat_input_name = None;

        for input in all_inputs.iter() {
            let Some(input_name) = input
                .from
                .into_iter()
                .filter_map(|part| match part {
                    nixel::Part::Raw(raw) => {
                        let content = raw.content.trim().to_string();

                        if ["inputs", "url"].contains(&content.as_ref()) {
                            None
                        } else {
                            Some(content)
                        }
                    }
                    _ => None,
                })
                .next()
            else {
                tracing::warn!("couldn't get input name from attrpath, skipping");
                continue;
            };

            let url = find_input_value_by_path(&input.to, ["url".into()].into())?;

            if let Some(ref url) = url {
                if url == "github:edolstra/flake-compat" {
                    // Save the flake-compat input name for later (so we can find it again)
                    flake_compat_input_name = Some(input_name.clone());
                    continue;
                }
            }

            let maybe_parsed_url = url.map(|u| u.parse::<url::Url>().ok()).flatten();

            let new_input_url = match maybe_parsed_url {
                Some(parsed_url) => convert_input_to_flakehub(&self.api_addr, parsed_url).await?,
                None => None,
            };

            if let Some(new_input_url) = new_input_url {
                let input_attr_path: VecDeque<String> =
                    ["inputs".into(), input_name.clone(), "url".into()].into();
                let Some(attr) = crate::cli::cmd::add::flake::find_first_attrset_by_path(
                    &parsed.expression,
                    Some(input_attr_path),
                )?
                else {
                    return Err(color_eyre::eyre::eyre!(
                        "there was no `inputs.{input_name}.url` attribute, but there should have been; \
                        please report this"
                    ));
                };
                new_flake_contents = crate::cli::cmd::add::flake::update_flake_input(
                    attr,
                    input_name,
                    new_input_url,
                    new_flake_contents,
                )?;
            }
        }

        let outputs_attr = crate::cli::cmd::add::flake::find_first_attrset_by_path(
            &parsed.expression,
            Some(["outputs".into()].into()),
        )?;

        // - has no nixpkgs in inputs but does have it in flake.lock, add it to flakehub.com/f/nixos/nixpkgs/0.1.0.tar.gz
        if let Some(outputs_attr) = outputs_attr {
            if let nixel::Expression::Function(f) = &*outputs_attr.to {
                let input_name = String::from("nixpkgs");
                match &f.head {
                    // outputs = { nixpkgs, ... } @ inputs: { }
                    nixel::FunctionHead::Destructured(head)
                        if head
                            .arguments
                            .iter()
                            .any(|arg| &*arg.identifier == input_name) =>
                    {
                        let (_, flakehub_url) = crate::cli::cmd::add::get_flakehub_project_and_url(
                            &self.api_addr,
                            "nixos",
                            &input_name,
                            None,
                        )
                        .await?;

                        new_flake_contents = crate::cli::cmd::add::flake::insert_flake_input(
                            &parsed.expression,
                            input_name.clone(),
                            flakehub_url.clone(),
                            new_flake_contents,
                            crate::cli::cmd::add::flake::InputsInsertionLocation::Top,
                        )?;
                    }
                    _ => {}
                }
            }
        }

        if let Some(input_name) = flake_compat_input_name {
            // Re-parse the contents since we might have added an input, and that will screw up offset calculations.
            let parsed = nixel::parse(new_flake_contents.clone());
            let input_attr_path: VecDeque<String> = ["inputs".into(), input_name.clone()].into();
            let input = crate::cli::cmd::add::flake::find_first_attrset_by_path(
                &parsed.expression,
                Some(input_attr_path),
            )?
            // This expect is safe because we already know there
            .expect(&format!("inputs.{input_name} disappeared from flake.nix"));

            let (_, flake_input_value) = crate::cli::cmd::add::get_flakehub_project_and_url(
                &self.api_addr,
                "edolstra",
                "flake-compat",
                None,
            )
            .await?;

            let (from_span, to_span) = crate::cli::cmd::add::flake::kv_to_span(&input);

            let indentation = crate::cli::cmd::add::flake::indentation_from_from_span(
                &new_flake_contents,
                &from_span,
            )?;
            let insertion_pos = nixel::Position {
                line: from_span.start.line,
                column: indentation.len() + 1, // since the indentation is already there
            };
            let offset = crate::cli::cmd::add::flake::position_to_offset(
                &new_flake_contents,
                &insertion_pos,
            )?;

            let start = crate::cli::cmd::add::flake::position_to_offset(
                &new_flake_contents,
                &from_span.start,
            )?;
            let end =
                crate::cli::cmd::add::flake::position_to_offset(&new_flake_contents, &to_span.end)?;
            new_flake_contents.replace_range(start..=end, "");

            let inputs_attr = crate::cli::cmd::add::flake::find_first_attrset_by_path(
                &parsed.expression,
                Some(["inputs".into()].into()),
            )?
            .expect("inputs disappeared from flake.nix");

            match inputs_attr.from.len() {
                // inputs = { nixpkgs.url = ""; };
                1 => {
                    let flake_input = format!(r#"{input_name}.url = "{flake_input_value}";"#);
                    new_flake_contents.insert_str(offset, &flake_input);
                }

                // inputs.nixpkgs = { url = ""; inputs.something.follows = ""; };
                // OR
                // inputs.nixpkgs.url = "";
                // OR
                // inputs.nixpkgs.inputs.something.follows = "";
                // etc...
                _len => {
                    let flake_input =
                        format!(r#"inputs.{input_name}.url = "{flake_input_value}";"#);
                    new_flake_contents.insert_str(offset, &flake_input);
                }
            }
        }

        if self.dry_run {
            println!("{new_flake_contents}");
        } else {
            tokio::fs::write(self.flake_path, new_flake_contents).await?;
            // TODO: nix flake lock?
        }

        Ok(ExitCode::SUCCESS)
    }
}

// FIXME: only supports strings for now
#[tracing::instrument(skip_all)]
// TODO: return the span as well
fn find_input_value_by_path(
    expr: &nixel::Expression,
    attr_path: VecDeque<String>,
    // FIXME: return a url::Url...?
) -> color_eyre::Result<Option<String>> {
    let mut found_value = None;

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
                        if most_recent_attr_matched
                        // If `this_attr_path` is empty, that means we've matched as much of the
                        // attr path as we can of this key node, and thus we need to recurse into
                        // its value node to continue checking if we want this input or not.
                        || this_attr_path.is_empty()
                        {
                            // We recurse again to deduplicate nixel::Expression::String/IndentedString handling
                            found_value = find_input_value_by_path(&kv.to, search_attr_path)?;

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
        nixel::Expression::String(s) => {
            found_value = s
                .parts
                .first()
                .map(|part| match part {
                    nixel::Part::Raw(raw) => Some(raw.content.trim().to_string()),
                    _ => None,
                })
                .flatten();
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

    Ok(found_value)
}

#[tracing::instrument(skip_all)]
async fn convert_input_to_flakehub(
    api_addr: &url::Url,
    parsed_url: url::Url,
) -> color_eyre::Result<Option<url::Url>> {
    let mut url = None;

    match parsed_url.host() {
        // A URL like `https://github.com/...`
        Some(_host) => match parsed_url.scheme() {
            "https" => {
                tracing::error!("not yet implemented");
                // dbg!("any other type of url", &parsed_url);
            }
            scheme => {
                tracing::warn!("unimplemented url scheme {scheme}");
            }
        },
        // A URL like `github:nixos/nixpkgs`
        None => match parsed_url.scheme() {
            "github" => {
                url = convert_github_input_to_flakehub(parsed_url, api_addr).await?;
            }
            scheme => {
                tracing::warn!("unimplemented flake input scheme {scheme}");
            }
        },
    }

    Ok(url)
}

#[tracing::instrument(skip_all)]
async fn convert_github_input_to_flakehub(
    parsed_url: url::Url,
    api_addr: &url::Url,
) -> color_eyre::Result<Option<url::Url>> {
    let mut url = None;

    let (org, project, maybe_version_or_branch) =
        match parsed_url.path().split('/').collect::<Vec<_>>()[..] {
            // `nixos/nixpkgs/nixos-23.05`
            [org, project, maybe_version_or_branch] => {
                (org, project, Some(maybe_version_or_branch))
            }
            // `nixos/nixpkgs`
            [org, project] => (org, project, None),
            _ => Err(color_eyre::eyre::eyre!(
                "flakehub input did not match the expected format of `org/project` or
                `org/project/version`"
            ))?,
        };

    match maybe_version_or_branch {
        Some(version_or_branch) => {
            // github:{org}/{repo}/{something} if {something} parses as a semver tag -> flakehub.com/{org}/{repo}/{something}.tar.gz
            if let Ok(version) = semver::Version::parse(
                version_or_branch
                    .strip_prefix("v")
                    .unwrap_or(version_or_branch),
            ) {
                let (_, flakehub_url) = crate::cli::cmd::add::get_flakehub_project_and_url(
                    &api_addr,
                    org,
                    project,
                    Some(&version.to_string()),
                )
                .await?;
                url = Some(flakehub_url);
            // - has nixpkgs:
            } else if (org.to_lowercase().as_ref(), project.to_lowercase().as_ref())
                == ("nixos", "nixpkgs")
            {
                let branch = version_or_branch;
                //   - ignore `-small` and `-darwin` suffixes on branches
                let branch = branch
                    .strip_suffix("-small")
                    .or_else(|| branch.strip_suffix("-darwin"))
                    .unwrap_or(branch);

                let release_branch_captures = RELEASE_BRANCH_REGEX.captures(branch);
                match branch {
                    //   - nixpkgs-unstable and nixos-unstable -> flakehub.com/f/nixos/nixpkgs/0.1.0.tar.gz
                    "nixpkgs-unstable" | "nixos-unstable" => {
                        let (_, flakehub_url) = crate::cli::cmd::add::get_flakehub_project_and_url(
                            &api_addr,
                            org,
                            project,
                            Some("0.1.0"),
                        )
                        .await?;
                        url = Some(flakehub_url);
                    }
                    _ => {
                        //   - nixos-{yy}.{mm} -> flakehub.com/f/nixos/nixpkgs/0.{yymm}.0.tar.gz IFF {yymm} >= 2003
                        if let Some(captures) = release_branch_captures {
                            // Unwraps here are safe because we're guaranteed to have them if
                            // the captures object is Some(_)
                            let year_str = captures.name("year").unwrap().as_str();
                            let month_str = captures.name("month").unwrap().as_str();
                            let year: u64 = year_str.parse()?;
                            let month: u64 = month_str.parse()?;

                            // NixOS 20.03 and later have a flake.nix
                            if year >= 20 && month >= 3 {
                                let version = format!("0.{year_str}{month_str}.0");
                                // FIXME: (maybe) -- this returns the latest despite specifying version .0 (requirements say to use .0)
                                let (_, flakehub_url) =
                                    crate::cli::cmd::add::get_flakehub_project_and_url(
                                        &api_addr,
                                        org,
                                        project,
                                        Some(&version),
                                    )
                                    .await?;
                                url = Some(flakehub_url);
                            }
                        } else {
                            tracing::warn!(
                                "nixpkgs input was not an unstable or nixos-YY.MM release branch, was '{branch}'"
                            );
                        }
                    }
                }
            } else {
                // github:{org}/{repo}/{something} fallthrough -> warn and do nothing
                tracing::warn!("input was not of the form [org]/[project]/[semver], skipping");
            }
        }
        None => {
            // github:{org}/{repo} -> flakehub.com/f/{org}/{repo}/x.y.z.tar.gz (where x.y.z is the currently-latest version)
            if let Ok((_, flakehub_url)) =
                crate::cli::cmd::add::get_flakehub_project_and_url(&api_addr, org, project, None)
                    .await
            {
                url = Some(flakehub_url);
            } else {
                tracing::warn!("didn't have {org}/{project} uploaded");
            }
        }
    }

    Ok(url)
}