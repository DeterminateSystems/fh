mod home_manager;
mod nixos;

use std::{os::unix::prelude::PermissionsExt, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use color_eyre::eyre::Context;

use crate::{
    cli::{
        cmd::{nix_command, parse_output_ref},
        error::FhError,
    },
    path,
};

use self::{
    home_manager::{HomeManager, HOME_MANAGER_SCRIPT},
    nixos::{NixOS, NIXOS_PROFILE, NIXOS_SCRIPT},
};

use super::{CommandExecute, FlakeHubClient};

/// Update the specified Nix profile with the path resolved from a FlakeHub output reference.
#[derive(Parser)]
pub(crate) struct ApplySubcommand {
    #[clap(subcommand)]
    system: System,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Subcommand)]
enum System {
    /// Apply the resolved store path on a NixOS system
    #[clap(name = "nixos")]
    NixOS(NixOS),

    /// Resolve the store path for a Home Manager configuration and run its activate script
    HomeManager(HomeManager),
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let output_ref = match &self.system {
            System::NixOS(nixos) => nixos.output_ref()?,
            System::HomeManager(home_manager) => home_manager.output_ref()?,
        };

        tracing::info!("Resolving store path for output: {}", output_ref);

        let output_ref = parse_output_ref(&output_ref)?;

        let resolved_path = FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        match self.system {
            System::NixOS(NixOS { action, .. }) => {
                let profile_path =
                    apply_path_to_profile(NIXOS_PROFILE, &resolved_path.store_path).await?;

                let script_path = path!(&profile_path, "bin", NIXOS_SCRIPT);

                tracing::debug!(
                    "Checking for {} script at {}",
                    NIXOS_SCRIPT,
                    &script_path.display().to_string(),
                );

                if script_path.exists() && script_path.is_file() {
                    tracing::debug!(
                        "Found {} script at {}",
                        NIXOS_SCRIPT,
                        &script_path.display().to_string(),
                    );

                    if let Ok(script_path_metadata) = tokio::fs::metadata(&script_path).await {
                        let permissions = script_path_metadata.permissions();
                        if permissions.mode() & 0o111 != 0 {
                            tracing::info!(
                                "{} {}",
                                &script_path.display().to_string(),
                                action.to_string(),
                            );

                            // switch-to-configuration <action>
                            let output = tokio::process::Command::new(&script_path)
                                .args([&action.to_string()])
                                .output()
                                .await
                                .wrap_err("failed to run switch-to-configuration script")?;

                            println!("{}", String::from_utf8_lossy(&output.stdout));
                        }
                    }
                }
            }
            System::HomeManager(_) => {
                let script_path = path!(&resolved_path.store_path, HOME_MANAGER_SCRIPT);

                tracing::debug!(
                    "Checking for {} script at {}",
                    HOME_MANAGER_SCRIPT,
                    &script_path.display().to_string(),
                );

                if script_path.exists() && script_path.is_file() {
                    tracing::debug!(
                        "Found {} script at {}",
                        HOME_MANAGER_SCRIPT,
                        &script_path.display().to_string(),
                    );

                    if let Ok(script_path_metadata) = tokio::fs::metadata(&script_path).await {
                        let permissions = script_path_metadata.permissions();
                        if permissions.mode() & 0o111 != 0 {
                            tracing::info!("{}", &script_path.display().to_string());

                            // activate
                            let output = tokio::process::Command::new(&script_path)
                                .output()
                                .await
                                .wrap_err("failed to run activate script")?;

                            println!("{}", String::from_utf8_lossy(&output.stdout));
                        }
                    }
                }
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}

async fn apply_path_to_profile(profile: &str, store_path: &str) -> Result<String, FhError> {
    let profile_path = format!("/nix/var/nix/profiles/{profile}");

    tracing::debug!("Successfully located Nix profile at {profile_path}");

    if let Ok(path) = tokio::fs::metadata(&profile_path).await {
        if !path.is_dir() {
            tracing::debug!(
                "Profile path {path:?} exists but isn't a directory; this should never happen"
            );
            return Err(FhError::MissingProfile(profile_path.to_string()));
        }
    } else {
        return Err(FhError::MissingProfile(profile_path.to_string()));
    }

    tracing::info!("Building resolved store path with Nix: {}", store_path);

    nix_command(&[
        "build",
        "--print-build-logs",
        // `--max-jobs 0` ensures that `nix build` doesn't really *build* anything
        // and acts more as a fetch operation
        "--max-jobs",
        "0",
        "--profile",
        &profile_path,
        store_path,
    ])
    .await
    .wrap_err("failed to build resolved store path with Nix")?;

    tracing::info!(
        "Successfully applied resolved path {} to profile at {profile_path}",
        store_path
    );

    Ok(profile_path)
}
