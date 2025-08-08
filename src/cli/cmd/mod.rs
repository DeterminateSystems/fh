pub(crate) mod add;
pub(crate) mod apply;
pub(crate) mod completion;
pub(crate) mod convert;
pub(crate) mod eject;
pub(crate) mod fetch;
pub(crate) mod init;
pub(crate) mod list;
pub(crate) mod login;
pub(crate) mod resolve;
pub(crate) mod search;
pub(crate) mod status;

use std::{fmt::Display, process::Stdio};

use color_eyre::eyre::{self, WrapErr};
use once_cell::sync::Lazy;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION},
    Client, StatusCode,
};
use serde::{Deserialize, Serialize};
use tabled::settings::{
    style::{HorizontalLine, On, VerticalLineIter},
    Style,
};
use tokio::process::Command;
use url::Url;

use self::{
    init::command_exists,
    list::{Flake, Org, Release, Version},
    resolve::ResolvedPath,
    search::SearchResult,
    status::TokenStatus,
};
use crate::{flakehub_url, APP_USER_AGENT};

use super::error::FhError;

#[allow(clippy::type_complexity)]
static DEFAULT_STYLE: Lazy<
    Style<
        On,
        On,
        On,
        On,
        (),
        (),
        [HorizontalLine; 1],
        VerticalLineIter<std::array::IntoIter<tabled::settings::style::VerticalLine, 0>>,
    >,
> = Lazy::new(|| {
    Style::ascii()
        .remove_vertical()
        .remove_horizontal()
        .horizontals([HorizontalLine::new(1, Style::modern().get_horizontal())
            .main(Some('-'))
            .intersection(None)])
});

pub trait CommandExecute {
    async fn execute(self) -> color_eyre::Result<std::process::ExitCode>;
}

#[derive(clap::Subcommand)]
pub(crate) enum FhSubcommands {
    Add(add::AddSubcommand),
    Apply(apply::ApplySubcommand),
    Completion(completion::CompletionSubcommand),
    Convert(convert::ConvertSubcommand),
    Eject(eject::EjectSubcommand),
    Fetch(fetch::FetchSubcommand),
    Init(init::InitSubcommand),
    List(list::ListSubcommand),
    Login(login::LoginSubcommand),
    Resolve(resolve::ResolveSubcommand),
    Search(search::SearchSubcommand),
    Status(status::StatusSubcommand),
}

#[derive(Debug, Deserialize)]
struct ProjectMetadata {
    source_github_owner_repo_pair: String,
    source_subdirectory: Option<String>,
    version: String,
}

#[derive(Debug, Deserialize)]
struct ProjectCanonicalNames {
    project: String,
    // FIXME: detect Nix version and strip .tar.gz if it supports it
    pretty_download_url: url::Url,
}

pub(crate) struct FlakeHubClient;

impl FlakeHubClient {
    pub(crate) async fn search(
        api_addr: &str,
        query: String,
    ) -> Result<Vec<SearchResult>, FhError> {
        let url = flakehub_url!(api_addr, "search");
        let params = vec![("q", query)];
        get_with_params(url, params, false).await
    }

    async fn flakes(api_addr: &str, owner: Option<String>) -> Result<Vec<Flake>, FhError> {
        match owner {
            Some(owner) => {
                let projects: Vec<list::Project> =
                    get(flakehub_url!(api_addr, "orgs", &owner, "projects"), true)
                        .await
                        .unwrap();

                Ok(projects
                    .into_iter()
                    .map(|proj| Flake {
                        org: proj.organization_name,
                        project: proj.name,
                    })
                    .collect())
            }
            None => get(flakehub_url!(api_addr, "flakes"), true).await,
        }
    }

    async fn flakes_by_label(api_addr: &str, label: &str) -> Result<Vec<Flake>, FhError> {
        let url = flakehub_url!(api_addr, "label", label);
        get(url, true).await
    }

    async fn releases(api_addr: &str, org: &str, project: &str) -> Result<Vec<Release>, FhError> {
        let url = flakehub_url!(api_addr, "f", org, project, "releases");
        get(url, true).await
    }

    async fn orgs(api_addr: &str) -> Result<Vec<Org>, FhError> {
        let url = flakehub_url!(api_addr, "orgs");
        let params = vec![("include_public", String::from("true"))];
        get_with_params(url, params, true).await
    }

    async fn versions(
        api_addr: &str,
        org: &str,
        project: &str,
        constraint: &str,
    ) -> Result<Vec<Version>, FhError> {
        let version = urlencoding::encode(constraint);
        let url = flakehub_url!(api_addr, "version", "resolve", org, project, &version);
        get(url, true).await
    }

