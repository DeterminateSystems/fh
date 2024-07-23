mod nixos;

use std::{os::unix::prelude::PermissionsExt, path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand};
use color_eyre::eyre::Context;

use crate::cli::{
    cmd::{nix_command, parse_output_ref},
    error::FhError,
};

use self::nixos::NixOS;

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
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let (profile, output_ref) = match &self.system {
            System::NixOS(NixOS { output_ref, .. }) => ("system", output_ref),
        };

        tracing::info!("Resolving store path for output: {}", output_ref);

        let output_ref = parse_output_ref(output_ref.to_string())?;

        let resolved_path = FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        let profile_path = apply_path_to_profile(profile, &resolved_path.store_path).await?;

        match self.system {
            System::NixOS(NixOS { run, .. }) => {
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

                    if let Ok(switch_bin_path_metadata) =
                        tokio::fs::metadata(&switch_bin_path).await
                    {
                        let permissions = switch_bin_path_metadata.permissions();
                        if permissions.mode() & 0o111 != 0 {
                            if let Some(verb) = run {
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
