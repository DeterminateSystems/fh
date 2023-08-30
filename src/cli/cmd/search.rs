use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{row, Attr, Cell, Row, Table};
use std::process::ExitCode;

use crate::cli::FLAKEHUB_WEB_ROOT;

use super::{CommandExecute, FlakeHubClient, TABLE_FORMAT};

/// Searches FlakeHub for flakes that match your query.
#[derive(Debug, Parser)]
pub(crate) struct SearchSubcommand {
    /// The search query.
    query: String,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(serde_derive::Deserialize)]
pub struct SearchResult {
    org: String,
    project: String,
    #[allow(dead_code)]
    description: Option<String>,
    #[allow(dead_code)]
    tags: Option<Vec<String>>,
}

impl SearchResult {
    fn name(&self) -> String {
        format!("{}/{}", self.org, self.project)
    }

    fn url(&self) -> String {
        format!("{}/flake/{}/{}", FLAKEHUB_WEB_ROOT, self.org, self.project)
    }
}

#[async_trait::async_trait]
impl CommandExecute for SearchSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner());

        let client = FlakeHubClient::new(&self.api_addr)?;

        match client.search(self.query).await {
            Ok(results) => {
                if results.is_empty() {
                    println!("No results");
                } else {
                    let mut table = Table::new();
                    table.set_format(*TABLE_FORMAT);
                    table.set_titles(row!["Flake", "FlakeHub URL"]);

                    for flake in results {
                        table.add_row(Row::new(vec![
                            Cell::new(&flake.name()).with_style(Attr::Bold),
                            Cell::new(&flake.url()).with_style(Attr::Dim),
                        ]));
                    }

                    table.printstd();
                }
            }
            Err(e) => {
                println!("Error: {e}");
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
