pub(crate) mod add;
pub(crate) mod completion;
pub(crate) mod convert;
pub(crate) mod eject;
pub(crate) mod init;
pub(crate) mod list;
pub(crate) mod login;
pub(crate) mod search;
pub(crate) mod status;

use once_cell::sync::Lazy;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION},
    Client as HttpClient,
};
use serde::Serialize;
use tabled::settings::{
    style::{HorizontalLine, On, VerticalLineIter},
    Style,
};

use self::{
    list::{Flake, Org, Release, Version},
    login::auth_token_path,
    search::SearchResult,
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

pub(crate) struct FlakeHubClient {
    client: HttpClient,
    api_addr: url::Url,
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

impl FlakeHubClient {
    pub(crate) async fn new(api_addr: &url::Url, authenticate: bool) -> Result<Self, FhError> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        if authenticate {
            if let Ok(token) = tokio::fs::read_to_string(auth_token_path()?).await {
                if !token.is_empty() {
                    headers.insert(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Bearer {}", token.trim()))?,
                    );
                }
            }
        }

        let client = reqwest::Client::builder()
            .user_agent(crate::APP_USER_AGENT)
            .default_headers(headers)
            .build()?;

        Ok(Self {
            api_addr: api_addr.clone(),
            client,
        })
    }

    pub(crate) async fn search(&self, query: String) -> Result<Vec<SearchResult>, FhError> {
        let params = [("q", query)];

        let endpoint = self.api_addr.join("search")?;

        let results = self
            .client
            .get(endpoint)
            .query(&params)
            .send()
            .await?
            .json::<Vec<SearchResult>>()
            .await?;

        Ok(results)
    }

    async fn flakes(&self) -> Result<Vec<Flake>, FhError> {
        let endpoint = self.api_addr.join("flakes")?;

        let flakes = self
            .client
            .get(endpoint)
            .send()
            .await?
            .json::<Vec<Flake>>()
            .await?;

        Ok(flakes)
    }

    async fn flakes_by_label(&self, label: &str) -> Result<Vec<Flake>, FhError> {
        let url = flakehub_url!(&self.api_addr.to_string(), "label", label);

        let flakes = self
            .client
            .get(&url.to_string())
            .send()
            .await?
            .json::<Vec<Flake>>()
            .await?;

        Ok(flakes)
    }

    async fn releases(&self, org: &str, project: &str) -> Result<Vec<Release>, FhError> {
        let url = flakehub_url!(&self.api_addr.to_string(), "f", org, project, "releases");

        let flakes = self
            .client
            .get(&url.to_string())
            .send()
            .await?
            .json::<Vec<Release>>()
            .await?;

        Ok(flakes)
    }

    async fn orgs(&self) -> Result<Vec<Org>, FhError> {
        let endpoint = self.api_addr.join("orgs")?;

        let orgs = self
            .client
            .get(endpoint)
            .send()
            .await?
            .json::<Vec<Org>>()
            .await?;

        Ok(orgs)
    }

    async fn versions(
        &self,
        org: &str,
        project: &str,
        constraint: &str,
    ) -> Result<Vec<Version>, FhError> {
        let version = urlencoding::encode(constraint);

        let url = flakehub_url!(
            &self.api_addr.to_string(),
            "version",
            "resolve",
            org,
            project,
            &version
        );

        let versions = self
            .client
            .get(url)
            .send()
            .await?
            .json::<Vec<Version>>()
            .await?;

        Ok(versions)
    }
}

pub(crate) fn print_json<T: Serialize>(value: T) -> Result<(), FhError> {
    let json = serde_json::to_string(&value)?;
    println!("{}", json);
    Ok(())
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