    async fn metadata(
        api_addr: &str,
        org: &str,
        project: &str,
        version: &str,
    ) -> color_eyre::Result<ProjectMetadata> {
        let url = flakehub_url!(api_addr, "version", org, project, version);
        let client = make_base_client(true).await?;

        let res = client.get(url.to_string()).send().await?;

        // Enrich the CLI error text with the error returned by FlakeHub
        if let Err(e) = res.error_for_status_ref() {
            let err_text = res.text().await?;
            return Err(e).wrap_err(err_text)?;
        };

        let res = res.json::<ProjectMetadata>().await?;

        Ok(res)
    }

    async fn resolve(
        api_addr: &str,
        output_ref: &FlakeOutputRef,
        include_token: bool,
    ) -> Result<ResolvedPath, FhError> {
        let FlakeOutputRef {
            ref org,
            project: ref flake,
            ref version_constraint,
            ref attr_path,
        } = output_ref;

        let mut url = flakehub_url!(
            api_addr,
            "f",
            org,
            flake,
            version_constraint,
            "output",
            attr_path
        );

        if include_token {
            url.set_query(Some("include_token=true"));
        }

        let client = make_base_client(true).await?;

        match client.get(url).send().await {
            Ok(res) => match res.status() {
                StatusCode::OK => Ok(res.json().await?),
                StatusCode::NOT_FOUND => Err(FhError::NotFound(
                    "output reference".to_string(),
                    output_ref.to_string(),
                )),
                StatusCode::UNAUTHORIZED => {
                    Err(FhError::NotAuthorized("output reference".to_string()))
                }
                status => Err(FhError::MiscHttp(status)),
            },
            Err(e) => Err(e.into()),
        }
    }

    async fn project_and_url(
        api_addr: &str,
        org: &str,
        project: &str,
        version: Option<&str>,
    ) -> color_eyre::Result<(String, url::Url)> {
        let url = match version {
            Some(version) => flakehub_url!(api_addr, "version", org, project, version),
            None => flakehub_url!(api_addr, "f", org, project),
        };
        let client = make_base_client(true).await?;
        let res = client.get(url.to_string()).send().await?;

        // Enrich the CLI error text with the error returned by FlakeHub
        if let Err(e) = res.error_for_status_ref() {
            let err_text = res.text().await?;
            return Err(e).wrap_err(err_text)?;
        };

        let res = res.json::<ProjectCanonicalNames>().await?;
        Ok((res.project, res.pretty_download_url))
    }

    async fn auth_status(api_addr: &str, token: &str) -> color_eyre::Result<TokenStatus> {
        let url = flakehub_url!(api_addr, "cli", "status");

        let res = reqwest::Client::builder()
            .user_agent(APP_USER_AGENT)
            .build()?
            .get(url)
            .header(AUTHORIZATION, &format!("Bearer {token}"))
            .send()
            .await
            .wrap_err("Failed to send request")?;

        if res.status() == 401 {
            return Err(color_eyre::eyre::eyre!(
                "The provided token was invalid. Please try again, or contact support@flakehub.com if the problem persists."
            ));
        }

        let res = res
            .error_for_status()
            .wrap_err("Request was unsuccessful")?;
        let token_status: TokenStatus = res.json().await.wrap_err(
            "Failed to get TokenStatus from response (wasn't JSON, or was invalid JSON?)",
        )?;

        Ok(token_status)
    }
}

async fn get<T: for<'de> Deserialize<'de>>(url: Url, authenticated: bool) -> Result<T, FhError> {
    let client = make_base_client(authenticated).await?;

    Ok(client.get(url).send().await?.json::<T>().await?)
}

async fn get_with_params<T: for<'de> Deserialize<'de>>(
    url: Url,
    params: Vec<(&str, String)>,
    authenticated: bool,
) -> Result<T, FhError> {
    let client = make_base_client(authenticated).await?;

    Ok(client
        .get(url)
        .query(&params)
        .send()
        .await?
        .json::<T>()
        .await?)
}

pub(crate) fn print_json<T: Serialize>(value: T) -> Result<(), FhError> {
    let json = serde_json::to_string(&value)?;
    println!("{json}");
    Ok(())
}

// Parses a flake reference as a string to construct paths of the form:
// https://api.flakehub.com/f/{org}/{flake}/{version_constraint}/output/{attr_path}
struct FlakeOutputRef {
    org: String,
    project: String,
    version_constraint: String,
    attr_path: String,
}

impl Display for FlakeOutputRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}/{}#{}",
            self.org, self.project, self.version_constraint, self.attr_path
        )
    }
}

impl TryFrom<String> for FlakeOutputRef {
    type Error = FhError;

