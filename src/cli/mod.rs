pub(crate) mod cmd;

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
