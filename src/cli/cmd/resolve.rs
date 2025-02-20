use std::process::ExitCode;

use clap::Parser;
use serde::{Deserialize, Serialize};

use super::{parse_flake_output_ref, print_json, CommandExecute, FlakeHubClient};

/// Resolves a FlakeHub flake reference into a store path.
#[derive(Debug, Parser)]
pub(crate) struct ResolveSubcommand {
    /// The FlakeHub flake reference to resolve.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    flake_ref: String,

    /// Output the result as JSON displaying the store path plus the original attribute path.
    #[arg(long, env = "FH_OUTPUT_JSON")]
    json: bool,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ResolvedPath {
    // The original attribute path, i.e. attr_path in {org}/{flake}/{version}#{attr_path}
    attribute_path: String,
    // The resolved store path
    pub(crate) store_path: String,
    // A JWT that can only substitute the closure of this store path
    pub(crate) token: Option<String>,
}

impl CommandExecute for ResolveSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let output_ref = parse_flake_output_ref(&self.frontend_addr, &self.flake_ref)?;

        let resolved_path =
            FlakeHubClient::resolve(self.api_addr.as_ref(), &output_ref, false).await?;

        tracing::debug!(
            "Successfully resolved reference {} to path {}",
            &output_ref,
            &resolved_path.store_path
        );

        if self.json {
            print_json(resolved_path)?;
        } else {
            println!("{}", resolved_path.store_path);
        }

        Ok(ExitCode::SUCCESS)
    }
}
