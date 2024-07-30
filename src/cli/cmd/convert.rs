use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

use clap::Parser;
use color_eyre::eyre::Context;
use once_cell::sync::Lazy;
use tracing::{span, Level};

use super::{nix_command, CommandExecute};

// match {nixos,nixpkgs,release}-YY.MM branches
static RELEASE_BRANCH_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"(nixos|nixpkgs|release)-(?<year>[[:digit:]]{2})\.(?<month>[[:digit:]]{2})")
        .unwrap()
});

const NIXPKGS_IMPLICIT_INPUT_NAME: &str = "nixpkgs";
const SHELL_NIX: &str = "shell.nix";
const DEFAULT_NIX: &str = "default.nix";
const FLAKE_COMPAT_MARKER: &str = "https://github.com/edolstra/flake-compat/archive";

const FLAKE_COMPAT_CONTENTS_PREFIX: &str = r#"(import
  (
    let lock = builtins.fromJSON (builtins.readFile ./flake.lock); in
    fetchTarball {
      url = lock.nodes.flake-compat.locked.url or "https://github.com/edolstra/flake-compat/archive/${lock.nodes.flake-compat.locked.rev}.tar.gz";
      sha256 = lock.nodes.flake-compat.locked.narHash;
    }
  )
  { src = ./.; }
)"#;

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
        let (new_flake_contents, flake_compat_input_name) = self
            .convert_inputs_to_flakehub(&parsed.expression, &flake_contents)
            .await?;
        let new_flake_contents = self
            .make_implicit_nixpkgs_explicit(&parsed.expression, &new_flake_contents)
            .await?;
        let new_flake_contents = if let Some(flake_compat_input_name) = flake_compat_input_name {
            let new_flake_contents = self
                .fixup_flake_compat_input(&new_flake_contents, flake_compat_input_name)
                .await?;

            if !self.dry_run {
                self.fixup_flake_compat_nix_files().await?;
            }

            new_flake_contents
        } else {
            new_flake_contents
        };

        if self.dry_run {
            println!("{new_flake_contents}");
        } else {
            tokio::fs::write(self.flake_path, new_flake_contents).await?;

            tracing::debug!("Running: nix flake lock");

            nix_command(&["flake", "lock"], false)
                .await
                .wrap_err("failed to create missing lock file entries")?;
        }

        Ok(ExitCode::SUCCESS)
    }
}

