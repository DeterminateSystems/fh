mod add;
mod list;
mod search;

use lazy_static::lazy_static;
use prettytable::format::{FormatBuilder, LinePosition, LineSeparator, TableFormat};
use reqwest::Client as HttpClient;

use crate::cli::cmd::list::Org;

use self::{list::Flake, search::SearchResult};

lazy_static! {
    pub(super) static ref TABLE_FORMAT: TableFormat = FormatBuilder::new()
        .borders('|')
        .padding(1, 1)
        .separators(
            &[LinePosition::Top, LinePosition::Title, LinePosition::Bottom],
            LineSeparator::new('-', '+', '+', '+'),
        )
        .build();
}

#[async_trait::async_trait]
pub trait CommandExecute {
    async fn execute(self) -> color_eyre::Result<std::process::ExitCode>;
}

#[derive(clap::Subcommand)]
pub(crate) enum FhSubcommands {
    Add(add::AddSubcommand),
    Search(search::SearchSubcommand),
    List(list::ListSubcommand),
}

pub(super) struct FlakeHubClient {
    client: HttpClient,
    api_addr: url::Url,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum FhError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

impl FlakeHubClient {
    pub(super) fn new(api_addr: &url::Url) -> Result<Self, FhError> {
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

    pub(super) async fn search(&self, query: String) -> Result<Vec<SearchResult>, FhError> {
        let params = [("q", query)];

        let endpoint = format!("{}/search", self.api_addr);

        let results = self
            .client
            .get(&endpoint)
            .query(&params)
            .send()
            .await?
            .json::<Vec<SearchResult>>()
            .await?;

        Ok(results)
    }

    async fn flakes(&self) -> Result<Vec<Flake>, FhError> {
        let endpoint = format!("{}/flakes", self.api_addr);

        let flakes = self
            .client
            .get(&endpoint)
            .send()
            .await?
            .json::<Vec<Flake>>()
            .await?;

        Ok(flakes)
    }

    async fn orgs(&self) -> Result<Vec<String>, FhError> {
        let endpoint = format!("{}/orgs", self.api_addr);

        let orgs = self
            .client
            .get(&endpoint)
            .send()
            .await?
            .json::<Vec<Org>>()
            .await?
            .iter()
            .map(|Org { name }| name.clone())
            .collect();

        Ok(orgs)
    }
}
