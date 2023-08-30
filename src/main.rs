mod cli;

use clap::Parser;

use crate::cli::{
    cmd::{CommandExecute, FhSubcommands},
    Cli,
};

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

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