impl ConvertSubcommand {
    #[tracing::instrument(skip_all)]
    async fn convert_inputs_to_flakehub(
        &self,
        expr: &nixel::Expression,
        flake_contents: &str,
    ) -> color_eyre::Result<(String, Option<String>)> {
        let mut new_flake_contents = flake_contents.to_string();

        let all_toplevel_inputs = crate::cli::cmd::add::flake::find_all_attrsets_by_path(
            expr,
            Some(["inputs".into()].into()),
        )?;
        tracing::trace!("All inputs detected: {:#?}", all_toplevel_inputs);
        let all_inputs = crate::cli::cmd::add::flake::collect_all_inputs(all_toplevel_inputs)?;
        tracing::trace!("Collected inputs: {:#?}", all_inputs);
        let mut flake_compat_input_name = None;

        for input in all_inputs.iter() {
            tracing::trace!("Examining input: {:#?}", input);
            let Some(input_name) = input.from.iter().find_map(|part| match part {
                nixel::Part::Raw(raw) => {
                    let content = raw.content.trim().to_string();

                    if ["inputs", "url"].contains(&content.as_ref()) {
                        None
                    } else {
                        Some(content)
                    }
                }
                _ => None,
            }) else {
                tracing::debug!("couldn't get input name from attrpath, skipping");
                continue;
            };

            let span = span!(Level::DEBUG, "processing_input", %input_name);
            let _span_guard = span.enter();

            let url = find_input_value_by_path(&input.to, ["url".into()].into())?;
            tracing::debug!("Current input's `url` value: {:?}", url);

            let url = match url {
                Some(url) => {
                    if url == "github:edolstra/flake-compat" {
                        // Save the flake-compat input name for later (so we can find it again)
                        flake_compat_input_name = Some(input_name.clone());
                        continue;
                    }

                    // Bare-minimum Nixpkgs-from-flake-registry handling
                    if url == "nixpkgs" || url.starts_with("nixpkgs/") {
                        let mut url = url;
                        url.insert_str(0, "github:NixOS/");
                        Some(url)
                    } else {
                        Some(url)
                    }
                }
                None => None,
            };
            tracing::debug!("Transformed URL: {:?}", url);

            let maybe_parsed_url = url.and_then(|u| u.parse::<url::Url>().ok());
            tracing::trace!("Parsed URL: {:?}", maybe_parsed_url);

            let new_input_url = match maybe_parsed_url {
                Some(parsed_url) => convert_input_to_flakehub(&self.api_addr, parsed_url).await?,
                None => None,
            };

            if let Some(new_input_url) = new_input_url {
                let input_attr_path: VecDeque<String> =
                    ["inputs".into(), input_name.clone(), "url".into()].into();
                let Some(attr) = crate::cli::cmd::add::flake::find_first_attrset_by_path(
                    expr,
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

        Ok((new_flake_contents, flake_compat_input_name))
    }

    #[tracing::instrument(skip_all)]
    async fn make_implicit_nixpkgs_explicit(
        &self,
        expr: &nixel::Expression,
        flake_contents: &str,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();
        let input_name = String::from(NIXPKGS_IMPLICIT_INPUT_NAME);
        let outputs_attr = crate::cli::cmd::add::flake::find_first_attrset_by_path(
            expr,
            Some(["outputs".into()].into()),
        )?;

        let nixpkgs_input_attr = crate::cli::cmd::add::flake::find_first_attrset_by_path(
            expr,
            Some(["inputs".into(), input_name.clone()].into()),
        )?;

        // If there's already an input that matches the nixpkgs implicit input name, we don't need
        // to insert another input for it.
        if nixpkgs_input_attr.is_some() {
            return Ok(new_flake_contents);
        }

        // - has no nixpkgs in inputs but does have it in flake.lock, add it to flakehub.com/f/nixos/nixpkgs/0.1.0.tar.gz
        if let Some(outputs_attr) = outputs_attr {
            if let nixel::Expression::Function(f) = &*outputs_attr.to {
                match &f.head {
                    // outputs = { nixpkgs, ... } @ inputs: { }
                    nixel::FunctionHead::Destructured(head)
                        if head
                            .arguments
                            .iter()
                            .any(|arg| *arg.identifier == input_name) =>
                    {
                        let (_, flakehub_url) = crate::cli::cmd::add::get_flakehub_project_and_url(
                            &self.api_addr,
                            "nixos",
                            &input_name,
                            None,
                        )
                        .await?;

                        new_flake_contents = crate::cli::cmd::add::flake::insert_flake_input(
                            expr,
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

        Ok(new_flake_contents)
    }

    #[tracing::instrument(skip_all)]
    async fn fixup_flake_compat_input(
        &self,
        flake_contents: &str,
        input_name: String,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();

        // Re-parse the contents since we might have added an input, and that will screw up offset calculations.
        let parsed = nixel::parse(new_flake_contents.clone());
        let input_attr_path: VecDeque<String> = ["inputs".into(), input_name.clone()].into();
        let input = crate::cli::cmd::add::flake::find_first_attrset_by_path(
            &parsed.expression,
            Some(input_attr_path),
        )?
        // This expect is safe because we already know there
        .unwrap_or_else(|| panic!("inputs.{input_name} disappeared from flake.nix"));

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
        let offset =
            crate::cli::cmd::add::flake::position_to_offset(&new_flake_contents, &insertion_pos)?;

        let start =
            crate::cli::cmd::add::flake::position_to_offset(&new_flake_contents, &from_span.start)?;
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
                let flake_input = format!(r#"inputs.{input_name}.url = "{flake_input_value}";"#);
                new_flake_contents.insert_str(offset, &flake_input);
            }
        }

        Ok(new_flake_contents)
    }

    async fn fixup_flake_compat_nix_files(&self) -> color_eyre::Result<()> {
        let shell_nix_path = PathBuf::from(SHELL_NIX);
        let default_nix_path = PathBuf::from(DEFAULT_NIX);
        let mut shell_nix_clean = true;
        let mut default_nix_clean = true;

        let git_toplevel = tokio::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .status()
            .await?;
        let is_a_git_repo = git_toplevel.success();

        if is_a_git_repo {
            let files = tokio::process::Command::new("git")
                .args(["ls-files ", "--modified ", "--full-name"])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .output()
                .await?;
            let output = std::str::from_utf8(&files.stdout)?;

            for line in output.lines() {
                if line.contains("shell.nix") {
                    shell_nix_clean = false;
                }
                if line.contains("default.nix") {
                    default_nix_clean = false;
                }
            }
        }

        if shell_nix_path.exists() {
            let existing_contents = tokio::fs::read_to_string(&shell_nix_path).await?;
            if existing_contents.contains(FLAKE_COMPAT_MARKER) {
                let contents = format!("{FLAKE_COMPAT_CONTENTS_PREFIX}.shellNix\n");

                if !shell_nix_clean || !is_a_git_repo {
                    tracing::info!(
                        "We recommend you update the contents of your {SHELL_NIX} to use the flake-compat pinned in your flake:\n{contents}"
                    );
                } else {
                    tokio::fs::write(shell_nix_path, contents).await?;
                }
            }
        }

        if default_nix_path.exists() {
            let existing_contents = tokio::fs::read_to_string(&default_nix_path).await?;
            if existing_contents.contains(FLAKE_COMPAT_MARKER) {
                let contents = format!("{FLAKE_COMPAT_CONTENTS_PREFIX}.defaultNix\n");

                if !default_nix_clean || !is_a_git_repo {
                    tracing::info!(
                        "We recommend you update the contents of your {DEFAULT_NIX} to use the flake-compat pinned in your flake:\n{contents}"
                    );
                } else {
                    tokio::fs::write(default_nix_path, contents).await?;
                }
            }
        }

        Ok(())
    }
}

// FIXME: only supports strings for now
#[tracing::instrument(skip_all)]
// TODO: return the span as well
pub(crate) fn find_input_value_by_path(
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
            found_value = s.parts.first().and_then(|part| match part {
                nixel::Part::Raw(raw) => Some(raw.content.trim().to_string()),
                _ => None,
            });
        }
        nixel::Expression::IndentedString(s) => {
            found_value = s.parts.first().and_then(|part| match part {
                nixel::Part::Raw(raw) => Some(raw.content.trim().to_string()),
                _ => None,
            });
        }
        nixel::Expression::Uri(u) => {
            found_value = Some(u.uri.trim().to_string());
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
        Some(host) => {
            if host == url::Host::Domain("api.flakehub.com") {
                let mut mod_url = parsed_url.clone();
                mod_url.set_host(Some("flakehub.com"))?;
                url = Some(mod_url);
            } else {
                match parsed_url.scheme() {
                    "https" => {
                        tracing::debug!("https://... urls are not yet implented");
                    }
                    scheme => {
                        tracing::debug!("unimplemented url scheme {scheme}");
                    }
                }
            }
        }
        // A URL like `github:nixos/nixpkgs`
        None => match parsed_url.scheme() {
            "github" => {
                url = convert_github_input_to_flakehub(parsed_url, api_addr).await?;
            }
            scheme => {
                tracing::debug!("unimplemented flake input scheme {scheme}");
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
                    .strip_prefix('v')
                    .unwrap_or(version_or_branch),
            ) {
                if let Ok((_, flakehub_url)) = crate::cli::cmd::add::get_flakehub_project_and_url(
                    api_addr,
                    org,
                    project,
                    Some(&version.to_string()),
                )
                .await
                {
                    url = Some(flakehub_url);
                }
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
                        if let Ok((_, flakehub_url)) =
                            crate::cli::cmd::add::get_flakehub_project_and_url(
                                api_addr,
                                org,
                                project,
                                Some("0.1.0"),
                            )
                            .await
                        {
                            url = Some(flakehub_url);
                        }
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
                                if let Ok((_, flakehub_url)) =
                                    crate::cli::cmd::add::get_flakehub_project_and_url(
                                        api_addr,
                                        org,
                                        project,
                                        Some(&version),
                                    )
                                    .await
                                {
                                    url = Some(flakehub_url);
                                }
                            }
                        } else {
                            tracing::debug!(
                                "nixpkgs input was not an unstable or nixos-YY.MM release branch, was '{branch}'"
                            );
                        }
                    }
                }
            } else {
                // github:{org}/{repo}/{something} fallthrough -> warn and do nothing
                tracing::debug!("input was not of the form [org]/[project]/[semver], skipping");
            }
        }
        None => {
            // github:{org}/{repo} -> flakehub.com/f/{org}/{repo}/x.y.z.tar.gz (where x.y.z is the currently-latest version)
            if let Ok((_, flakehub_url)) =
                crate::cli::cmd::add::get_flakehub_project_and_url(api_addr, org, project, None)
                    .await
            {
                url = Some(flakehub_url);
            } else {
                tracing::debug!("didn't have {org}/{project} uploaded");
            }
        }
    }

    Ok(url)
}

#[cfg(test)]
mod test {
    use axum::{extract::Path, response::IntoResponse};

    async fn version(
        Path((org, project, version)): Path<(String, String, String)>,
    ) -> axum::response::Response {
        axum::Json(serde_json::json!({
            "project": project,
            "pretty_download_url": format!("http://flakehub-localhost/f/{org}/{project}/{version}.tar.gz"),
        }))
        .into_response()
    }

    async fn no_version(Path((org, project)): Path<(String, String)>) -> axum::response::Response {
        axum::Json(serde_json::json!({
            "project": project,
            "pretty_download_url": format!("http://flakehub-localhost/f/{org}/{project}/*.tar.gz"),
        }))
        .into_response()
    }

    fn test_router() -> axum::Router {
        axum::Router::new()
            .route(
                "/version/:org/:project/:version",
                axum::routing::get(version),
            )
            .route("/f/:org/:project", axum::routing::get(no_version))
    }

    #[tokio::test]
    async fn nixpkgs_to_flakehub() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url = url::Url::parse("github:someorg/somerepo").unwrap();
            let tarball_url = super::convert_input_to_flakehub(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(tarball_url.path(), "/f/someorg/somerepo/*.tar.gz");
        }
    }

    #[tokio::test]
    async fn nixpkgs_release_to_flakehub() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url = url::Url::parse("github:nixos/nixpkgs/nixos-23.05").unwrap();
            let tarball_url = super::convert_input_to_flakehub(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(tarball_url.path(), "/f/nixos/nixpkgs/0.2305.0.tar.gz");
        }
    }