    fn try_from(flakehub_ref: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = flakehub_ref.split('#').collect();

        if let Some(release_parts) = parts.first() {
            let Some(attr_path) = parts.get(1) else {
                Err(FhError::MissingFromOutputRef(String::from(
                    "the output attribute path",
                )))?
            };

            let release_parts: Vec<&str> = release_parts.split('/').collect();

            if release_parts.len() > 3 {
                return Err(FhError::MalformedFlakeOutputRef);
            }

            let Some(org) = release_parts.first() else {
                Err(FhError::MissingFromOutputRef(String::from(
                    "the flake's org",
                )))?
            };
            let Some(flake) = release_parts.get(1) else {
                Err(FhError::MissingFromOutputRef(String::from(
                    "the flake's name",
                )))?
            };
            let Some(version) = release_parts.get(2) else {
                Err(FhError::MissingFromOutputRef(String::from(
                    "the flake's version constraint",
                )))?
            };

            Ok(FlakeOutputRef {
                org: org.to_string(),
                project: flake.to_string(),
                version_constraint: version.to_string(),
                attr_path: attr_path.to_string(),
            })
        } else {
            Err(FhError::MissingFromOutputRef(String::from(
                "the flake's release info ({org}/{flake}/{version}) and the output's attribute path",
            )))
        }
    }
}

// When testing, we need to not check for auth info in $XDG_CONFIG_HOME/flakehub/auth, as
// that causes the Nix sandbox build to fail
#[cfg(test)]
async fn make_base_client(_authenticated: bool) -> Result<Client, FhError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    Ok(reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .default_headers(headers)
        .build()?)
}

#[cfg(not(test))]
async fn make_base_client(authenticated: bool) -> Result<Client, FhError> {
    use self::login::user_auth_token_read_path;

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    if authenticated {
        if let Ok(token) = tokio::fs::read_to_string(user_auth_token_read_path().await?).await {
            if !token.is_empty() {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", token.trim()))?,
                );
            }
        }
    }

    Ok(reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .default_headers(headers)
        .build()?)
}

#[macro_export]
macro_rules! flakehub_url {
    ($url:expr, $($segment:expr),+ $(,)?) => {{
        let mut url = url::Url::parse($url)
            .expect("failed to parse flakehub web root url (this should never happen)");

        {
            let mut segs = url
                .path_segments_mut()
                .expect("URL cannot be a base (this should never happen)");

            $(
                segs.push($segment);
            )+
        }
        url
    }};
}

fn is_root_user() -> bool {
    nix::unistd::getuid().is_root()
}

async fn nix_command(args: &[String], sudo_if_necessary: bool) -> Result<(), FhError> {
    if !command_exists("nix") {
        return Err(FhError::MissingExecutable("nix".to_string()));
    }

    let use_sudo = sudo_if_necessary && !is_root_user();

    let mut cmd = if use_sudo {
        tracing::warn!(
            "Current user is {} rather than root; running Nix command using sudo",
            whoami::username()
        );

        let mut cmd = tokio::process::Command::new("sudo");
        cmd.arg("nix");
        cmd
    } else {
        tokio::process::Command::new("nix")
    };

    cmd.args(["--extra-experimental-features", "nix-command flakes"]);
    cmd.args(args);
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    let cmd_str = format!("{:?}", cmd.as_std());
    tracing::debug!("Running: {:?}", cmd_str);

    let output = cmd
        .spawn()
        .wrap_err("failed to spawn Nix command")?
        .wait_with_output()
        .await
        .wrap_err("failed to wait for Nix command output")?;

    if output.status.success() {
        Ok(())
    } else {
        Err(FhError::FailedNixCommand(cmd_str))
    }
}

fn parse_flake_output_ref(
    frontend_addr: &url::Url,
    output_ref: &str,
) -> Result<FlakeOutputRef, FhError> {
    // Ensures that users can use both forms:
    // 1. https://flakehub/f/{org}/{project}/{version_req}#{output}
    // 2. {org}/{project}/{version_req}#{output}
    let output_ref = String::from(
        output_ref
            .strip_prefix(frontend_addr.join("f/")?.as_str())
            .unwrap_or(output_ref),
    );

    output_ref.try_into()
}

// Ensure that release refs are of the form {org}/{project}/{version_req}
fn parse_release_ref(flake_ref: &str) -> Result<String, FhError> {
    match flake_ref.split('/').collect::<Vec<_>>()[..] {
        [org, project, version_req] => {
            validate_segment(org)?;
            validate_segment(project)?;
            validate_segment(version_req)?;

            Ok(flake_ref.to_string())
        }
        _ => Err(FhError::FlakeParse(format!(
            "flake ref {flake_ref} invalid; must be of the form {{org}}/{{project}}/{{version_req}}"
        ))),
    }
}

// Ensure that orgs, project names, and the like don't contain whitespace
fn validate_segment(s: &str) -> Result<(), FhError> {
    if s.chars().any(char::is_whitespace) {
        return Err(FhError::FlakeParse(format!(
            "path segment {s} contains whitespace"
        )));
    }

    Ok(())
}

