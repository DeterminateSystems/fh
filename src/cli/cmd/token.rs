use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cli::cmd::{CommandExecute, FlakeHubClient};

/// Manage FlakeHub authentication tokens
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
            Device { cmd } => match cmd {
                DeviceSubcommands::Create { org, description } => {
                    let token = FlakeHubClient::generate_device_token(
                        self.api_addr.as_ref(),
                        &org,
                        &description,
                    )
                    .await?;
                    println!("{token}");
                    Ok(ExitCode::SUCCESS)
                }
            },
        }
    }
}

#[derive(Debug, Subcommand)]
enum TokenSubcommands {
    /// Manage FlakeHub device tokens
    Device {
        #[command(subcommand)]
        cmd: DeviceSubcommands,
    },
}

#[derive(Debug, Subcommand)]
enum DeviceSubcommands {
    /// Generate a device token for your org.
    #[command(
        long_about = "Generate a device token for your FlakeHub organization. This operation is restricted to admins of the organization. Only coarse-grained tokens are currently supported."
    )]
    Create {
        /// The FlakeHub organization for which you want to generate the token
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        org: String,

        /// A description for the token (must be a non-empty string)
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        description: String,
    },
}
