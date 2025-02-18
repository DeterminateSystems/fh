use std::process::ExitCode;

use clap::Parser;
use color_eyre::eyre::{self, WrapErr as _};
use color_eyre::Result;
use tokio::fs;
use tokio::process::Command;

use crate::cli::cmd::nix_command;
use crate::shared::create_temp_netrc;

use super::{CommandExecute, FlakeHubClient};

/// First line of the error message printed by Nix when --out-link isn't
/// supported. We use this as a feature test to determine which copying we do.
const OUT_LINK_NOT_SUPPORTED: &[u8] = b"error: unrecognised flag '--out-link'";

#[derive(Parser)]
pub(crate) struct FetchSubcommand {
    /// The FlakeHub flake reference to fetch.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    flake_ref: String,

    /// Output link to store paths in, a la Nix's `--out-link` option.
    #[clap(long)]
    out_link: Option<String>,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    cache_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for FetchSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> Result<ExitCode> {
        let parsed = super::parse_flake_output_ref(&self.frontend_addr, &self.flake_ref)?;

        let resolved_path = FlakeHubClient::resolve(
            self.api_addr.as_str(),
            &parsed,
            /* use scoped token */ true,
        )
        .await?;

        tracing::info!(
            "Resolved {} to {}",
            self.flake_ref,
            resolved_path.store_path
        );

        let token = match resolved_path.token {
            Some(token) => token,
            None => eyre::bail!("Did not receive a scoped token from FlakeHub!"),
        };

        let dir = tempfile::tempdir()?;

        let cache_host = self
            .cache_addr
            .host_str()
            .expect("malformed URL: missing host");

        let netrc_path = create_temp_netrc(dir.path(), cache_host, &token).await?;
        let token_path = netrc_path.display().to_string();

        let out_link = self.out_link.as_deref();

        copy(
            self.cache_addr.as_str(),
            &resolved_path.store_path,
            token_path,
            out_link,
        )
        .await?;

        dir.close()?;

        Ok(ExitCode::SUCCESS)
    }
}

#[tracing::instrument(skip_all)]
async fn copy(
    cache_host: &str,
    store_path: &str,
    token_path: String,
    out: Option<&str>,
) -> Result<()> {
    match out {
        None => copy_without_out_link(cache_host, store_path, token_path).await,
        Some(out) => {
            if copy_supports_out_link().await? {
                copy_with_out_link(cache_host, store_path, token_path, out).await
            } else {
                copy_with_manual_symlink(cache_host, store_path, token_path, out).await
            }
        }
    }
}

async fn copy_supports_out_link() -> Result<bool> {
    // Not using nix_command() here because we need to read the stderr of the resulting command
    let output = Command::new("nix")
        .args(["copy", "--out-link"])
        .output()
        .await
        .wrap_err("Could not run nix")?;

    // Grab only the first line of output from nix since it's the one we care about (the problem it encountered)
    let error_line = output.stderr.split(|&c| c == b'\n').next();
    let error_line = match error_line {
        Some(line) => line,
        None => {
            tracing::warn!("Could not determine if `nix copy` supports --out-link; falling back to manual links");
            return Ok(false);
        }
    };

    let supported = error_line != OUT_LINK_NOT_SUPPORTED;
    tracing::debug!(supported, "Setting support for nix copy --out-link");

    Ok(supported)
}

async fn copy_without_out_link(
    cache_host: &str,
    store_path: &str,
    token_path: String,
) -> Result<()> {
    let args = vec![
        "copy".into(),
        "--option".into(),
        "narinfo-cache-negative-ttl".into(),
        "0".into(),
        "--from".into(),
        cache_host.into(),
        store_path.into(),
        "--netrc-file".into(),
        token_path,
    ];

    nix_command(&args, false)
        .await
        .wrap_err("Failed to copy resolved store path with Nix")?;

    tracing::info!("Fetched {store_path}");
    Ok(())
}

async fn copy_with_out_link(
    cache_host: &str,
    store_path: &str,
    token_path: String,
    out: &str,
) -> Result<()> {
    let args = vec![
        "copy".into(),
        "--option".into(),
        "narinfo-cache-negative-ttl".into(),
        "0".into(),
        "--from".into(),
        cache_host.into(),
        store_path.into(),
        "--out-link".into(),
        out.into(),
        "--netrc-file".into(),
        token_path,
    ];

    nix_command(&args, false)
        .await
        .wrap_err("Failed to copy resolved store path with Nix")?;

    tracing::info!("Fetched {store_path} to {out}");
    Ok(())
}

async fn copy_with_manual_symlink(
    cache_host: &str,
    store_path: &str,
    token_path: String,
    out: &str,
) -> Result<()> {
    copy_without_out_link(cache_host, store_path, token_path).await?;

    // TODO: figure out how to make this a GC root
    fs::symlink(store_path, out)
        .await
        .wrap_err_with(|| format!("Could not create symbolic link from {store_path} to {out}"))?;

    tracing::info!("Created manual symbolic link from {store_path} to {out}");
    Ok(())
}
