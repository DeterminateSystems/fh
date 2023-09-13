use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use prettytable::{row, Attr, Cell, Row, Table};
use serde::Deserialize;
use std::io::IsTerminal;
use std::process::ExitCode;
use url::Url;

use super::{FhError, TABLE_FORMAT};
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

#[derive(Clone, Deserialize)]
pub(super) struct Flake {
    pub(super) org: String,
    pub(super) project: String,
}

impl TryFrom<String> for Flake {
    type Error = FhError;

    fn try_from(flake_ref: String) -> Result<Self, Self::Error> {
        let (org, project) = match flake_ref.split('/').collect::<Vec<_>>()[..] {
            // `nixos/nixpkgs`
            [org, repo] => (org, repo),
            _ => {
                return Err(FhError::FlakeParse(format!(
                    "flake ref {flake_ref} invalid; must be of the form {{org}}/{{project}}"
                )))
            }
        };
        Ok(Self {
            org: String::from(org),
            project: String::from(project),
        })
    }
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
    Releases {
        /// The flake for which you want to list releases.
        flake: String,
    },
    /// List all versions that match the provided version constraint.
    Versions {
        /// The flake for which you want to list compatible versions.
        flake: String,
        /// The version constraint as a string.
        constraint: String,
    },
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
                                    Cell::new(&flake.url(&self.api_addr)).with_style(Attr::Dim),
                                ]));
                            }

                            if std::io::stdout().is_terminal() {
                                table.printstd();
                            } else {
                                table.to_csv(std::io::stdout())?;
                            }
                        }
                    }
                    Err(e) => return Err(e.into()),
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
                                let mut url = self.api_addr.clone();
                                {
                                    let mut segs = url.path_segments_mut().expect(
                                        "flakehub url cannot be base (this should never happen)",
                                    );
                                    segs.push("org").push(&org);
                                }

                                table.add_row(Row::new(vec![
                                    Cell::new(&org).with_style(Attr::Bold),
                                    Cell::new(url.as_ref()).with_style(Attr::Dim),
                                ]));
                            }

                            if std::io::stdout().is_terminal() {
                                table.printstd();
                            } else {
                                table.to_csv(std::io::stdout())?;
                            }
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Releases { flake } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                let flake = Flake::try_from(flake)?;

                match client.releases(&flake.org, &flake.project).await {
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
                    Err(e) => return Err(e.into()),
                }
            }
            Versions { flake, constraint } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                let flake = Flake::try_from(flake)?.clone();

                match client
                    .versions(&flake.org, &flake.project, &constraint)
                    .await
                {
                    Ok(versions) => {
                        if versions.is_empty() {
                            eprintln!("No versions match the provided constraint");
                        } else {
                            let mut table = Table::new();
                            table.set_format(*TABLE_FORMAT);
                            table.set_titles(row![
                                "Simplified version",
                                "Full version",
                                "FlakeHub URL"
                            ]);

                            for version in versions {
                                let mut flakehub_root = self.api_addr.clone();

                                {
                                    let mut path_segments_mut = flakehub_root
                                        .path_segments_mut()
                                        .expect(
                                        "flakehub url cannot be base (this should never happen)",
                                    );

                                    path_segments_mut
                                        .push("flake")
                                        .push(&flake.org)
                                        .push(&flake.project)
                                        .push(&version.simplified_version);
                                }

                                table.add_row(Row::new(vec![
                                    Cell::new(&version.simplified_version).with_style(Attr::Bold),
                                    Cell::new(&version.version).with_style(Attr::Dim),
                                    Cell::new(flakehub_root.as_ref())
                                        .with_style(Attr::Underline(true)),
                                ]));
                            }

                            if std::io::stdout().is_terminal() {
                                table.printstd();
                            } else {
                                table.to_csv(std::io::stdout())?;
                            }
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}
