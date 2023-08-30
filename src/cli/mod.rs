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

    #[clap(
        global = true,
        long,
        default_value = "https://api.flakehub.com",
        hide = true
    )]
    pub backend_host: String,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
