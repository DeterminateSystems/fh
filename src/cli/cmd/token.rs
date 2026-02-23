use std::process::ExitCode;

use clap::{Parser, Subcommand};
use uuid::Uuid;

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
        match self.cmd {
            TokenSubcommands::Device { cmd } => match cmd {
                DeviceSubcommands::List { org } => {
                    let tokens =
                        FlakeHubClient::list_device_tokens(self.api_addr.as_ref(), &org).await?;
                    for token in tokens {
                        println!("{token}");
                    }
                }
                DeviceSubcommands::Create { org, description } => {
                    let token = FlakeHubClient::generate_device_token(
                        self.api_addr.as_ref(),
                        &org,
                        &description,
                    )
                    .await?;
                    println!("{token}");
                }

                DeviceSubcommands::Revoke { org, token_id } => {
                    FlakeHubClient::revoke_device_token(self.api_addr.as_ref(), &org, &token_id)
                        .await?;
                    println!("Token for {org} with ID {token_id} successfully revoked");
                }
            },
        }

        Ok(ExitCode::SUCCESS)
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
    /// List available device tokens for your org
    #[command(
        long_about = "List all device tokens associated with your FlakeHub organization. This operation is restricted to admins of the org. Only coarse-grained tokens are currently supported."
    )]
    List {
        /// The FlakeHub organization for which you want to list tokens
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        org: String,
    },

    /// Generate a device token for your org
    #[command(
        long_about = "Generate a device token for your FlakeHub organization. This operation is restricted to admins of the org. Only coarse-grained tokens are currently supported."
    )]
    Create {
        /// The FlakeHub organization for which you want to generate the token
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        org: String,

        /// A description for the token (must be a non-empty string)
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        description: String,
    },

    /// Revoke a device token associated with your org
    #[command(
        long_about = "Revoke a device token associated with your FlakeHub organization. This operation is restricted to admins of the org."
    )]
    Revoke {
        /// The FlakeHub organization for which the token was generated
        #[arg(short, long, value_parser = clap::builder::NonEmptyStringValueParser::new())]
        org: String,

        /// The token's unique ID
        #[arg(short = 'i', long, value_parser = parse_uuid)]
        token_id: Uuid,
    },
}

fn parse_uuid(s: &str) -> Result<Uuid, String> {
    Uuid::parse_str(s).map_err(|e| format!("failed to parse UUID: {e}"))
}
