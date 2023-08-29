pub(crate) mod cmd;

/// fh: a CLI for interacting with FlakeHub
#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[clap(
        global = true,
        long,
        default_value = "https://flakehub.com",
        hide = true
    )]
    pub host: String,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