    #[tokio::test]
    async fn test_flake1_convert() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let convert = super::ConvertSubcommand {
                flake_path: "".into(),
                dry_run: true,
                api_addr: server_url,
            };
            let flake_contents = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/samples/flake1.test.nix"
            ));
            let flake_contents = flake_contents.to_string();
            let parsed = nixel::parse(flake_contents.clone());

            let (new_flake_contents, flake_compat_input_name) = convert
                .convert_inputs_to_flakehub(&parsed.expression, &flake_contents)
                .await
                .unwrap();
            let new_flake_contents = convert
                .make_implicit_nixpkgs_explicit(&parsed.expression, &new_flake_contents)
                .await
                .unwrap();
            let new_flake_contents = convert
                .fixup_flake_compat_input(&new_flake_contents, flake_compat_input_name.unwrap())
                .await
                .unwrap();

            assert!(new_flake_contents.contains(
            r#"flake-compat.url = "http://flakehub-localhost/f/edolstra/flake-compat/*.tar.gz";"#
        ));
            assert!(new_flake_contents.contains("f/nixos/nixpkgs/0.2305.0.tar.gz"));

            let nixpkgs_url_lines: Vec<_> = new_flake_contents
                .lines()
                .filter(|line| {
                    line.contains("nixpkgs.url") && line.contains("f/nixos/nixpkgs/0.2305.0.tar.gz")
                })
                .collect();
            let num_nixpkgs_url_lines = nixpkgs_url_lines.len();
            assert_eq!(num_nixpkgs_url_lines, 1);
        }
    }

    #[tokio::test]
    async fn test_nixpkgs_from_registry() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let convert = super::ConvertSubcommand {
                flake_path: "".into(),
                dry_run: true,
                api_addr: server_url,
            };
            let flake_contents = r#"
{
  description = "cole-h's NixOS configuration";

  inputs = {
    nixpkgs.url = "nixpkgs";
  };

  outputs = { self, ... } @ tes: { };
}
"#;
            let flake_contents = flake_contents.to_string();
            let parsed = nixel::parse(flake_contents.clone());

            let (new_flake_contents, _) = convert
                .convert_inputs_to_flakehub(&parsed.expression, &flake_contents)
                .await
                .unwrap();

            assert!(new_flake_contents.contains(
                r#"nixpkgs.url = "http://flakehub-localhost/f/NixOS/nixpkgs/*.tar.gz";"#
            ));
        }
    }

    #[tokio::test]
    async fn old_flakehub_to_new_flakehub() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url =
                url::Url::parse("https://api.flakehub.com/f/NixOS/nixpkgs/0.1.514192.tar.gz")
                    .unwrap();
            let tarball_url = super::convert_input_to_flakehub(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(
                tarball_url.host().unwrap(),
                url::Host::Domain("flakehub.com")
            );
            assert_ne!(
                tarball_url.host().unwrap(),
                url::Host::Domain("api.flakehub.com")
            );
        }
    }
}
