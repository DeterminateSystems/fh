use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{row, Attr, Cell, Row, Table};
use serde::Deserialize;
use std::io::IsTerminal;
use std::process::ExitCode;

use super::TABLE_FORMAT;
use crate::cli::{cmd::FlakeHubClient, FLAKEHUB_WEB_ROOT};

use super::CommandExecute;

/// Lists key FlakeHub resources.
#[derive(Parser)]
pub(crate) struct ListSubcommand {
    #[command(subcommand)]
    cmd: Subcommands,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Deserialize)]
pub(super) struct Flake {
    org: String,
    project: String,
}

#[derive(Deserialize)]
pub(super) struct Version {
    version: String,
    simplified_version: String,
}

impl Flake {
    fn name(&self) -> String {
        format!("{}/{}", self.org, self.project)
    }

    fn url(&self) -> String {
        format!("{}/flake/{}/{}", FLAKEHUB_WEB_ROOT, self.org, self.project)
    }
}

#[derive(Deserialize)]
pub(super) struct Org {
    pub(super) name: String,
}

#[derive(Deserialize)]
pub(super) struct Release {
    version: String,
}

#[derive(Subcommand)]
enum Subcommands {
    /// Lists all currently public flakes on FlakeHub.
    Flakes,
    /// Lists all currently public organizations on FlakeHub.
    Orgs,
    /// List all releases for a specific flake on FlakeHub.
    Releases { flake: String },
    /// List all versions that match the provided version constraint.
    Versions { flake: String, constraint: String },
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
                            eprintln!("No results");
                        } else {
                            let mut table = Table::new();
                            table.set_format(*TABLE_FORMAT);
                            table.set_titles(row!["Flake", "FlakeHub URL"]);

                            for flake in flakes {
                                table.add_row(Row::new(vec![
                                    Cell::new(&flake.name()).with_style(Attr::Bold),
                                    Cell::new(&flake.url()).with_style(Attr::Dim),
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
            }
            Orgs => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                match client.orgs().await {
                    Ok(orgs) => {
                        if orgs.is_empty() {
                            eprintln!("No results");
                        } else {
                            let mut table = Table::new();
                            table.set_format(*TABLE_FORMAT);
                            table.set_titles(row!["Organization", "FlakeHub URL"]);

                            for org in orgs {
                                let url = format!("{}/org/{}", FLAKEHUB_WEB_ROOT, org);
                                table.add_row(Row::new(vec![
                                    Cell::new(&org).with_style(Attr::Bold),
                                    Cell::new(&url).with_style(Attr::Dim),
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
            }
            Releases { flake } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                match client.releases(flake).await {
                    Ok(releases) => {
                        if releases.is_empty() {
                            eprintln!("No results");
                        } else {
                            let mut table = Table::new();
                            table.set_format(*TABLE_FORMAT);
                            table.set_titles(row!["Version"]);

                            for release in releases {
                                table.add_row(Row::new(vec![Cell::new(&release.version)]));
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
            }
            Versions { flake, constraint } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());
                match client.versions(flake.clone(), constraint.clone()).await {
                    Ok(versions) => {
                        if versions.is_empty() {
                            eprintln!("No versions match the provided constraint");
                        } else {
                            let mut table = Table::new();
                            table.set_format(*TABLE_FORMAT);
                            table.set_titles(row!["Simplified version", "Full version"]);

                            for version in versions {
                                table.add_row(Row::new(vec![
                                    Cell::new(&version.simplified_version).with_style(Attr::Bold),
                                    Cell::new(&version.version).with_style(Attr::Dim),
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
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
