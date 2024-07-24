mod home_manager;
mod nix_darwin;
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
    nix_darwin::{NixDarwin, NIX_DARWIN_ACTION, NIX_DARWIN_SCRIPT},
    nixos::{NixOs, NIXOS_PROFILE, NIXOS_SCRIPT},
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
    /// Resolve the store path for a Home Manager configuration and run its activation script
    HomeManager(HomeManager),

    /// Resolve the store path for a nix-darwin configuration and run its activation script
    NixDarwin(NixDarwin),

    /// Apply the resolved store path on a NixOS system
    #[clap(name = "nixos")]
    NixOs(NixOs),
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let output_ref = match &self.system {
            System::NixOs(nixos) => nixos.output_ref()?,
            System::HomeManager(home_manager) => home_manager.output_ref()?,
            System::NixDarwin(nix_darwin) => nix_darwin.output_ref()?,
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
            System::HomeManager(_) => {
                // /nix/store/{path}/activate
                let script_path = path!(&resolved_path.store_path, HOME_MANAGER_SCRIPT);
                run_script(script_path, None, HOME_MANAGER_SCRIPT).await?;
            }
            System::NixDarwin(_) => {
                // /nix/store/{path}/sw/bin/darwin-rebuild
                let script_path = path!(&resolved_path.store_path, "sw", "bin", NIX_DARWIN_SCRIPT);
                run_script(
                    script_path,
                    Some(NIX_DARWIN_ACTION.to_string()),
                    NIX_DARWIN_SCRIPT,
                )
                .await?;
            }
            System::NixOs(NixOs { action, .. }) => {
                let profile_path =
                    apply_path_to_profile(NIXOS_PROFILE, &resolved_path.store_path).await?;

                let script_path = path!(&profile_path, "bin", NIXOS_SCRIPT);

                run_script(script_path, Some(action.to_string()), NIXOS_SCRIPT).await?;
            }
        }

        Ok(ExitCode::SUCCESS)
    }
}

async fn run_script(
    script_path: PathBuf,
    action: Option<String>,
    script_name: &str,
) -> Result<(), FhError> {
    tracing::debug!(
        "Checking for {} script at {}",
        script_name,
        &script_path.display().to_string(),
    );

    if script_path.exists() && script_path.is_file() {
        tracing::debug!(
            "Found {} script at {}",
            script_name,
            &script_path.display().to_string(),
        );

        if let Ok(script_path_metadata) = tokio::fs::metadata(&script_path).await {
            let permissions = script_path_metadata.permissions();
            if permissions.mode() & 0o111 != 0 {
                if let Some(action) = &action {
                    tracing::info!("{} {}", &script_path.display().to_string(), action);
                } else {
                    tracing::info!("{}", &script_path.display().to_string());
                }

                let output = if let Some(action) = &action {
                    tokio::process::Command::new(&script_path)
                        .arg(action)
                        .output()
                        .await
                        .wrap_err(format!("failed to run {script_name} script"))?
                } else {
                    tokio::process::Command::new(&script_path)
                        .output()
                        .await
                        .wrap_err(format!("failed to run {script_name} script"))?
                };

                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
        }
    }

    Ok(())
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
