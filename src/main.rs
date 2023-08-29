mod cli;

use clap::Parser;

use crate::cli::{
    cmd::{CommandExecute, FhSubcommands},
    Cli,
};

#[tokio::main]
async fn main() -> color_eyre::Result<std::process::ExitCode> {
    use FhSubcommands::*;

    let Cli { subcommand, host } = Cli::parse();

    match subcommand {
        Add(add) => add.execute(&host).await,
        List(list) => list.execute(&host).await,
        Search(search) => search.execute(&host).await,
    }
}
