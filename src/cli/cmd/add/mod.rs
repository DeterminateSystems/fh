// FIXME: extract to somewhere else so it's more convenient
pub(crate) mod flake;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use color_eyre::eyre::WrapErr;

use self::flake::InputsInsertionLocation;

use super::{CommandExecute, FlakeHubClient};

const FALLBACK_FLAKE_CONTENTS: &str = r#"{
  description = "My new flake.";

  outputs = { ... } @ inputs: { };
}
"#;

/// Adds a flake input to your flake.nix.
#[derive(Parser, Debug)]
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
    /// A reference in the form of `NixOS/nixpkgs` or `NixOS/nixpkgs/0.2411.*` (without a URL
    /// scheme) will be inferred as a FlakeHub input.
    pub(crate) input_ref: String,
    /// Whether to insert a new input at the top of or the bottom of an existing `inputs` attrset.
    #[clap(long, default_value_t = InputsInsertionLocation::Top)]
    pub(crate) insertion_location: InputsInsertionLocation,
    /// Print to stdout the new flake.nix contents instead of writing it to disk.
    #[clap(long)]
    pub(crate) dry_run: bool,

    #[clap(from_global)]
    api_addr: url::Url,
}

impl CommandExecute for AddSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let (flake_contents, parsed) = load_flake(&self.flake_path).await?;

        let (flake_input_name, flake_input_url) =
            infer_flake_input_name_url(self.api_addr, self.input_ref, self.input_name).await?;
        let input_url_attr_path: VecDeque<String> = [
            String::from("inputs"),
            flake_input_name.clone(),
            String::from("url"),
        ]
        .into();

        let new_flake_contents = flake::upsert_flake_input(
            &parsed.expression,
            flake_input_name,
            flake_input_url,
            flake_contents,
            input_url_attr_path,
            self.insertion_location,
        )?;

        if self.dry_run {
            println!("{new_flake_contents}");
        } else {
            tokio::fs::write(self.flake_path, new_flake_contents).await?;
        }

        Ok(ExitCode::SUCCESS)
    }
}

#[tracing::instrument(skip_all)]
// FIXME: make a nix or nix_util module or something
pub(crate) async fn load_flake(
    flake_path: &PathBuf,
) -> color_eyre::Result<(String, nixel::Parsed)> {
    let mut contents = tokio::fs::read_to_string(&flake_path)
        .await
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(FALLBACK_FLAKE_CONTENTS.to_string())
            } else {
                Err(e)
            }
        })
        .wrap_err_with(|| format!("Failed to open {}", flake_path.display()))?;

    if contents.trim().is_empty() {
        contents = FALLBACK_FLAKE_CONTENTS.to_string();
    };

    let mut parsed = nixel::parse(contents.clone());

    if let nixel::Expression::Map(map) = *parsed.expression.clone()
        && map.bindings.is_empty()
    {
        contents = FALLBACK_FLAKE_CONTENTS.to_string();
        parsed = nixel::parse(FALLBACK_FLAKE_CONTENTS.to_string());
    }

    Ok((contents, parsed))
}

#[tracing::instrument(skip_all)]
async fn infer_flake_input_name_url(
    api_addr: url::Url,
    flake_ref: String,
    input_name: Option<String>,
) -> color_eyre::Result<(String, url::Url)> {
    let flake_ref = flake_ref.trim_end_matches('/');
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
                (None, _) => Err(color_eyre::eyre::eyre!(
                    "cannot infer an input name for {parsed_url}; please specify one with the `--input-name` flag"
                )),
            }
        }
        // A URL like `nixos/nixpkgs` or `nixos/nixpkgs/0.2411`
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let (org, project, version) = match flake_ref.split('/').collect::<Vec<_>>()[..] {
                // `nixos/nixpkgs/0.2411`
                [org, project, version] => {
                    let version = version.strip_suffix(".tar.gz").unwrap_or(version);
                    let version = version.strip_prefix('v').unwrap_or(version);
                    semver::VersionReq::parse(version).map_err(|_| {
                        color_eyre::eyre::eyre!(
                            "version '{version}' was not a valid SemVer version requirement"
                        )
                    })?;

                    (org, project, Some(version))
                }
                // `nixos/nixpkgs`
                [org, project] => (org, project, None),
                _ => Err(color_eyre::eyre::eyre!(
                    "flakehub input did not match the expected format of \
                    `org/project` or `org/project/version`"
                ))?,
            };

            let (flakehub_input, mut url) =
                get_flakehub_project_and_url(&api_addr, org, project, version).await?;

            if let Some(path_without_suffix) = url.path().strip_suffix(".tar.gz") {
                let owned_path = path_without_suffix.to_string();
                url.set_path(&owned_path);
            }

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

#[tracing::instrument(skip_all)]
pub(crate) async fn get_flakehub_project_and_url(
    api_addr: &url::Url,
    org: &str,
    project: &str,
    version: Option<&str>,
) -> color_eyre::Result<(String, url::Url)> {
    FlakeHubClient::project_and_url(api_addr.as_ref(), org, project, version).await
}
