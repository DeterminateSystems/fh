mod cli;

use clap::Parser;

use crate::cli::{
    cmd::{CommandExecute, FhSubcommands},
    Cli,
};

#[tokio::main]
async fn main() -> color_eyre::Result<std::process::ExitCode> {
    use FhSubcommands::*;

    let Cli { subcommand, .. } = Cli::parse();

    match subcommand {
        Add(add) => add.execute().await,
        List(list) => list.execute().await,
        Search(search) => search.execute().await,
    }
}
