use std::process::ExitCode;

use clap::Parser;
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

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let output_ref = parse_output_ref(self.output_ref)?;

        let resolved_path = FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        let profile = if self.profile.starts_with("/nix/var/nix/profiles") {
            self.profile
        } else {
            format!("/nix/var/nix/profiles/{}", self.profile)
        };

        tracing::debug!("Successfully located Nix profile {profile}");

        if let Ok(path) = tokio::fs::metadata(&profile).await {
            tracing::debug!("Profile path {path:?} exists but isn't a directory");

            if !path.is_dir() {
                return Err(FhError::MissingProfile(profile).into());
            }
        } else {
            return Err(FhError::MissingProfile(profile).into());
        }

        tracing::debug!(
            "Running: nix build --print-build-logs --max-jobs 0 --profile {} {}",
            &profile,
            &resolved_path.store_path,
        );

        nix_command(&[
            "build",
            "--print-build-logs",
            "--max-jobs",
            "0",
            "--profile",
            &profile,
            &resolved_path.store_path,
        ])
        .await
        .wrap_err("failed to build resolved store path with Nix")?;

        tracing::info!(
            "Successfully applied resolved path {} to profile {profile}",
            &resolved_path.store_path
        );

        Ok(ExitCode::SUCCESS)
    }
}
