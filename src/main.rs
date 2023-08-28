mod cli;

use clap::Parser;

use crate::cli::cmd::CommandExecute;
use crate::cli::cmd::FhSubcommands;

#[tokio::main]
async fn main() -> color_eyre::Result<std::process::ExitCode> {
    let cli = cli::Cli::parse();

    match cli.subcommand {
        FhSubcommands::Add(add) => add.execute().await,
    }
}