/// Copy a Nix closure from a given host into the store.
pub async fn copy_closure(
    cache_host: impl Into<String>,
    store_path: impl Into<String>,
    token_path: impl Into<String>,
) -> color_eyre::Result<()> {
    let args = vec![
        "copy".into(),
        "--option".into(),
        "narinfo-cache-negative-ttl".into(),
        "0".into(),
        "--from".into(),
        cache_host.into(),
        store_path.into(),
        "--netrc-file".into(),
        token_path.into(),
    ];

    nix_command(&args, false)
        .await
        .wrap_err("Failed to copy resolved store path with Nix")?;

    Ok(())
}

async fn copy_supports_out_link() -> color_eyre::Result<bool> {
    const OUT_LINK_NOT_SUPPORTED: &[u8] = b"error: unrecognised flag '--out-link'";

    // Not using nix_command() here because we need to read the stderr of the resulting command
    let output = Command::new("nix")
        .args(["copy", "--out-link"])
        .output()
        .await
        .wrap_err("Could not run nix")?;

    // Grab only the first line of output from nix since it's the one we care about (the problem it encountered)
    let error_line = output.stderr.split(|&c| c == b'\n').next();
    let error_line = match error_line {
        Some(line) => line,
        None => {
            tracing::warn!("Could not determine if `nix copy` supports --out-link; falling back to manual links");
            return Ok(false);
        }
    };

    let supported = error_line != OUT_LINK_NOT_SUPPORTED;
    tracing::debug!(supported, "Setting support for nix copy --out-link");

    Ok(supported)
}

async fn copy_closure_with_out_link(
    cache_host: impl Into<String>,
    store_path: impl Into<String>,
    token_path: impl Into<String>,
    out_path: impl Into<String>,
) -> color_eyre::Result<()> {
    let args = vec![
        "copy".into(),
        "--option".into(),
        "narinfo-cache-negative-ttl".into(),
        "0".into(),
        "--from".into(),
        cache_host.into(),
        store_path.into(),
        "--out-link".into(),
        out_path.into(),
        "--netrc-file".into(),
        token_path.into(),
    ];

    nix_command(&args, false)
        .await
        .wrap_err("Failed to copy resolved store path with Nix")
}

async fn copy_closure_with_realise(
    cache_host: impl Into<String>,
    store_path: impl Into<String>,
    token_path: impl Into<String>,
    out_path: impl Into<String>,
) -> color_eyre::Result<()> {
    let cache_host = cache_host.into();
    let store_path = store_path.into();
    let token_path = token_path.into();
    let out_path = out_path.into();

    // First, copy the closure down into the user's Nix store
    copy_closure(cache_host, &store_path, &token_path).await?;

    // Now we can use a plain `nix-store --realise` on it
    let mut command = Command::new("nix-store");
    let output = command
        .arg("--realise")
        .arg(&store_path)
        .arg("--add-root")
        .arg(&out_path)
        .spawn()
        .wrap_err("Failed to spawn nix-store command")?
        .wait_with_output()
        .await?;

    eyre::ensure!(
        output.status.success(),
        "Could not use nix-store --realise to copy {store_path} to {out_path}; consider upgrading to Nix version 2.26 or greater which is immune to this problem"
    );

    Ok(())
}

/// Copy a Nix closure like [`copy_closure`], but with a GC root. The bool that
/// is returned indicates if `nix copy --out-link` (supported with version 2.26)
/// was used.
pub async fn copy_closure_with_gc_root(
    cache_host: impl Into<String>,
    store_path: impl Into<String>,
    token_path: impl Into<String>,
    out_path: impl Into<String>,
) -> color_eyre::Result<bool> {
    let use_out_link = copy_supports_out_link().await?;

    if use_out_link {
        copy_closure_with_out_link(cache_host, store_path, token_path, out_path).await?;
    } else {
        copy_closure_with_realise(cache_host, store_path, token_path, out_path).await?;
    }

    Ok(use_out_link)
}

#[cfg(test)]
mod tests {
    #[test]
    fn flakehub_url_macro() {
        let root = "https://flakehub.com";

        for (provided, expected) in vec![
            (
                flakehub_url!(root, "flake", "DeterminateSystems", "fh"),
                "https://flakehub.com/flake/DeterminateSystems/fh",
            ),
            (
                flakehub_url!(root, "flake", "NixOS", "nixpkgs", "*"),
                "https://flakehub.com/flake/NixOS/nixpkgs/*",
            ),
            (
                flakehub_url!(root, "flake", "nix-community", "home-manager", "releases"),
                "https://flakehub.com/flake/nix-community/home-manager/releases",
            ),
        ] {
            assert_eq!(provided.as_ref(), expected);
        }
    }
}
