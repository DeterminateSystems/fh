use std::process::ExitCode;

use clap::Parser;
use serde::{Deserialize, Serialize};

use super::{print_json, CommandExecute, FlakeHubClient};

/// Resolves a FlakeHub flake reference into a store path.
#[derive(Debug, Parser)]
pub(crate) struct ResolveSubcommand {
    /// The FlakeHub flake reference to resolve.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    flake_ref: String,

    /// Display the result as JSON displaying the store path plus the original attribute path.
    #[arg(short, long)]
    json: bool,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ResolvedPath {
    // The original attribute path, e.g. attr_path in {org}/{flake}/{version}#{attr_path}
    attribute_path: String,
    // The resolved store path
    store_path: String,
}

#[async_trait::async_trait]
impl CommandExecute for ResolveSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let value = FlakeHubClient::resolve(self.api_addr.as_ref(), self.flake_ref).await?;

        if self.json {
            print_json(value)?;
        } else {
            println!("{}", value.store_path);
        }

        Ok(ExitCode::SUCCESS)
    }
}
