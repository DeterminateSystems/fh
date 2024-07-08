use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use tokio::io::AsyncWriteExt;

use crate::cli::cmd::FlakeHubClient;

use super::{CommandExecute, FhError};

const CACHE_PUBLIC_KEYS: &[&str; 2] = &[
    "cache.flakehub.com-1:t6986ugxCA+d/ZF9IeMzJkyqi5mDhvFIx7KA/ipulzE=",
    "cache.flakehub.com-2:ntBGiaKSmygJOw2j1hFS7KDlUHQWmZALvSJ9PxMJJYU=",
];

/// Log in to FlakeHub in order to allow authenticated fetching of flakes.
#[derive(Debug, Parser)]
pub(crate) struct LoginSubcommand {
    /// Skip following up a successful login with `fh status`.
    #[clap(long)]
    skip_status: bool,

    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    cache_addr: url::Url,

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
        let mut login_url = self.frontend_addr.clone();
        login_url.set_path("token/create");
        login_url.query_pairs_mut().append_pair(
            "description",
            &format!(
                "FlakeHub CLI on {}",
                gethostname::gethostname().to_string_lossy()
            ),
        );

        println!("Log in to FlakeHub: {}", login_url);
        println!("And then follow the prompts below:");
        println!();

        let token = crate::cli::cmd::init::prompt::Prompt::maybe_token("Paste your token here:");
        let (token, status) = match token {
            Some(token) => {
                // This serves as validating that provided token is actually a JWT, and is valid.
                let status = FlakeHubClient::auth_status(self.api_addr.as_ref(), &token).await?;
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
        // Note the root version uses extra-trusted-substituters, which
        // mean the cache is not enabled until a user (trusted or untrusted)
        // adds it to extra-substituters in their nix.conf.
        //
        // Note the root version sets netrc-file until the user authentication
        // patches (https://github.com/NixOS/nix/pull/9857) land.
        let root_nix_config_addition = format!(
            "\n\
            netrc-file = {netrc}\n\
            extra-trusted-substituters = {cache_addr}\n\
            extra-trusted-public-keys = {keys}\n\
            ",
            netrc = netrc_file_string,
            cache_addr = self.cache_addr,
            keys = CACHE_PUBLIC_KEYS.join(" "),
        );

        let user_nix_config_addition = format!(
            "\n\
            netrc-file = {netrc}\n\
            extra-substituters = {cache_addr}\n\
            extra-trusted-public-keys = {keys}\n\
            ",
            netrc = netrc_file_string,
            cache_addr = self.cache_addr,
            keys = CACHE_PUBLIC_KEYS.join(" "),
        );
        let netrc_contents = format!(
            "\
            machine {frontend_host} login flakehub password {token}\n\
            machine {backend_host} login flakehub password {token}\n\
            machine {cache_host} login flakehub password {token}\n\
            ",
            frontend_host = self
                .frontend_addr
                .host_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("frontend_addr had no host"))?,
            backend_host = self
                .api_addr
                .host_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("api_addr had no host"))?,
            cache_host = self
                .cache_addr
                .host_str()
                .ok_or_else(|| color_eyre::eyre::eyre!("cache_addr had no host"))?,
        );

        // NOTE: Keep an eye on any movement in the following issues / PRs. Them being resolved
        // means we may be able to ditch setting `netrc-file` in favor of `access-tokens`. (The
        // benefit is that `access-tokens` can be appended to, but `netrc-file` is a one-time thing
        // so if the user has their own `netrc-file`, Nix will decide which one wins.)
        // https://github.com/NixOS/nix/pull/9145 ("WIP: Support access-tokens for fetching tarballs from private sources")
        // https://github.com/NixOS/nix/issues/8635 ("Credentials provider support for builtins.fetch*")
        // https://github.com/NixOS/nix/issues/8439 ("--access-tokens option does nothing")
        tokio::fs::write(netrc_file_path, &netrc_contents).await?;
        tokio::fs::write(token_path, &token).await?;

        upsert_user_nix_config(
            &nix_config_path,
            &netrc_file_string,
            &netrc_contents,
            &user_nix_config_addition,
            &self.cache_addr,
        )
        .await?;

        let added_nix_config =
            nix_config_parser::NixConfig::parse_string(root_nix_config_addition.clone(), None)?;
        let root_nix_config_path = PathBuf::from("/etc/nix/nix.conf");
        let root_nix_config = nix_config_parser::NixConfig::parse_file(&root_nix_config_path)?;
        let mut root_meaningfully_different = false;

        for (merged_setting_name, merged_setting_value) in added_nix_config.settings() {
            if let Some(existing_setting_value) =
                root_nix_config.settings().get(merged_setting_name)
            {
                if merged_setting_value != existing_setting_value {
                    dbg!(&merged_setting_name);
                    root_meaningfully_different = true;
                }
            } else {
                root_meaningfully_different = true;
            }
        }

