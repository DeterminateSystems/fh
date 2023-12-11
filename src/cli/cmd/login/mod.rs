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

        let token = crate::cli::cmd::init::prompt::Prompt::maybe_token("Paste your token here:");
        let (token, status) = match token {
            Some(token) => {
                // This serves as validating that provided token is actually a JWT, and is valid.
                let status = crate::cli::cmd::status::get_status_from_auth_token(
                    self.api_addr.clone(),
                    &token,
                )
                .await?;
                (token, status)
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
        let netrc_file_path = xdg.place_data_file("flakehub/netrc")?;
        // $XDG_CONFIG_HOME/fh/auth; basically ~/.config/fh/auth
        let token_path = auth_token_path()?;

        let netrc_file_string = netrc_file_path.display().to_string();
        let nix_config_addition = format!("\nnetrc-file = {}\n", netrc_file_string);
        let netrc_contents = format!(
            "\
            machine {frontend_host} login flakehub password {token}\n\
            machine {backend_host} login flakehub password {token}\n\
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
        tokio::fs::write(&netrc_file_path, &netrc_contents).await?;
        tokio::fs::write(&token_path, token).await?;

        let nix_config =
            nix_config_parser::NixConfig::parse_file(&nix_config_path).unwrap_or_default();
        let mut merged_nix_config = nix_config.clone();
        let maybe_existing_netrc_file = merged_nix_config
            .settings_mut()
            .insert("netrc-file".to_string(), netrc_file_string.clone());

        let maybe_prompt = match maybe_existing_netrc_file {
            // If the setting is the same as we'd set, we don't need to touch the file at all.
            Some(existing_netrc_file) if existing_netrc_file == netrc_file_string => None,
            // If the settings are different, ask if we can change it.
            Some(existing_netrc_file) => Some(format!(
                "May I change `netrc-file` from `{}` to `{}`?",
                existing_netrc_file, netrc_file_string
            )),
            // If there is no `netrc-file` setting, ask if we can set it.
            None => Some(format!(
                "May I set `netrc-file` to `{}`?",
                netrc_file_string
            )),
        };

        let maybe_write_to_nix_conf =
            maybe_prompt.map(|prompt| crate::cli::cmd::init::prompt::Prompt::bool(&prompt));

        if let Some(write_to_nix_conf) = maybe_write_to_nix_conf {
            let mut write_success = None;
            if write_to_nix_conf {
                write_success = match tokio::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open(&nix_config_path)
                    .await
                {
                    Ok(mut file) => {
                        let nix_config_contents =
                            tokio::fs::read_to_string(&nix_config_path).await?;
                        let nix_config_contents =
                            merge_nix_configs(nix_config, nix_config_contents, merged_nix_config);
                        let write_status = file.write_all(nix_config_contents.as_bytes()).await;
                        Some(write_status.is_ok())
                    }
                    Err(_) => Some(false),
                };
            } else {
                print!("No problem! ");
            }

            let write_failed = write_success.is_some_and(|x| !x);
            if write_failed {
                print!("Writing to {} failed. ", nix_config_path.display());
            }

            if write_failed || !write_to_nix_conf {
                println!(
                    "Please add the following contents to {}:\n{nix_config_addition}",
                    nix_config_path.display()
                );
                println!(
                    "Or add the following contents to your existing netrc file:\n\n\
                    {netrc_contents}"
                );
            }
        }

        if !self.skip_status {
            print!("{status}");
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

// NOTE(cole-h): Adapted from
// https://github.com/DeterminateSystems/nix-installer/blob/0b0172547c4666f6b1eacb6561a59d6b612505a3/src/action/base/create_or_merge_nix_config.rs#L284
const NIX_CONF_COMMENT_CHAR: char = '#';
fn merge_nix_configs(
    mut existing_nix_config: nix_config_parser::NixConfig,
    mut existing_nix_config_contents: String,
    mut merged_nix_config: nix_config_parser::NixConfig,
) -> String {
    let mut new_config = String::new();

    // We append a newline to ensure that, in the case there are comments at the end of the
    // file and _NO_ trailing newline, we still preserve the entire block of comments.
    existing_nix_config_contents.push('\n');

    let (associated_lines, _, _) = existing_nix_config_contents.split('\n').fold(
        (Vec::new(), Vec::new(), false),
        |(mut all_assoc, mut current_assoc, mut associating): (
            Vec<Vec<String>>,
            Vec<String>,
            bool,
        ),
         line| {
            let line = line.trim();

            if line.starts_with(NIX_CONF_COMMENT_CHAR) {
                associating = true;
            } else if line.is_empty() || !line.starts_with(NIX_CONF_COMMENT_CHAR) {
                associating = false;
            }

            current_assoc.push(line.to_string());

            if !associating {
                all_assoc.push(current_assoc);
                current_assoc = Vec::new();
            }

            (all_assoc, current_assoc, associating)
        },
    );

    for line_group in associated_lines {
        if line_group.is_empty() || line_group.iter().all(|line| line.is_empty()) {
            continue;
        }

        // This expect should never reasonably panic, because we would need a line group
        // consisting solely of a comment and nothing else, but unconditionally appending a
        // newline to the config string before grouping above prevents this from occurring.
        let line_idx = line_group
            .iter()
            .position(|line| !line.starts_with(NIX_CONF_COMMENT_CHAR))
            .expect("There should always be one line without a comment character");

        let setting_line = &line_group[line_idx];
        let comments = line_group[..line_idx].join("\n");

        // If we're here, but the line without a comment char is empty, we have
        // standalone comments to preserve, but no settings with inline comments.
        if setting_line.is_empty() {
            for line in &line_group {
                new_config.push_str(line);
                new_config.push('\n');
            }

            continue;
        }

        // Preserve inline comments for settings we've merged
        let to_remove = if let Some((name, value)) = existing_nix_config
            .settings()
            .iter()
            .find(|(name, _value)| setting_line.starts_with(*name))
        {
            new_config.push_str(&comments);
            new_config.push('\n');
            new_config.push_str(name);
            new_config.push_str(" = ");

            if let Some(merged_value) = merged_nix_config.settings_mut().remove(name) {
                new_config.push_str(&merged_value);
                new_config.push(' ');
            } else {
                new_config.push_str(value);
            }

            if let Some(inline_comment_idx) = setting_line.find(NIX_CONF_COMMENT_CHAR) {
                let inline_comment = &setting_line[inline_comment_idx..];
                new_config.push_str(inline_comment);
                new_config.push('\n');
            }

            Some(name.clone())
        } else {
            new_config.push_str(&comments);
            new_config.push('\n');
            new_config.push_str(setting_line);
            new_config.push('\n');

            None
        };

        if let Some(to_remove) = to_remove {
            existing_nix_config.settings_mut().remove(&to_remove);
        }
    }

    // Add the leftover existing nix config
    for (name, value) in existing_nix_config.settings() {
        if merged_nix_config.settings().get(name).is_some() {
            continue;
        }

        new_config.push_str(name);
        new_config.push_str(" = ");
        new_config.push_str(value);
        new_config.push('\n');
    }

    new_config.push('\n');

    for (name, value) in merged_nix_config.settings() {
        new_config.push_str(name);
        new_config.push_str(" = ");
        new_config.push_str(value);
        new_config.push('\n');
    }

    new_config
        .strip_prefix('\n')
        .unwrap_or(&new_config)
        .to_owned()
}
