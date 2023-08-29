pub(crate) mod cmd;

/// fh: a CLI for interacting with FlakeHub
#[derive(clap::Parser)]
pub(crate) struct Cli {
    /// The FlakeHub address to communicate with.
    ///
    /// Primarily useful for debugging FlakeHub.
    #[clap(
        global = true,
        long,
        default_value = "https://flakehub.com",
        hide = true
    )]
    pub host: url::Url,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
