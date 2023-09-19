use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{row, Attr, Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::{io::IsTerminal, process::ExitCode};
use url::Url;

use super::{list::FLAKEHUB_WEB_ROOT, print_json, CommandExecute, FlakeHubClient, TABLE_FORMAT};

/// Searches FlakeHub for flakes that match your query.
#[derive(Debug, Parser)]
pub(crate) struct SearchSubcommand {
    /// The search query.
    query: String,

    /// The maximum number of search results to return.
    #[clap(short, long, default_value = "10")]
    max_results: usize,

    /// Output results as JSON.
    #[clap(long)]
    json: bool,

    #[clap(from_global)]
    api_addr: url::Url,
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

    fn url(&self) -> String {
        let mut url = Url::parse(FLAKEHUB_WEB_ROOT)
            .expect("failed to parse flakehub web root url (this should never happen)");
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
                } else if self.json {
                    print_json(&results)?;
                } else {
                    let mut table = Table::new();
                    table.set_format(*TABLE_FORMAT);
                    table.set_titles(row!["Flake", "FlakeHub URL"]);

                    let results: Vec<&SearchResult> =
                        results.iter().take(self.max_results).collect();

                    for result in results {
                        table.add_row(Row::new(vec![
                            Cell::new(&result.name()).with_style(Attr::Bold),
                            Cell::new(&result.url()).with_style(Attr::Dim),
                        ]));
                    }

                    if std::io::stdout().is_terminal() {
                        table.printstd();
                    } else {
                        table.to_csv(std::io::stdout())?;
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
