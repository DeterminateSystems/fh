pub(crate) mod add;
pub(crate) mod completion;
pub(crate) mod convert;
pub(crate) mod eject;
pub(crate) mod init;
pub(crate) mod list;
pub(crate) mod login;
pub(crate) mod search;
pub(crate) mod status;

use color_eyre::eyre::WrapErr;
use once_cell::sync::Lazy;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION},
    Client,
};
use serde::{Deserialize, Serialize};
use tabled::settings::{
    style::{HorizontalLine, On, VerticalLineIter},
    Style,
};
use url::Url;

use self::{
    list::{Flake, Org, Release, Version},
    search::SearchResult,
    status::TokenStatus,
};
use crate::flakehub_url;

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

#[async_trait::async_trait]
pub trait CommandExecute {
    async fn execute(self) -> color_eyre::Result<std::process::ExitCode>;
}

#[derive(clap::Subcommand)]
pub(crate) enum FhSubcommands {
    Add(add::AddSubcommand),
    Completion(completion::CompletionSubcommand),
    Init(init::InitSubcommand),
    List(list::ListSubcommand),
    Search(search::SearchSubcommand),
    Convert(convert::ConvertSubcommand),
    Login(login::LoginSubcommand),
    Status(status::StatusSubcommand),
    Eject(eject::EjectSubcommand),
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FhError {
    #[error("file error: {0}")]
    Filesystem(#[from] std::io::Error),

    #[error("flake name parsing error: {0}")]
    FlakeParse(String),

    #[error("invalid header: {0}")]
    Header(#[from] reqwest::header::InvalidHeaderValue),

    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("interactive initializer error: {0}")]
    Interactive(#[from] inquire::InquireError),

    #[error("json parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("label parsing error: {0}")]
    LabelParse(String),

    #[error("the flake has no inputs")]
    NoInputs,

    #[error("template error: {0}")]
    Render(#[from] handlebars::RenderError),

    #[error("template error: {0}")]
    Template(#[from] Box<handlebars::TemplateError>),

    #[error("a presumably unreachable point was reached: {0}")]
    Unreachable(String),

    #[error("url parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("xdg base directory error: {0}")]
    Xdg(#[from] xdg::BaseDirectoriesError),
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
    async fn get<T: for<'de> Deserialize<'de>>(
        url: Url,
        params: Option<Vec<(&str, String)>>,
        authenticated: bool,
    ) -> Result<T, FhError> {
        let client = make_base_client(authenticated).await?;

        let req = client.get(url);

        Ok(if let Some(params) = params {
            req.query(&params).send().await?.json::<T>().await
        } else {
            req.send().await?.json::<T>().await
        }?)
    }

    pub(crate) async fn search(
        api_addr: &str,
        query: String,
    ) -> Result<Vec<SearchResult>, FhError> {
        let url = flakehub_url!(api_addr, "search");
        let params = vec![("q", query)];
        Self::get(url, Some(params), false).await
    }

    async fn flakes(api_addr: &str) -> Result<Vec<Flake>, FhError> {
        let url = flakehub_url!(api_addr, "flakes");
        Self::get(url, None, true).await
    }

    async fn flakes_by_label(api_addr: &str, label: &str) -> Result<Vec<Flake>, FhError> {
        let url = flakehub_url!(api_addr, "label", label);
        Self::get(url, None, true).await
    }

    async fn releases(api_addr: &str, org: &str, project: &str) -> Result<Vec<Release>, FhError> {
        let url = flakehub_url!(api_addr, "f", org, project, "releases");
        Self::get(url, None, true).await
    }

    async fn orgs(api_addr: &str) -> Result<Vec<Org>, FhError> {
        let url = flakehub_url!(api_addr, "orgs");
        Self::get(url, None, true).await
    }

    async fn versions(
        api_addr: &str,
        org: &str,
        project: &str,
        constraint: &str,
    ) -> Result<Vec<Version>, FhError> {
        let version = urlencoding::encode(constraint);

        let url = flakehub_url!(api_addr, "version", "resolve", org, project, &version);

        Self::get(url, None, true).await
    }

    async fn metadata(
        api_addr: &str,
        org: &str,
        project: &str,
        version: &str,
    ) -> color_eyre::Result<ProjectMetadata> {
        let url = flakehub_url!(api_addr, "version", org, project, version);
        let client = make_base_client(true).await?;

        let res = client.get(&url.to_string()).send().await?;

        if let Err(e) = res.error_for_status_ref() {
            let err_text = res.text().await?;
            return Err(e).wrap_err(err_text)?;
        };

        let res = res.json::<ProjectMetadata>().await?;

        Ok(res)
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
        let res = client.get(&url.to_string()).send().await?;
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
            .user_agent(crate::APP_USER_AGENT)
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

pub(crate) fn print_json<T: Serialize>(value: T) -> Result<(), FhError> {
    let json = serde_json::to_string(&value)?;
    println!("{}", json);
    Ok(())
}

// When testing, we need to not check for auth info in $XDG_CONFIG_HOME/flakehub/auth, as
// that causes the Nix sandbox build to fail
#[cfg(test)]
async fn make_base_client(_authenticated: bool) -> Result<Client, FhError> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    Ok(reqwest::Client::builder()
        .user_agent(crate::APP_USER_AGENT)
        .default_headers(headers)
        .build()?)
}

#[cfg(not(test))]
async fn make_base_client(authenticated: bool) -> Result<Client, FhError> {
    use self::login::auth_token_path;

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    if authenticated {
        if let Ok(token) = tokio::fs::read_to_string(auth_token_path()?).await {
            if !token.is_empty() {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", token.trim()))?,
                );
            }
        }
    }

    Ok(reqwest::Client::builder()
        .user_agent(crate::APP_USER_AGENT)
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
