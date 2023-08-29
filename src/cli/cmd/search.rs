use clap::Parser;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::ExitCode;

use super::{CommandExecute, FlakeHubClient};

/// Searches FlakeHub for flakes that match your query.
#[derive(Debug, Parser)]
pub(crate) struct SearchSubcommand {
    /// The search query.
    query: String,

    #[clap(from_global)]
    host: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for SearchSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let pb = ProgressBar::new_spinner();
        pb.set_style(ProgressStyle::default_spinner());

        let client = FlakeHubClient::new(&self.host)?;

        match client.search(self.query).await {
            Ok(results) => {
                if results.is_empty() {
                    println!("No results");
                } else {
                    for SearchResult { org, project, .. } in results {
                        println!(
                            "{}{}{}\n    {}/flake/{}/{}",
                            style(org.clone()).cyan(),
                            style("/").white(),
                            style(project.clone()).red(),
                            self.host,
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

        Ok(ExitCode::SUCCESS)
    }
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
