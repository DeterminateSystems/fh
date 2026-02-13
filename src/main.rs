pub(crate) mod cli;
pub(crate) mod dnixd;
pub(crate) mod shared;

use std::io::IsTerminal;

use clap::Parser;

use crate::cli::{
    Cli,
    cmd::{CommandExecute, FhSubcommands},
};

const DETERMINATE_STATE_DIR: &str = "/nix/var/determinate";
const DETERMINATE_NIXD_SOCKET_NAME: &str = "determinate-nixd.socket";
const DETERMINATE_NIXD_TOKEN_NAME: &str = "token";

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"),);

#[tokio::main]
async fn main() -> color_eyre::Result<std::process::ExitCode> {
    color_eyre::config::HookBuilder::default()
        .issue_url(concat!(env!("CARGO_PKG_REPOSITORY"), "/issues/new"))
        .add_issue_metadata("version", env!("CARGO_PKG_VERSION"))
        .add_issue_metadata("os", std::env::consts::OS)
        .add_issue_metadata("arch", std::env::consts::ARCH)
        .theme(if !std::io::stderr().is_terminal() {
            color_eyre::config::Theme::new()
        } else {
            color_eyre::config::Theme::dark()
        })
        .install()?;

    let cli = Cli::parse();
    cli.instrumentation.setup().await?;

    match cli.subcommand {
        FhSubcommands::Add(add) => add.execute().await,
        FhSubcommands::Apply(apply) => apply.execute().await,
        FhSubcommands::Completion(completion) => completion.execute().await,
        FhSubcommands::Convert(convert) => convert.execute().await,
        FhSubcommands::Eject(eject) => eject.execute().await,
        FhSubcommands::Fetch(fetch) => fetch.execute().await,
        FhSubcommands::Init(init) => init.execute().await,
        FhSubcommands::List(list) => list.execute().await,
        FhSubcommands::Login(login) => login.execute().await,
        FhSubcommands::Resolve(resolve) => resolve.execute().await,
        FhSubcommands::Search(search) => search.execute().await,
        FhSubcommands::Status(status) => status.execute().await,
    }
}
