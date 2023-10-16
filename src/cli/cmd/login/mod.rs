use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tokio::io::AsyncWriteExt;

use super::CommandExecute;

/// Login to FlakeHub in order to allow authenticated fetching of flakes.
#[derive(Debug, Parser)]
pub(crate) struct LoginSubcommand {
    /// Skip following up a successful login with `fh status`.
    #[clap(long)]
    skip_status: bool,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for LoginSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        self.manual_login().await?;

        Ok(ExitCode::SUCCESS)
    }
}

impl LoginSubcommand {
    async fn manual_login(&self) -> color_eyre::Result<()> {
        // FIXME: this should really be the frontend, but the frontend doesn't have a /login path
        // yet...
        let mut login_url = self.api_addr.clone();
        login_url.set_path("login");
        login_url.set_query(Some("redirect=/token/create"));

        println!("Login to FlakeHub: {}", login_url);
        println!("And then follow the prompts below:");
        println!();

        let token = crate::cli::cmd::init::prompt::Prompt::maybe_string("Paste your token here:");
        let token = match token {
            Some(token) => {
                // FIXME: validate that the token is valid?
                // or at least validate that it's a... jwt at all lol
                token
            }
            None => {
                tracing::error!("Missing token.");
                std::process::exit(1);
            }
        };

        let xdg = xdg::BaseDirectories::new()?;

        // $XDG_CONFIG_HOME/nix/nix.conf; basically ~/.config/nix/nix.conf
        let nix_config_path = xdg.place_config_file("nix/nix.conf")?;
        // $XDG_DATA_HOME/fh/netrc; basically ~/.local/share/flakehub/netrc
        let netrc_path = xdg.place_data_file("flakehub/netrc")?;
        // $XDG_CONFIG_HOME/fh/auth; basically ~/.config/fh/auth
        let token_path = auth_token_path()?;

        let nix_config_addition = format!("\nnetrc-file = {}\n", netrc_path.display());
        let netrc_contents = format!(
            "\
            machine {frontend_host} login FIXME password {token}\n\
            machine {backend_host} login FIXME password {token}\n\
            ",
            frontend_host = self
                .frontend_addr
                .host_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("frontend_addr had no host"))?,
            backend_host = self
                .api_addr
                .host_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("api_addr had no host"))?,
        );

        // NOTE: Keep an eye on any movement in the following issues / PRs. Them being resolved
        // means we may be able to ditch setting `netrc-file` in favor of `access-tokens`. (The
        // benefit is that `access-tokens` can be appended to, but `netrc-file` is a one-time thing
        // so if the user has their own `netrc-file`, Nix will decide which one wins.)
        // https://github.com/NixOS/nix/pull/9145 ("WIP: Support access-tokens for fetching tarballs from private sources")
        // https://github.com/NixOS/nix/issues/8635 ("Credentials provider support for builtins.fetch*")
        // https://github.com/NixOS/nix/issues/8439 ("--access-tokens option does nothing")
        tokio::fs::write(netrc_path, &netrc_contents).await?;
        tokio::fs::write(token_path, token).await?;

        if crate::cli::cmd::init::prompt::Prompt::bool(&format!(
            "May I add `{}` to {}?",
            nix_config_addition.trim(),
            nix_config_path.display()
        )) {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(nix_config_path)
                .await?;
            file.write_all(nix_config_addition.as_bytes()).await?;
        } else {
            println!(
                "No problem! Please add the following contents to {}:\n{nix_config_addition}",
                nix_config_path.display()
            );
            println!(
                "Or add the following contents to your existing netrc file:\n\n{netrc_contents}"
            );
        }

        if !self.skip_status {
            crate::cli::cmd::status::get_status(self.api_addr.clone()).await?;
        }

        Ok(())
    }
}

pub(crate) fn auth_token_path() -> color_eyre::Result<PathBuf> {
    let xdg = xdg::BaseDirectories::new()?;
    // $XDG_CONFIG_HOME/fh/auth; basically ~/.config/fh/auth
    let token_path = xdg.place_config_file("flakehub/auth")?;

    Ok(token_path)
}
