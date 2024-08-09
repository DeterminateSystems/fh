mod home_manager;
mod nix_darwin;
mod nixos;

use std::{
    os::unix::prelude::PermissionsExt,
    path::{Path, PathBuf},
    process::{ExitCode, Stdio},
};

use clap::{Parser, Subcommand};
use color_eyre::eyre::Context;
use tempfile::{tempdir, TempDir};

use crate::cli::{cmd::nix_command, error::FhError};

use self::{home_manager::HomeManager, nix_darwin::NixDarwin, nixos::NixOs};

use super::{CommandExecute, FlakeHubClient};

/// Apply the configuration at the specified FlakeHub output reference to the current system
#[derive(Parser)]
pub(crate) struct ApplySubcommand {
    #[clap(subcommand)]
    system: System,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
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

pub trait ApplyType {
    fn get_ref(&self) -> &str;

    fn default_ref(&self) -> String;

    fn profile_path(&self) -> Option<&Path>;

    fn requires_root(&self) -> bool;

    fn relative_path(&self) -> &Path;

    fn action(&self) -> Option<String>;
}

#[async_trait::async_trait]
impl CommandExecute for ApplySubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let applyer: Box<&(dyn ApplyType + Send + Sync)> = match &self.system {
            System::HomeManager(home_manager) => Box::new(home_manager),
            System::NixOs(nixos) => Box::new(nixos),
            System::NixDarwin(nix_darwin) => Box::new(nix_darwin),
        };

        let output_ref = {
            parse_output_ref(
                &self.frontend_addr,
                applyer.get_ref(),
                &applyer.default_ref(),
            )?
        };

        tracing::info!("Resolving {}", output_ref);

        let resolved_path = FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        let (profile_path, _tempdir) = apply_path_to_profile(
            applyer.profile_path(),
            &resolved_path.store_path,
            applyer.requires_root(),
        )
        .await?;

        let script_path = profile_path.join(applyer.relative_path());

        run_script(
            script_path,
            applyer.action(),
            &applyer
                .relative_path()
                .file_name()
                .expect("The apply type should absolutely have a file name.")
                .to_string_lossy(),
        )
        .await?;

        Ok(ExitCode::SUCCESS)
    }
}

// This function enables you to provide simplified paths:
//
// fh apply nixos omnicorp/systems/0.1
//
// Here, `omnicorp/systems/0.1` resolves to `omnicorp/systems/0.1#nixosConfigurations.$(hostname)`.
// If you need to apply a configuration at a path that doesn't conform to this pattern, you
// can still provide an explicit path.
fn parse_output_ref(
    frontend_addr: &url::Url,
    output_ref: &str,
    default_path: &str,
) -> Result<super::FlakeOutputRef, FhError> {
    let with_default_output_path = match output_ref.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => output_ref.to_string(),
        [_release] => format!("{}#{}", output_ref, default_path),
        _ => return Err(FhError::MalformedOutputRef(output_ref.to_string())),
    };

    let output_ref =
        super::parse_flake_output_ref(frontend_addr, &with_default_output_path)?.to_string();

    let parsed = super::validate_release_ref(&output_ref)?;

    parsed.try_into()
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
            // The expected application script exists
            let permissions = script_path_metadata.permissions();
            if permissions.mode() & 0o111 != 0 {
                // The expected application script is executable
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

async fn apply_path_to_profile(
    input_profile_path: Option<&Path>,
    store_path: &str,
    sudo_if_necessary: bool,
) -> Result<(PathBuf, Option<TempDir>), FhError> {
    let temp_handle: Option<TempDir>;

    let profile_path: PathBuf = if let Some(profile_path) = input_profile_path {
        temp_handle = None;
        tracing::info!(
            "Applying resolved store path {} to profile at {}",
            store_path,
            profile_path.display()
        );

        profile_path.into()
    } else {
        let dir = tempdir()?;
        let profile_path = dir.path().join("profile");

        temp_handle = Some(dir);
        profile_path
    };

    nix_command(
        &[
            "build",
            "--print-build-logs",
            // `--max-jobs 0` ensures that `nix build` doesn't really *build* anything
            // and acts more as a fetch operation
            "--max-jobs",
            "0",
            "--profile",
            profile_path.to_str().ok_or(FhError::InvalidProfile)?,
            store_path,
        ],
        sudo_if_necessary,
    )
    .await
    .wrap_err("failed to build resolved store path with Nix")?;

    if input_profile_path.is_some() {
        tracing::info!(
            "Successfully applied resolved path {} to profile at {}",
            store_path,
            profile_path.display()
        );
    }

    Ok((profile_path, temp_handle))
}

#[cfg(test)]
mod tests {
    use super::parse_output_ref;

    #[test]
    fn test_parse_output_ref() {
        let cases: Vec<(&str, &str)> = vec![
            ("foo/bar/*", "foo/bar/*#DefaultFooBar"),
            ("foo/bar/0.1.*", "foo/bar/0.1.*#DefaultFooBar"),
            (
                "omnicorp/web/0.1.2#homeConfigurations.my-config",
                "omnicorp/web/0.1.2#homeConfigurations.my-config",
            ),
            (
                "omnicorp/web/0.1.2#packages.x86_64-linux.default",
                "omnicorp/web/0.1.2#packages.x86_64-linux.default",
            ),
            (
                "https://flakehub.com/f/omnicorp/web/0.1.2#packages.x86_64-linux.default",
                "omnicorp/web/0.1.2#packages.x86_64-linux.default",
            ),
        ];

        for (input, expect) in cases {
            assert_eq!(
                &parse_output_ref(
                    &url::Url::parse("https://flakehub.com/f").unwrap(),
                    input,
                    "DefaultFooBar"
                )
                .unwrap_or_else(|_| panic!("failing case: {input}"))
                .to_string(),
                expect,
            );
        }
    }
}
