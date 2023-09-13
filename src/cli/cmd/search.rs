use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{row, Attr, Cell, Row, Table};
use std::process::ExitCode;
use url::Url;

use super::{CommandExecute, FlakeHubClient, TABLE_FORMAT};

/// Searches FlakeHub for flakes that match your query.
#[derive(Debug, Parser)]
pub(crate) struct SearchSubcommand {
    /// The search query.
    query: String,

    #[clap(short, long, default_value = "10")]
    max_results: usize,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(serde_derive::Deserialize)]
pub struct SearchResult {
    org: String,
    project: String,
}

impl SearchResult {
    fn name(&self) -> String {
        format!("{}/{}", self.org, self.project)
    }

    fn url(&self, api_addr: &Url) -> String {
        let mut url = api_addr.clone();
        {
            let mut segs = url
                .path_segments_mut()
                .expect("flakehub url cannot be base (this should never happen)");
            segs.push("flake").push(&self.org).push(&self.project);
        }
        url.to_string()
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
                    eprintln!("No results");
                } else {
                    let mut table = Table::new();
                    table.set_format(*TABLE_FORMAT);
                    table.set_titles(row!["Flake", "FlakeHub URL"]);

                    let results: Vec<&SearchResult> =
                        results.iter().take(self.max_results).collect();

                    for flake in results {
                        table.add_row(Row::new(vec![
                            Cell::new(&flake.name()).with_style(Attr::Bold),
                            Cell::new(&flake.url(&self.api_addr)).with_style(Attr::Dim),
                        ]));
                    }

                    table.printstd();
                }
            }
            Err(e) => {
                eprintln!("Error: {e}");
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
