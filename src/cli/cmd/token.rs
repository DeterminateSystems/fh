use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::cli::cmd::CommandExecute;

/// Generate a FlakeHub authentication token
#[derive(Debug, Parser)]
pub(crate) struct TokenSubcommand {
    #[command(subcommand)]
    cmd: TokenSubcommands,
}

impl CommandExecute for TokenSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        Ok(ExitCode::SUCCESS)
    }
}

#[derive(Debug, Subcommand)]
enum TokenSubcommands {
    /// Generate a device token
    Device {
        /// A description for the token
        #[arg(long)]
        description: String,
    },
}

impl CommandExecute for TokenSubcommands {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        match self {
            TokenSubcommands::Device { description: _ } => {}
        }

        Ok(ExitCode::SUCCESS)
    }
}
