use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::{io::IsTerminal, process::ExitCode};
use tabled::{Table, Tabled};
use url::Url;

use crate::flakehub_url;

use super::{CommandExecute, FlakeHubClient, list::FLAKEHUB_WEB_ROOT, print_json};

/// Searches FlakeHub for flakes that match your query.
#[derive(Debug, Parser)]
pub(crate) struct SearchSubcommand {
    /// The search query.
    query: String,

    /// The maximum number of search results to return.
    #[clap(short, long, default_value = "10")]
    max_results: usize,

    /// Output results as JSON.
    #[clap(long, env = "FH_OUTPUT_JSON")]
    json: bool,

    #[clap(from_global)]
    api_addr: url::Url,

    /// Maximum number of results.
    #[arg(short, long)]
    limit: Option<usize>,
}

#[derive(Deserialize, Serialize)]
pub struct SearchResult {
    org: String,
    project: String,
}

impl SearchResult {
    fn name(&self) -> String {
        format!("{}/{}", self.org, self.project)
    }

    fn url(&self) -> Url {
        flakehub_url!(FLAKEHUB_WEB_ROOT, "flake", &self.org, &self.project)
    }
}

#[derive(Tabled, serde::Serialize)]
pub struct SearchResultRow {
    name: String,
    url: Url,
}

impl From<SearchResult> for SearchResultRow {
    fn from(value: SearchResult) -> Self {
        Self {
            name: value.name(),
            url: value.url(),
        }
    }
}

impl CommandExecute for SearchSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner());

        match FlakeHubClient::search(self.api_addr.as_ref(), self.query, self.limit).await {
            Ok(results) => {
                if results.is_empty() {
                    eprintln!("No results");
                } else if self.json {
                    print_json(&results)?;
                } else {
                    let rows: Vec<SearchResultRow> = results
                        .into_iter()
                        .take(self.max_results)
                        .map(Into::into)
                        .collect();

                    if std::io::stdout().is_terminal() {
                        let table = Table::new(rows);
                        println!("{table}");
                    } else {
                        csv::Writer::from_writer(std::io::stdout()).serialize(rows)?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
