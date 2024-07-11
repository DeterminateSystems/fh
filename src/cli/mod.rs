pub(crate) mod cmd;
mod error;
pub(crate) mod instrumentation;

/// fh: a CLI for interacting with FlakeHub
#[derive(clap::Parser)]
#[command(version)]
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

    /// The FlakeHub cache to communicate with.
    ///
    /// Primarily useful for debugging FlakeHub.
    #[clap(
        global = true,
        long,
        default_value = "https://cache.flakehub.com",
        hide = true
    )]
    pub cache_addr: url::Url,

    /// The FlakeHub frontend address to communicate with.
    ///
    /// Primarily useful for debugging FlakeHub.
    #[clap(
        global = true,
        long,
        default_value = "https://flakehub.com",
        hide = true
    )]
    pub frontend_addr: url::Url,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,

    #[clap(flatten)]
    pub instrumentation: instrumentation::Instrumentation,
}
