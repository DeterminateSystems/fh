use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::{Client as HttpClient, ClientBuilder};
use serde::Deserialize;

#[derive(Debug, Parser)]
struct Search {
    /// The search query.
    query: String,
}

#[derive(Parser)]
enum Subcommand {
    /// Search FlakeHub for flakes that match your query
    Search(Search),
    /// List all public flakes available on FlakeHub
    Flakes,
    /// List all public orgs available on FlakeHub
    Orgs,
}

/// fh: a CLI tool for interacting with FlakeHub.
#[derive(Parser)]
struct Cli {
    /// The FlakeHub server address.
    #[clap(
        long,
        env = "FLAKEHUB_HOST",
        default_value = "https://api.flakehub.com"
    )]
    host: String,

    #[clap(subcommand)]
    cmd: Subcommand,
}

#[derive(Deserialize)]
struct SearchResult {
    org: String,
    project: String,
    #[allow(dead_code)]
    description: Option<String>,
    #[allow(dead_code)]
    tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Flake {
    org: String,
    project: String,
}

struct Client {
    host: String,
    client: HttpClient,
}

#[derive(Debug, thiserror::Error)]
enum FhError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

impl Client {
    fn new(host: String) -> Result<Self, FhError> {
        let client = ClientBuilder::new().build()?;

        Ok(Self { host, client })
    }

    async fn search(&self, query: String) -> Result<Vec<SearchResult>, FhError> {
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
        #[derive(Deserialize)]
        struct Org {
            name: String,
        }

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

#[tokio::main]
async fn main() -> Result<(), FhError> {
    let Cli { host, cmd } = Cli::parse();

    let client = Client::new(host)?;

    match cmd {
        Subcommand::Search(search) => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(ProgressStyle::default_spinner());
            let Search { query, .. } = search;
            match client.search(query).await {
                Ok(results) => {
                    if results.is_empty() {
                        println!("No results");
                    } else {
                        for SearchResult { org, project, .. } in results {
                            println!(
                                "{}{}{}\n    https://flakehub.com/flake/{}/{}",
                                style(org.clone()).cyan(),
                                style("/").white(),
                                style(project.clone()).red(),
                                style(org).cyan(),
                                style(project).red(),
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {e}");
                }
            }
        }
        Subcommand::Flakes => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(ProgressStyle::default_spinner());
            match client.flakes().await {
                Ok(flakes) => {
                    if flakes.is_empty() {
                        println!("No results");
                    } else {
                        for Flake { org, project } in flakes {
                            println!(
                                "{}{}{}\n    https://flakehub.com/flake/{}/{}",
                                style(org.clone()).cyan(),
                                style("/").white(),
                                style(project.clone()).red(),
                                style(org).cyan(),
                                style(project).red(),
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {e}");
                }
            }
        }
        Subcommand::Orgs => {
            let pb = ProgressBar::new_spinner();
            pb.set_style(ProgressStyle::default_spinner());
            match client.orgs().await {
                Ok(orgs) => {
                    if orgs.is_empty() {
                        println!("No results");
                    } else {
                        for org in orgs {
                            println!(
                                "{}\n    https://flakehub.com/org/{}",
                                style(org.clone()).cyan(),
                                style(org).cyan(),
                            );
                        }
                    }
                }
                Err(e) => {
                    println!("Error: {e}");
                }
            }
        }
    }

    Ok(())
}
