use std::{fmt::Display, os::unix::prelude::PermissionsExt, path::PathBuf, process::ExitCode};

use clap::{Parser, ValueEnum};
use color_eyre::eyre::Context;

use crate::cli::{
    cmd::{nix_command, parse_output_ref},
    error::FhError,
};

use super::{CommandExecute, FlakeHubClient};

/// Update the specified Nix profile with the path resolved from a flake output reference.
#[derive(Parser)]
pub(crate) struct ApplySubcommand {
    /// The Nix profile to which you want to apply the resolved store path.
    #[arg(env = "FH_RESOLVE_PROFILE")]
    profile: String,

    /// The FlakeHub output reference to apply to the profile.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    output_ref: String,

    /// Output the result as JSON displaying the store path plus the original attribute path.
    #[arg(long, env = "FH_JSON_OUTPUT")]
    json: bool,

    /// The command to run with the profile: bin/switch-to-configuration <verb>
    #[arg(long, env = "FH_RESOLVE_VERB")]
    verb: Option<Verb>,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Verb {
    Switch,
    Boot,
    Test,
    DryActivate,
}

impl Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Switch => "switch",
                Self::Boot => "boot",
                Self::Test => "test",
                Self::DryActivate => "dry-activate",
            }
        )
    }
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        tracing::info!("Resolving store path for output: {}", self.output_ref);

        let output_ref = parse_output_ref(self.output_ref)?;

        let resolved_path = FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        let profile_path = if self.profile.starts_with("/nix/var/nix/profiles") {
            self.profile
        } else {
            format!("/nix/var/nix/profiles/{}", self.profile)
        };

        tracing::debug!("Successfully located Nix profile at {profile_path}");

        if let Ok(path) = tokio::fs::metadata(&profile_path).await {
            if !path.is_dir() {
                tracing::debug!(
                    "Profile path {path:?} exists but isn't a directory; this should never happen"
                );
                return Err(FhError::MissingProfile(profile_path).into());
            }
        } else {
            return Err(FhError::MissingProfile(profile_path).into());
        }

        tracing::info!(
            "Building resolved store path with Nix: {}",
            &resolved_path.store_path,
        );

        nix_command(&[
            "build",
            "--print-build-logs",
            "--max-jobs",
            "0",
            "--profile",
            &profile_path,
            &resolved_path.store_path,
        ])
        .await
        .wrap_err("failed to build resolved store path with Nix")?;

        tracing::info!(
            "Successfully applied resolved path {} to profile at {profile_path}",
            &resolved_path.store_path
        );

        let switch_bin_path = {
            let mut path = PathBuf::from(&profile_path);
            path.push("bin");
            path.push("switch-to-configuration");
            path
        };

        tracing::debug!(
            "Checking for switch-to-configuration executable at {}",
            &switch_bin_path.display().to_string(),
        );

        if switch_bin_path.exists() && switch_bin_path.is_file() {
            tracing::debug!(
                "Found switch-to-configuration executable at {}",
                &switch_bin_path.display().to_string(),
            );

            if let Ok(switch_bin_path_metadata) = tokio::fs::metadata(&switch_bin_path).await {
                let permissions = switch_bin_path_metadata.permissions();
                if permissions.mode() & 0o111 != 0 {
                    if let Some(verb) = self.verb {
                        tracing::info!(
                            "Switching configuration by running {} {}",
                            &switch_bin_path.display().to_string(),
                            verb.to_string(),
                        );

                        let output = tokio::process::Command::new(&switch_bin_path)
                            .args([&verb.to_string()])
                            .output()
                            .await
                            .wrap_err("failed to run switch-to-configuration")?;

                        println!("{}", String::from_utf8_lossy(&output.stdout));
                    } else {
                        tracing::info!(
                            "Successfully resolved path {} to profile {}",
                            &resolved_path.store_path,
                            profile_path
                        );

                        println!("For more information on how to update your machine:\n\n    {profile_path}/bin/switch-to-configuration --help");
                    }
                } else {
                    tracing::debug!(
                        "switch-to-configuration executable at {} isn't executable; skipping",
                        &switch_bin_path.display().to_string()
                    );
                }
            }
        } else {
            tracing::debug!(
                "No switch-to-configuration executable found at {}",
                &switch_bin_path.display().to_string(),
            );
        }

        Ok(ExitCode::SUCCESS)
    }
}
