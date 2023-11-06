use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use tabled::{Table, Tabled};
use std::io::IsTerminal;
use std::process::ExitCode;
use url::Url;

use super::{print_json, FhError};
use crate::cli::cmd::{FlakeHubClient, DEFAULT_STYLE};

use super::CommandExecute;

pub(crate) const FLAKEHUB_WEB_ROOT: &str = "https://flakehub.com";

/// Lists key FlakeHub resources.
#[derive(Parser)]
pub(crate) struct ListSubcommand {
    #[command(subcommand)]
    cmd: Subcommands,

    /// Output results as JSON.
    #[arg(long, global = true)]
    json: bool,

    #[arg(from_global)]
    api_addr: url::Url,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Flake {
    pub(crate) org: String,
    pub(crate) project: String,
}

impl Flake {
    fn name(&self) -> String {
        format!("{}/{}", self.org, self.project)
    }

    fn url(&self) -> Url {
        let mut url = Url::parse(FLAKEHUB_WEB_ROOT)
            .expect("failed to parse flakehub web root url (this should never happen)");
        {
            let mut segs = url
                .path_segments_mut()
                .expect("flakehub url cannot be base (this should never happen)");

            segs.push("flake").push(&self.org).push(&self.project);
        }
        url
    }
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


#[derive(Deserialize, Serialize)]
pub(crate) struct Version {
    version: semver::Version,
    simplified_version: semver::Version,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Org {
    pub(crate) name: String,
}

#[derive(Deserialize, Serialize, Tabled)]
pub(crate) struct Release {
    pub(crate) version: String,
}

#[derive(Subcommand)]
enum Subcommands {
    /// Lists all currently public flakes on FlakeHub.
    Flakes,
    /// Lists all public flakes with the provided label.
    Label { label: String },
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
                        } else if self.json {
                            print_json(&flakes)?;
                        } else {
                            let rows = flakes.into_iter().map(Into::into).collect::<Vec<FlakeRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                csv::Writer::from_writer(std::io::stdout()).serialize(rows)?;
                            }
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Label { label } => {
                if string_has_whitespace(&label) {
                    return Err(FhError::LabelParse(String::from("whitespace not allowed")).into());
                }

                let label = label.to_lowercase();

                match client.flakes_by_label(&label).await {
                    Ok(flakes) => {
                        if flakes.is_empty() {
                            eprintln!("No results");
                        } else if self.json {
                            print_json(&flakes)?;
                        } else {
                            let rows = flakes.into_iter().map(Into::into).collect::<Vec<FlakeRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                csv::Writer::from_writer(std::io::stdout()).serialize(rows)?;
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
                        } else if self.json {
                            print_json(&orgs)?;
                        } else {
                            let rows = orgs.into_iter().map(Into::into).collect::<Vec<OrgRow>>();

                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                csv::Writer::from_writer(std::io::stdout()).serialize(rows)?;
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
                        } else if self.json {
                            print_json(&releases)?;
                        } else {
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(releases);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                csv::Writer::from_writer(std::io::stdout()).serialize(releases)?;
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
                        } else if self.json {
                            print_json(&versions)?;
                        } else {
                            let rows = versions.into_iter().map(|v| (flake.clone(), v).into()).collect::<Vec<VersionRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                csv::Writer::from_writer(std::io::stdout()).serialize(rows)?;
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

fn string_has_whitespace(s: &str) -> bool {
    s.chars().any(char::is_whitespace)
}


#[derive(Tabled, serde::Serialize)]
struct OrgRow {
    organization: String,
    #[tabled(rename = "FlakeHub URL")]
    flakehub_url: Url,
}

impl From<Org> for OrgRow {
    fn from(value: Org) -> Self {
        let mut url = Url::parse(FLAKEHUB_WEB_ROOT).expect(
            "failed to parse flakehub web root url (this should never happen)",
        );

        {
            let mut segs = url.path_segments_mut().expect(
                "flakehub url cannot be base (this should never happen)",
            );

            segs.push("org").push(&value.name);
        }

        Self { organization: value.name, flakehub_url: url }
    }
}

#[derive(Tabled, serde::Serialize)]
struct VersionRow {
    simplified_version: semver::Version,
    #[tabled(rename = "FlakeHub URL")]
    flakehub_url: Url,
    full_version: semver::Version,
}

impl From<(Flake, Version)> for VersionRow {
    fn from((flake, version): (Flake, Version)) -> Self {
        let mut url = Url::parse(FLAKEHUB_WEB_ROOT).expect(
            "failed to parse flakehub web root url (this should never happen)",
        );

        {
            let mut path_segments_mut = url.path_segments_mut().expect(
                "flakehub url cannot be base (this should never happen)",
            );

            path_segments_mut
                .push("flake")
                .push(&flake.org)
                .push(&flake.project)
                .push(&version.simplified_version.to_string());
        }

        Self { simplified_version: version.simplified_version, flakehub_url: url, full_version: version.version }
    }
}


#[derive(Tabled, serde::Serialize)]
struct FlakeRow {
    flake: String,
    #[tabled(rename = "FlakeHub URL")]
    flakehub_url: Url,
}

impl From<Flake> for FlakeRow {
    fn from(value: Flake) -> Self {
        let mut url = Url::parse(FLAKEHUB_WEB_ROOT).expect(
            "failed to parse flakehub web root url (this should never happen)",
        );

        {
            let mut segs = url.path_segments_mut().expect(
                "flakehub url cannot be base (this should never happen)",
            );

            segs.push("org").push(&value.org);
        }

        Self { flake: value.name(), flakehub_url: value.url() }
    }
}