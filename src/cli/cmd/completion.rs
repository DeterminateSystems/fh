use std::{io::stdout, process::ExitCode};

use crate::cli::Cli;
use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};

use super::CommandExecute;

/// Prints completion for shells to use.
#[derive(Parser)]
pub(crate) struct CompletionSubcommand {
    /// Shell
    #[arg(value_enum)]
    shell: Shell,
}

impl CommandExecute for CompletionSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let cli = &mut Cli::command();
        generate(self.shell, cli, cli.get_name().to_string(), &mut stdout());

        Ok(ExitCode::SUCCESS)
    }
}
