use std::process::ExitCode;

use clap::Parser;
use color_eyre::eyre;
use color_eyre::Result;

use crate::cli::cmd::copy_closure_with_gc_root;
use crate::shared::create_temp_netrc;

use super::{CommandExecute, FlakeHubClient};

#[derive(Parser)]
pub(crate) struct FetchSubcommand {
    /// The FlakeHub flake reference to fetch.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    flake_ref: String,

    /// Target link to use as a Nix garbage collector root
    target: String,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    cache_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
}

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

        let netrc_path = create_temp_netrc(dir.path(), &self.cache_addr, &token).await?;
        let token_path = netrc_path.display().to_string();

        copy_closure_with_gc_root(
            self.cache_addr.as_str(),
            &resolved_path.store_path,
            token_path,
            &self.target,
        )
        .await?;

        tracing::info!("Copied {} to {}", resolved_path.store_path, self.target);

        dir.close()?;

        Ok(ExitCode::SUCCESS)
    }
}
