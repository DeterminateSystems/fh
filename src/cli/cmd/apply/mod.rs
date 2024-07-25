mod home_manager;
#[cfg(target_os = "macos")]
mod nix_darwin;
#[cfg(target_os = "linux")]
mod nixos;

use std::{
    os::unix::prelude::PermissionsExt,
    path::PathBuf,
    process::{ExitCode, Stdio},
};

use clap::{Parser, Subcommand};
use color_eyre::eyre::Context;

use crate::{
    cli::{
        cmd::{nix_command, parse_flake_output_ref},
        error::FhError,
    },
    path,
};

use self::home_manager::{HomeManager, HOME_MANAGER_SCRIPT};

#[cfg(target_os = "linux")]
use self::nixos::{NixOs, NIXOS_PROFILE, NIXOS_SCRIPT};

#[cfg(target_os = "macos")]
use self::nix_darwin::{NixDarwin, DARWIN_REBUILD_ACTION, NIX_DARWIN_PROFILE, NIX_DARWIN_SCRIPT};

use super::{CommandExecute, FlakeHubClient};

/// Apply the configuration at the specified FlakeHub output reference to the current system
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

    #[cfg(target_os = "macos")]
    /// Resolve the store path for a nix-darwin configuration and run its activation script
    NixDarwin(NixDarwin),

    #[cfg(target_os = "linux")]
    /// Apply the resolved store path on a NixOS system
    #[clap(name = "nixos")]
    NixOs(NixOs),
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let output_ref = match &self.system {
            System::HomeManager(home_manager) => home_manager.output_ref()?,
            #[cfg(target_os = "linux")]
            System::NixOs(nixos) => nixos.output_ref()?,
            #[cfg(target_os = "macos")]
            System::NixDarwin(nix_darwin) => nix_darwin.output_ref()?,
        };

        tracing::info!("Resolving store path for output: {}", output_ref);

        let output_ref = parse_flake_output_ref(&output_ref)?;

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
            #[cfg(target_os = "macos")]
            System::NixDarwin(_) => {
                let profile_path =
                    apply_path_to_profile(NIX_DARWIN_PROFILE, &resolved_path.store_path).await?;

                // {path}/sw/bin/darwin-rebuild
                let script_path = path!(&profile_path, "sw", "bin", NIX_DARWIN_SCRIPT);

                run_script(
                    script_path,
                    Some(DARWIN_REBUILD_ACTION.to_string()),
                    NIX_DARWIN_SCRIPT,
                )
                .await?;
            }
            #[cfg(target_os = "linux")]
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

                let mut cmd = tokio::process::Command::new(&script_path);
                if let Some(action) = action {
                    cmd.arg(action);
                }

                let output = cmd
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .wrap_err("failed to spawn Nix command")?
                    .wait_with_output()
                    .await
                    .wrap_err(format!("failed to run {script_name} script"))?;

                println!("{}", String::from_utf8_lossy(&output.stdout));
            }
        }
    }

    Ok(())
}

async fn apply_path_to_profile(profile: &str, store_path: &str) -> Result<String, FhError> {
    let profile_path = format!("/nix/var/nix/profiles/{profile}");

    tracing::info!(
        "Applying resolved store path {} to profile at {}",
        store_path,
        profile_path
    );

    nix_command(
        &[
            "build",
            "--print-build-logs",
            // `--max-jobs 0` ensures that `nix build` doesn't really *build* anything
            // and acts more as a fetch operation
            "--max-jobs",
            "0",
            "--profile",
            &profile_path,
            store_path,
        ],
        true,
    )
    .await
    .wrap_err("failed to build resolved store path with Nix")?;

    tracing::info!(
        "Successfully applied resolved path {} to profile at {profile_path}",
        store_path
    );

    Ok(profile_path)
}
