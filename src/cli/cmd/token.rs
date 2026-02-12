use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cli::cmd::{CommandExecute, FlakeHubClient};

/// Generate a FlakeHub authentication token
#[derive(Debug, Parser)]
pub(crate) struct TokenSubcommand {
    #[command(subcommand)]
    cmd: TokenSubcommands,

    #[clap(from_global)]
    api_addr: url::Url,
}

impl CommandExecute for TokenSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        use TokenSubcommands::*;

        match self.cmd {
            Device { org, description } => {
                let token = FlakeHubClient::generate_device_token(
                    self.api_addr.as_ref(),
                    &org,
                    &description,
                )
                .await?;
                println!("{token}");
                Ok(ExitCode::SUCCESS)
            }
        }
    }
}

#[derive(Debug, Subcommand)]
enum TokenSubcommands {
    /// Generate a coarse-grained device token for a specific organization
    Device {
        /// The FlakeHub organization for which you want to generate the token
        #[arg(short, long)]
        org: String,

        /// A description for the token
        #[arg(long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        description: String,
    },
}
