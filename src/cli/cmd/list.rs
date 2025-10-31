use chrono::{DateTime, Utc};
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
    #[arg(long, global = true, env = "FH_OUTPUT_JSON")]
    json: bool,

    #[arg(from_global)]
    api_addr: url::Url,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Project {
    pub(crate) organization_name: String,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Tabled, Deserialize, Serialize)]
pub(crate) struct Release {
    #[tabled(rename = "Simplified version", display_with = "bold")]
    pub(crate) simplified_version: String,
    #[tabled(rename = "Version", display_with = "dimmed")]
    pub(crate) version: String,
    #[tabled(rename = "Revision", display_with = "dimmed")]
    pub(crate) revision: String,
    #[tabled(rename = "Published at", display_with = "tabled_opt_dim")]
    pub(crate) published_at: Option<DateTime<Utc>>,
    #[tabled(rename = "Updated at", display_with = "tabled_opt_dim")]
    pub(crate) updated_at: Option<DateTime<Utc>>,
    #[tabled(rename = "Commit count", display_with = "tabled_opt_dim")]
    pub(crate) commit_count: Option<i64>,
}

// Function for handling generic Option<T> in Tabled (dimmed)
fn tabled_opt_dim<T: std::fmt::Display>(v: &Option<T>) -> String {
    match v {
        Some(t) => dimmed(t.to_string()),
        None => String::new(),
    }
}

#[derive(Subcommand)]
enum Subcommands {
    /// Lists all currently public flakes on FlakeHub.
    Flakes {
        #[arg(long)]
        /// List flakes owned by this FlakeHub account.
        /// Includes private flakes your account has access to.
        owner: Option<String>,

        /// Maximum number of results.
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Lists all public flakes with the provided label.
    Label {
        label: String,

        /// Maximum number of results.
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// Lists all currently public organizations on FlakeHub.
    Orgs {
        /// Maximum number of results.
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// List all releases for a specific flake on FlakeHub.
    Releases {
        /// The flake for which you want to list releases.
        flake: String,

        /// Maximum number of results.
        #[arg(short, long)]
        limit: Option<usize>,
    },
    /// List all versions that match the provided version constraint.
    Versions {
        /// The flake for which you want to list compatible versions.
        flake: String,
        /// The version constraint as a string.
        constraint: String,

        /// Maximum number of results.
        #[arg(short, long)]
        limit: Option<usize>,
    },
}

impl CommandExecute for ListSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        use Subcommands::*;

        match self.cmd {
            Flakes { owner, limit } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                match FlakeHubClient::flakes(self.api_addr.as_ref(), owner, limit).await {
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
            Label { label, limit } => {
                if string_has_whitespace(&label) {
                    return Err(FhError::LabelParse(String::from("whitespace not allowed")).into());
                }

                let label = label.to_lowercase();

                match FlakeHubClient::flakes_by_label(self.api_addr.as_ref(), &label, limit).await {
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
            Orgs { limit } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                match FlakeHubClient::orgs(self.api_addr.as_ref(), limit).await {
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
            Releases { flake, limit } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                let flake = Flake::try_from(flake)?;

                let releases = FlakeHubClient::releases(
                    self.api_addr.as_ref(),
                    &flake.org,
                    &flake.project,
                    limit,
                )
                .await?;

                if releases.is_empty() {
                    eprintln!("No results");
                } else if self.json {
                    print_json(&releases)?;
                } else if std::io::stdout().is_terminal() {
                    let mut table = Table::new(releases);
                    table.with(DEFAULT_STYLE.clone());
                    println!("{table}");
                } else {
                    let mut writer = csv::Writer::from_writer(std::io::stdout());
                    for release in releases {
                        writer.serialize(release)?;
                    }
                }
            }
            Versions {
                flake,
                constraint,
                limit,
            } => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(ProgressStyle::default_spinner());

                let flake = Flake::try_from(flake)?.clone();

                match FlakeHubClient::versions(
                    self.api_addr.as_ref(),
                    &flake.org,
                    &flake.project,
                    &constraint,
                    limit,
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
    organization: String,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
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
    simplified_version: semver::Version,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
    flakehub_url: Url,
    #[tabled(rename = "Full version", display_with = "dimmed")]
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
    flake: String,
    #[tabled(rename = "FlakeHub URL", display_with = "dimmed")]
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

fn dimmed(v: impl ToString) -> String {
    v.to_string().dimmed().to_string()
}

fn bold(v: impl ToString) -> String {
    v.to_string().bold().to_string()
}
