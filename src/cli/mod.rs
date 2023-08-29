pub(crate) mod cmd;

#[derive(clap::Parser)]
pub(crate) struct Cli {
    #[clap(long, default_value = "https://api.flakehub.com")]
    pub host: String,

    #[clap(subcommand)]
    pub subcommand: cmd::FhSubcommands,
}
