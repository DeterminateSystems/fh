use std::process::ExitCode;

use clap::Parser;
use serde::{Deserialize, Serialize};

use super::{nix_command, print_json, CommandExecute, FlakeHubClient};

/// Resolves a FlakeHub flake reference into a store path.
#[derive(Debug, Parser)]
pub(crate) struct ResolveSubcommand {
    /// The FlakeHub flake reference to resolve.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    flake_ref: String,

    /// Output the result as JSON displaying the store path plus the original attribute path.
    #[arg(long)]
    json: bool,

    /// Build the resolved path with Nix.
    #[arg(short, long, default_value_t = false)]
    build: bool,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ResolvedPath {
    // The original attribute path, i.e. attr_path in {org}/{flake}/{version}#{attr_path}
    attribute_path: String,
    // The resolved store path
    store_path: String,
}

#[async_trait::async_trait]
impl CommandExecute for ResolveSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        // Ensures that users can use otherwise-valid flake refs
        let flake_ref = self
            .flake_ref
            .strip_prefix("https://flakehub.com/f/")
            .unwrap_or(&self.flake_ref);

        let resolved_path =
            FlakeHubClient::resolve(self.api_addr.as_ref(), flake_ref.to_string()).await?;

        if self.build {
            nix_command(&[
                "build",
                "--max-jobs",
                "0",
                "--print-build-logs",
                &resolved_path.store_path,
            ])
            .await?;
        }

        if self.json {
            print_json(resolved_path)?;
        } else {
            println!("{}", resolved_path.store_path);
        }

        Ok(ExitCode::SUCCESS)
    }
}
