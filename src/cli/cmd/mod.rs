pub(crate) mod add;
pub(crate) mod completion;
pub(crate) mod convert;
pub(crate) mod init;
pub(crate) mod list;
pub(crate) mod login;
pub(crate) mod search;
pub(crate) mod status;

use once_cell::sync::Lazy;
use prettytable::format::{FormatBuilder, LinePosition, LineSeparator, TableFormat};
use reqwest::Client as HttpClient;
use serde::Serialize;

use self::{
    list::{Flake, Org, Release, Version},
    search::SearchResult,
};

pub(crate) static TABLE_FORMAT: Lazy<TableFormat> = Lazy::new(|| {
    FormatBuilder::new()
        .borders('|')
        .padding(1, 1)
        .separators(
            &[LinePosition::Top, LinePosition::Title, LinePosition::Bottom],
            LineSeparator::new('-', '+', '+', '+'),
        )
        .build()
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
}

impl FlakeHubClient {
    pub(crate) fn new(api_addr: &url::Url) -> Result<Self, FhError> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "Accept",
            reqwest::header::HeaderValue::from_static("application/json"),
        );

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
        let mut url = self.api_addr.clone();
        {
            let mut segs = url
                .path_segments_mut()
                .expect("flakehub url cannot be base (this should never happen)");

            segs.push("label").push(label);
        }

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
        let mut url = self.api_addr.clone();
        {
            let mut segs = url
                .path_segments_mut()
                .expect("flakehub url cannot be base (this should never happen)");

            segs.push("f").push(org).push(project).push("releases");
        }

        let flakes = self
            .client
            .get(&url.to_string())
            .send()
            .await?
            .json::<Vec<Release>>()
            .await?;

        Ok(flakes)
    }

    async fn orgs(&self) -> Result<Vec<String>, FhError> {
        let endpoint = self.api_addr.join("orgs")?;

        let orgs = self
            .client
            .get(endpoint)
            .send()
            .await?
            .json::<Vec<Org>>()
            .await?
            .iter()
            .map(|Org { name }| name.clone())
            .collect();

        Ok(orgs)
    }

    async fn versions(
        &self,
        org: &str,
        project: &str,
        constraint: &str,
    ) -> Result<Vec<Version>, FhError> {
        let version = urlencoding::encode(constraint);

        let mut url = self.api_addr.clone();
        {
            let mut segs = url
                .path_segments_mut()
                .expect("flakehub url cannot be base (this should never happen)");

            segs.push("version")
                .push("resolve")
                .push(org)
                .push(project)
                .push(&version);
        }

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