        if root_meaningfully_different {
            println!(
                "Please add the following configuration to {nix_conf_path}:\n\
                {root_nix_config_addition}",
                nix_conf_path = root_nix_config_path.display()
            );

            #[cfg(target_os = "macos")]
            {
                println!("Then restart the Nix daemon:\n");
                println!("sudo launchctl unload /Library/LaunchDaemons/org.nixos.nix-daemon.plist");
                println!("sudo launchctl load /Library/LaunchDaemons/org.nixos.nix-daemon.plist");
                println!();
            }
            #[cfg(target_os = "linux")]
            {
                println!("Then restart the Nix daemon:\n");
                println!("sudo systemctl restart nix-daemon.service");
                println!();
            }
        }

        if !self.skip_status {
            print!("{status}");
        }

        Ok(())
    }
}

// TODO(cole-h): make this atomic -- copy the nix_config_path to some temporary file, then operate
// on that, then move it back if all is good
async fn upsert_user_nix_config(
    nix_config_path: &Path,
    netrc_file_string: &str,
    netrc_contents: &str,
    user_nix_config_addition: &str,
    cache_addr: &url::Url,
) -> Result<(), color_eyre::eyre::Error> {
    let nix_config = nix_config_parser::NixConfig::parse_file(nix_config_path).unwrap_or_default();
    let mut merged_nix_config = nix_config_parser::NixConfig::new();
    merged_nix_config
        .settings_mut()
        .insert("netrc-file".to_string(), netrc_file_string.to_string());

    let setting = "extra-trusted-public-keys".to_string();
    if let Some(existing) = nix_config.settings().get(&setting) {
        let existing_value = existing.split(' ').collect::<Vec<_>>();
        if CACHE_PUBLIC_KEYS.iter().all(|k| existing_value.contains(k)) {
            // Do nothing, all our keys are already in place.
        } else {
            // We're missing some keys, let's insert them.
            let mut merged_value =
                Vec::with_capacity(existing_value.len() + CACHE_PUBLIC_KEYS.len());
            merged_value.extend(CACHE_PUBLIC_KEYS);
            merged_value.extend(existing_value);
            merged_value.dedup();
            let merged_value = merged_value.join(" ");
            let merged_value = merged_value.trim();

            merged_nix_config
                .settings_mut()
                .insert(setting, merged_value.to_owned());
        }
    } else {
        merged_nix_config
            .settings_mut()
            .insert(setting, CACHE_PUBLIC_KEYS.join(" "));
    }

    let setting = "extra-substituters".to_string();
    if let Some(existing) = nix_config.settings().get(&setting) {
        let existing_value = existing.split(' ').collect::<Vec<_>>();
        if existing_value.contains(&cache_addr.as_ref()) {
            // Do nothing, our substituter is already in place.
        } else {
            // We're missing our substituter, let's insert it.
            let mut merged_value = Vec::with_capacity(existing_value.len() + 1);
            merged_value.push(cache_addr.as_ref());
            merged_value.extend(existing_value);
            merged_value.dedup();
            let merged_value = merged_value.join(" ");
            let merged_value = merged_value.trim();

            merged_nix_config
                .settings_mut()
                .insert(setting, merged_value.to_owned());
        }
    } else {
        merged_nix_config
            .settings_mut()
            .insert(setting, cache_addr.to_string());
    }

    let mut were_meaningfully_different = false;
    let mut prompt = String::from("The following settings will be modified:\n");
    for (merged_setting_name, merged_setting_value) in merged_nix_config.settings() {
        let mut p = format!(
            "* `{name}` = `{new_val}`",
            name = merged_setting_name,
            new_val = merged_setting_value,
        );
        if let Some(existing_setting_value) = nix_config.settings().get(merged_setting_name) {
            if merged_setting_value != existing_setting_value {
                were_meaningfully_different = true;
                p += &format!(
                    " (previously: `{old_val}`)",
                    old_val = existing_setting_value
                );
            }
        } else {
            were_meaningfully_different = true;
        }
        prompt += &p;
        prompt.push('\n');
    }
    prompt.push_str("Confirm? (y/N)");

    let mut nix_conf_write_success = None;
    if were_meaningfully_different {
        let update_nix_conf = crate::cli::cmd::init::prompt::Prompt::bool(&prompt);
        if update_nix_conf {
            nix_conf_write_success = match tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&nix_config_path)
                .await
            {
                Ok(mut file) => {
                    let nix_config_contents = tokio::fs::read_to_string(&nix_config_path).await?;
                    let nix_config_contents =
                        merge_nix_configs(nix_config, nix_config_contents, merged_nix_config);
                    let write_status = file.write_all(nix_config_contents.as_bytes()).await;
                    Some(write_status.is_ok())
                }
                Err(_) => Some(false),
            };
        }

        let write_failed = nix_conf_write_success.is_some_and(|x| !x);
        if write_failed {
            print!("Writing to {} failed. ", nix_config_path.display());
        }

        if write_failed || !update_nix_conf {
            println!(
                "Please add the following contents to {config_path}:\n{addition}",
                config_path = nix_config_path.display(),
                addition = user_nix_config_addition,
            );
            println!(
                "Or add the following contents to your existing netrc file:\n\n\
                {netrc_contents}"
            );
        }
    }

    Ok(())
}

pub(crate) fn auth_token_path() -> Result<PathBuf, FhError> {
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
