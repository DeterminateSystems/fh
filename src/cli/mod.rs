pub(crate) mod cmd;

/// fh: a CLI for interacting with FlakeHub, a platform for discovering and publishing Nix flakes from Determinate Systems.
#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[clap(long, default_value = "https://flakehub.com", hide = true)]
    pub host: String,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
