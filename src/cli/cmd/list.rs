use clap::{Parser, Subcommand};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::process::ExitCode;

use crate::cli::cmd::FlakeHubClient;

use super::CommandExecute;

/// Lists key FlakeHub resources.
#[derive(Parser)]
pub(crate) struct ListSubcommand {
    #[command(subcommand)]
    cmd: Subcommands,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Subcommand)]
enum Subcommands {
    /// Lists all currently public flakes on FlakeHub.
    Flakes,
    /// List all currently public organizations on FlakeHub.
    Orgs,
}

#[async_trait::async_trait]
impl CommandExecute for ListSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        use Subcommands::*;

        let client = FlakeHubClient::new(&self.api_addr)?;

        match self.cmd {
            Flakes => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                match client.flakes().await {
                    Ok(flakes) => {
                        if flakes.is_empty() {
                            println!("No results");
                        } else {
                            for Flake { org, project } in flakes {
                                println!(
                                    "{}{}{}\n    {}/flake/{}/{}",
                                    style(org.clone()).cyan(),
                                    style("/").white(),
                                    style(project.clone()).red(),
                                    self.api_addr,
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
            Orgs => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                match client.orgs().await {
                    Ok(orgs) => {
                        if orgs.is_empty() {
                            println!("No results");
                        } else {
                            for org in orgs {
                                println!(
                                    "{}\n    {}/org/{}",
                                    style(org.clone()).cyan(),
                                    self.api_addr,
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

        Ok(ExitCode::SUCCESS)
    }
}

#[derive(serde_derive::Deserialize)]
pub(super) struct Flake {
    org: String,
    project: String,
}

#[derive(serde_derive::Deserialize)]
pub(super) struct Org {
    pub(super) name: String,
}
