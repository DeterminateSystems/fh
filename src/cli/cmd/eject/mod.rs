use std::collections::VecDeque;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use color_eyre::eyre::WrapErr;
use once_cell::sync::Lazy;
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION};
use serde::Deserialize;
use tracing::{span, Level};

use super::CommandExecute;

static ROLLING_RELEASE_BUILD_META_REGEX: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"(rev)-.{40}").unwrap());
static RELEASE_VERSION_REGEX: Lazy<regex::Regex> = Lazy::new(|| {
    regex::Regex::new(r"0\.(?<year>[[:digit:]]{2})(?<month>[[:digit:]]{2}).+").unwrap()
});

/// Convert flake inputs from FlakeHub back to GitHub.
#[derive(Debug, Parser)]
pub(crate) struct EjectSubcommand {
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
impl CommandExecute for EjectSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        if !self.flake_path.exists() {
            return Err(color_eyre::eyre::eyre!(
                "the flake at {} did not exist",
                self.flake_path.display()
            ));
        }

        let (flake_contents, parsed) = crate::cli::cmd::add::load_flake(&self.flake_path).await?;
        let new_flake_contents = self
            .eject_inputs_to_github(&parsed.expression, &flake_contents)
            .await?;

        if self.dry_run {
            println!("{new_flake_contents}");
        } else {
            tokio::fs::write(self.flake_path, new_flake_contents).await?;
            // NOTE: We don't auto-lock like we do in `fh convert` because this is a lossy process.
            // We don't know if the version was a tag like `v1.0.0` or if it was just `1.0.0` (or
            // any other format). So, we do a best effort attempt of assuming `1.0.0` and letting
            // the user fix it up if that was wrong.
        }

        Ok(ExitCode::SUCCESS)
    }
}

impl EjectSubcommand {
    #[tracing::instrument(skip_all)]
    async fn eject_inputs_to_github(
        &self,
        expr: &nixel::Expression,
        flake_contents: &str,
    ) -> color_eyre::Result<String> {
        let mut new_flake_contents = flake_contents.to_string();

        let all_toplevel_inputs = crate::cli::cmd::add::flake::find_all_attrsets_by_path(
            expr,
            Some(["inputs".into()].into()),
        )?;
        tracing::trace!("All inputs detected: {:#?}", all_toplevel_inputs);
        let all_inputs = crate::cli::cmd::add::flake::collect_all_inputs(all_toplevel_inputs)?;
        tracing::trace!("Collected inputs: {:#?}", all_inputs);

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

            let url = crate::cli::cmd::convert::find_input_value_by_path(
                &input.to,
                ["url".into()].into(),
            )?;
            tracing::debug!("Current input's `url` value: {:?}", url);

            let maybe_parsed_url = url.and_then(|u| u.parse::<url::Url>().ok());
            tracing::trace!("Parsed URL: {:?}", maybe_parsed_url);

            let new_input_url = match maybe_parsed_url {
                Some(parsed_url) => eject_input_to_github(&self.api_addr, parsed_url).await?,
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

        Ok(new_flake_contents)
    }
}

#[tracing::instrument(skip_all)]
async fn eject_input_to_github(
    api_addr: &url::Url,
    parsed_url: url::Url,
) -> color_eyre::Result<Option<url::Url>> {
    let mut url = None;

    if let Some(host) = parsed_url.host() {
        // A URL like `https://flakehub.com/...`
        if host == url::Host::Domain("flakehub.com") {
            url = Some(eject_flakehub_input_to_github(parsed_url, api_addr).await?);
        }
    }

    Ok(url)
}

#[tracing::instrument(skip_all)]
async fn eject_flakehub_input_to_github(
    parsed_url: url::Url,
    api_addr: &url::Url,
) -> color_eyre::Result<url::Url> {
    let (org, project, version) = match parsed_url.path().split('/').collect::<Vec<_>>()[..] {
        // `/f/NixOS/nixpkgs/0.1.514192.tar.gz`
        ["", "f", org, project, version] => {
            let version = version.strip_suffix(".tar.gz").unwrap_or(version);
            (org, project, version)
        }
        _ => Err(color_eyre::eyre::eyre!(
            "flakehub input did not match the expected format of `/f/org/project/version`"
        ))?,
    };

    let ProjectMetadata {
        source_github_owner_repo_pair,
        source_subdirectory,
        version,
    } = get_metadata_from_flakehub(api_addr, org, project, version).await?;

    let maybe_version_or_branch = match source_github_owner_repo_pair.to_lowercase().as_str() {
        "nixos/nixpkgs" => {
            let version = separate_year_from_month_in_version(&version);

            version.map(|release| format!("nixos-{release}"))
        }
        "nix-community/home-manager" => {
            let version = separate_year_from_month_in_version(&version);

            version.map(|release| format!("release-{release}"))
        }
        _ => {
            let semver = semver::Version::parse(&version).map_err(|err| {
                color_eyre::eyre::eyre!(
                    "failed to parse semver version from flakehub api ('{}'): {err}",
                    version
                )
            })?;
            let meta = semver.build.as_str();

            if ROLLING_RELEASE_BUILD_META_REGEX.is_match(meta) {
                // Rolling release from the repo, follow the repo's HEAD instead
                None
            } else {
                Some(version)
            }
        }
    };

    let mut new_url = format!("github:{source_github_owner_repo_pair}");
    if let Some(version_or_branch) = maybe_version_or_branch {
        new_url.push('/');
        new_url.push_str(&version_or_branch);
    }
    if let Some(subdir) = source_subdirectory {
        new_url.push_str("?dir=");
        new_url.push_str(&subdir);
    }
    let new_url: url::Url = new_url.parse()?;

    Ok(new_url)
}

fn separate_year_from_month_in_version(version: &str) -> Option<String> {
    let release_version_captures = RELEASE_VERSION_REGEX.captures(version);
    let version = match release_version_captures {
        Some(captures) => {
            let year = captures.name("year").unwrap().as_str();
            let month = captures.name("month").unwrap().as_str();

            Some(format!("{year}.{month}"))
        }
        _ => None,
    };

    version
}

#[derive(Debug, Deserialize)]
struct ProjectMetadata {
    source_github_owner_repo_pair: String,
    source_subdirectory: Option<String>,
    version: String,
}

#[tracing::instrument(skip_all)]
async fn get_metadata_from_flakehub(
    api_addr: &url::Url,
    org: &str,
    project: &str,
    version: &str,
) -> color_eyre::Result<ProjectMetadata> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    let xdg = xdg::BaseDirectories::new()?;
    // $XDG_CONFIG_HOME/fh/auth; basically ~/.config/fh/auth
    let token_path = xdg.get_config_file("flakehub/auth");

