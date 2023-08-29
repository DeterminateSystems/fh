mod add;
mod list;
mod search;

use reqwest::{Client as HttpClient, ClientBuilder};

use crate::cli::cmd::list::Org;

use self::{list::Flake, search::SearchResult};

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
    host: String,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum FhError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

impl FlakeHubClient {
    pub(super) fn new(host: &str) -> Result<Self, FhError> {
        let client = ClientBuilder::new().build()?;
        Ok(Self {
            host: String::from(host),
            client,
        })
    }

    pub(super) async fn search(&self, query: String) -> Result<Vec<SearchResult>, FhError> {
        let params = [("q", query)];

        let endpoint = format!("{}/search", self.host);

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
        let endpoint = format!("{}/flakes", self.host);

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
        let endpoint = format!("{}/orgs", self.host);

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
