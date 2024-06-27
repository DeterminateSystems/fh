use std::process::ExitCode;

use clap::Parser;

use super::CommandExecute;

/// Prints the URL of a given FlakeHub flake
#[derive(Parser, Debug)]
pub(crate) struct GetURLSubcommand {
    /// The FlakeHub reference to print as a URL
    ///
    /// A FlakeHub reference is of a form like `NixOS/nixpkgs` or `NixOS/nixpkgs/0.2305.*`
    pub(crate) input_ref: String,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for GetURLSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let (_, flake_input_url) =
            crate::cli::cmd::add::infer_flake_input_name_url(self.api_addr, self.input_ref, None)
                .await?;

        println!("{}", flake_input_url);

        Ok(ExitCode::SUCCESS)
    }
}
