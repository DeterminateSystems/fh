use std::process::ExitCode;

use clap::Parser;

use super::CommandExecute;

/// Prints the URL of the given flake
#[derive(Parser, Debug)]
pub(crate) struct GetURLSubcommand {
    /// The name of the flake input.
    ///
    /// If not provided, it will be inferred from the provided input URL (if possible).
    #[clap(long)]
    pub(crate) input_name: Option<String>,
    /// The flake reference to add as an input.
    ///
    /// A reference in the form of `NixOS/nixpkgs` or `NixOS/nixpkgs/0.2305.*` (without a URL
    /// scheme) will be inferred as a FlakeHub input.
    pub(crate) input_ref: String,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for GetURLSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let (_, flake_input_url) =
            crate::cli::cmd::add::infer_flake_input_name_url(self.api_addr, self.input_ref, self.input_name).await?;

        println!("{}", flake_input_url);

        Ok(ExitCode::SUCCESS)
    }
}