    if token_path.exists() {
        let token = tokio::fs::read_to_string(&token_path)
            .await
            .wrap_err_with(|| format!("Could not open {}", token_path.display()))?;

        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))?,
        );
    }

    let client = reqwest::Client::builder()
        .user_agent(crate::APP_USER_AGENT)
        .default_headers(headers)
        .build()?;

    let mut flakehub_json_url = api_addr.clone();
    {
        let mut path_segments_mut = flakehub_json_url
            .path_segments_mut()
            .expect("flakehub url cannot be base (this should never happen)");

        path_segments_mut
            .push("version")
            .push(org)
            .push(project)
            .push(version);
    }

    let res = client.get(&flakehub_json_url.to_string()).send().await?;

    if let Err(e) = res.error_for_status_ref() {
        let err_text = res.text().await?;
        return Err(e).wrap_err(err_text)?;
    };

    let res = res.json::<ProjectMetadata>().await?;

    Ok(res)
}

#[cfg(test)]
mod test {
    use axum::{extract::Path, response::IntoResponse};

    async fn version(
        Path((org, project, version)): Path<(String, String, String)>,
    ) -> axum::response::Response {
        let version = if version == "*" {
            "0.1.0+rev-eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
        } else {
            &version
        };

        let (source_github_owner_repo_pair, source_subdirectory) =
            if org == "edolstra" && project == "blender-bin" {
                (format!("{org}/nix-warez"), Some("blender"))
            } else {
                (format!("{org}/{project}"), None)
            };

        axum::Json(serde_json::json!({
            "source_github_owner_repo_pair": source_github_owner_repo_pair,
            "source_subdirectory": source_subdirectory,
            "version": version,
        }))
        .into_response()
    }

    fn test_router() -> axum::Router {
        axum::Router::new().route(
            "/version/:org/:project/:version",
            axum::routing::get(version),
        )
    }

    #[tokio::test]
    async fn flakehub_to_github() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url =
                url::Url::parse("https://flakehub.com/f/someorg/somerepo/*.tar.gz").unwrap();
            let github_url = super::eject_input_to_github(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(github_url.to_string(), "github:someorg/somerepo");
        }
    }

    #[tokio::test]
    async fn versioned_flakehub_to_github() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url =
                url::Url::parse("https://flakehub.com/f/someorg/somerepo/1.0.0.tar.gz").unwrap();
            let github_url = super::eject_input_to_github(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(github_url.to_string(), "github:someorg/somerepo/1.0.0");
        }
    }

    #[tokio::test]
    async fn flakehub_nixpkgs_to_github() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let input_url =
                url::Url::parse("https://flakehub.com/f/nixos/nixpkgs/0.2311.*.tar.gz").unwrap();
            let github_url = super::eject_input_to_github(&server_url, input_url)
                .await
                .ok()
                .flatten()
                .unwrap();
            assert_eq!(github_url.to_string(), "github:nixos/nixpkgs/nixos-23.11");
        }
    }

    #[tokio::test]
    async fn test_flake8_eject() {
        if let Ok(test_server) = axum_test::TestServer::new(test_router().into_make_service()) {
            let server_addr = test_server.server_address();
            let server_url = server_addr.parse().unwrap();

            let eject = super::EjectSubcommand {
                flake_path: "".into(),
                dry_run: true,
                api_addr: server_url,
            };
            let flake_contents = include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/samples/flake8.test.nix"
            ));
            let flake_contents = flake_contents.to_string();
            let parsed = nixel::parse(flake_contents.clone());

            let new_flake_contents = eject
                .eject_inputs_to_github(&parsed.expression, &flake_contents)
                .await
                .unwrap();

            assert!(new_flake_contents.contains("github:NixOS/nixpkgs/nixos-23.05"));
            assert!(new_flake_contents.contains("github:DeterminateSystems/fh"));
            assert!(new_flake_contents.contains("github:DeterminateSystems/fh/0.0.0"));
            assert!(new_flake_contents.contains("github:edolstra/nix-warez/0.0.0?dir=blender"));
            assert!(new_flake_contents.contains("github:edolstra/nix-warez?dir=blender"));
            assert!(new_flake_contents.contains("github:nix-community/home-manager/release-23.05"));
        }
    }
}
