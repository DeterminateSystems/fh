use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::process::ExitCode;
use tabled::{Table, Tabled};
use url::Url;

use super::print_json;
use crate::{
    cli::{
        cmd::{FlakeHubClient, DEFAULT_STYLE},
        error::FhError,
    },
    flakehub_url,
};

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
        flakehub_url!(FLAKEHUB_WEB_ROOT, "flake", &self.org, &self.project)
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

#[derive(Deserialize, Serialize)]
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

        match self.cmd {
            Flakes => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                match FlakeHubClient::flakes(self.api_addr.as_ref()).await {
                    Ok(flakes) => {
                        if flakes.is_empty() {
                            eprintln!("No results");
                        } else if self.json {
                            print_json(&flakes)?;
                        } else {
                            let rows = flakes
                                .into_iter()
                                .map(Into::into)
                                .collect::<Vec<FlakeRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                let mut writer = csv::Writer::from_writer(std::io::stdout());
                                for row in rows {
                                    writer.serialize(row)?;
                                }
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

                match FlakeHubClient::flakes_by_label(self.api_addr.as_ref(), &label).await {
                    Ok(flakes) => {
                        if flakes.is_empty() {
                            eprintln!("No results");
                        } else if self.json {
                            print_json(&flakes)?;
                        } else {
                            let rows = flakes
                                .into_iter()
                                .map(Into::into)
                                .collect::<Vec<FlakeRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                let mut writer = csv::Writer::from_writer(std::io::stdout());
                                for row in rows {
                                    writer.serialize(row)?;
                                }
                            }
                        }
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Orgs => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                match FlakeHubClient::orgs(self.api_addr.as_ref()).await {
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
                                let mut writer = csv::Writer::from_writer(std::io::stdout());
                                for row in rows {
                                    writer.serialize(row)?;
                                }
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

                match FlakeHubClient::releases(self.api_addr.as_ref(), &flake.org, &flake.project)
                    .await
                {
                    Ok(releases) => {
                        let rows = releases
                            .into_iter()
                            .map(Into::into)
                            .collect::<Vec<ReleaseRow>>();

                        if rows.is_empty() {
                            eprintln!("No results");
                        } else if self.json {
                            print_json(&rows)?;
                        } else if std::io::stdout().is_terminal() {
                            let mut table = Table::new(rows);
                            table.with(DEFAULT_STYLE.clone());
                            println!("{table}");
                        } else {
                            let mut writer = csv::Writer::from_writer(std::io::stdout());
                            for row in rows {
                                writer.serialize(row)?;
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

                match FlakeHubClient::versions(
                    self.api_addr.as_ref(),
                    &flake.org,
                    &flake.project,
                    &constraint,
                )
                .await
                {
                    Ok(versions) => {
                        if versions.is_empty() {
                            eprintln!("No versions match the provided constraint");
                        } else if self.json {
                            print_json(&versions)?;
                        } else {
                            let rows = versions
                                .into_iter()
                                .map(|v| (flake.clone(), v).into())
                                .collect::<Vec<VersionRow>>();
                            if std::io::stdout().is_terminal() {
                                let mut table = Table::new(rows);
                                table.with(DEFAULT_STYLE.clone());
                                println!("{table}");
                            } else {
                                let mut writer = csv::Writer::from_writer(std::io::stdout());
                                for row in rows {
                                    writer.serialize(row)?;
                                }
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
    #[tabled(rename = "Organization", display_with = "bold")]
    #[serde(rename = "Organization")]
    organization: String,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
    #[serde(rename = "FlakeHub URL")]
    flakehub_url: Url,
}

impl From<Org> for OrgRow {
    fn from(value: Org) -> Self {
        let flakehub_url = flakehub_url!(FLAKEHUB_WEB_ROOT, "org", &value.name);

        Self {
            organization: value.name,
            flakehub_url,
        }
    }
}

#[derive(Tabled, serde::Serialize)]
struct VersionRow {
    #[tabled(rename = "Simplified version", display_with = "bold")]
    #[serde(rename = "Simplified version")]
    simplified_version: semver::Version,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
    #[serde(rename = "FlakeHub URL")]
    flakehub_url: Url,
    #[tabled(rename = "Full version", display_with = "dimmed")]
    #[serde(rename = "Full version")]
    full_version: semver::Version,
}

impl From<(Flake, Version)> for VersionRow {
    fn from((flake, version): (Flake, Version)) -> Self {
        let flakehub_url = flakehub_url!(
            FLAKEHUB_WEB_ROOT,
            "flake",
            &flake.org,
            &flake.project,
            &version.simplified_version.to_string()
        );

        Self {
            simplified_version: version.simplified_version,
            full_version: version.version,
            flakehub_url,
        }
    }
}

#[derive(Tabled, serde::Serialize)]
struct FlakeRow {
    #[tabled(rename = "Flake", display_with = "bold")]
    #[serde(rename = "Flake")]
    flake: String,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
    #[serde(rename = "FlakeHub URL")]
    flakehub_url: Url,
}

impl From<Flake> for FlakeRow {
    fn from(value: Flake) -> Self {
        Self {
            flake: value.name(),
            flakehub_url: value.url(),
        }
    }
}

#[derive(Tabled, serde::Serialize)]
pub(crate) struct ReleaseRow {
    #[serde(rename = "Version")]
    pub(crate) version: String,
}

impl From<Release> for ReleaseRow {
    fn from(value: Release) -> Self {
        Self {
            version: value.version,
        }
    }
}

fn dimmed(v: impl ToString) -> String {
    v.to_string().dimmed().to_string()
}

fn bold(v: impl ToString) -> String {
    v.to_string().bold().to_string()
}
