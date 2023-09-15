pub(crate) mod cmd;
mod instrumentation;

/// fh: a CLI for interacting with FlakeHub
#[derive(clap::Parser)]
pub(crate) struct Cli {
    /// The FlakeHub address to communicate with.
    ///
    /// Primarily useful for debugging FlakeHub.
    #[clap(
        global = true,
        long,
        default_value = "https://api.flakehub.com",
        hide = true
    )]
    pub api_addr: url::Url,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,

    #[clap(flatten)]
    pub instrumentation: instrumentation::Instrumentation,
}
