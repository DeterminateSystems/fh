use std::path::{Path, PathBuf};
use std::process::ExitCode;

use axum::body::Body;
use clap::Parser;
use color_eyre::eyre::eyre;
use color_eyre::eyre::WrapErr;
use http_body_util::BodyExt as _;
use hyper::client::conn::http1::SendRequest;
use hyper::{Method, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use crate::cli::cmd::FlakeHubClient;
use crate::cli::cmd::TokenStatus;
use crate::cli::error::FhError;
use crate::shared::{update_netrc_file, NetrcTokenAddRequest};
use crate::{DETERMINATE_NIXD_SOCKET_NAME, DETERMINATE_STATE_DIR};

use super::CommandExecute;

const CACHE_PUBLIC_KEYS: &[&str] = &[
    "cache.flakehub.com-3:hJuILl5sVK4iKm86JzgdXW12Y2Hwd5G07qKtHTOcDCM=",
    "cache.flakehub.com-4:Asi8qIv291s0aYLyH6IOnr5Kf6+OF14WVjkE6t3xMio=",
    "cache.flakehub.com-5:zB96CRlL7tiPtzA9/WKyPkp3A2vqxqgdgyTVNGShPDU=",
    "cache.flakehub.com-6:W4EGFwAGgBj3he7c5fNh9NkOXw0PUVaxygCVKeuvaqU=",
    "cache.flakehub.com-7:mvxJ2DZVHn/kRxlIaxYNMuDG1OvMckZu32um1TadOR8=",
    "cache.flakehub.com-8:moO+OVS0mnTjBTcOUh2kYLQEd59ExzyoW1QgQ8XAARQ=",
    "cache.flakehub.com-9:wChaSeTI6TeCuV/Sg2513ZIM9i0qJaYsF+lZCXg0J6o=",
    "cache.flakehub.com-10:2GqeNlIp6AKp4EF2MVbE1kBOp9iBSyo0UPR9KoR0o1Y=",
];

/// Log in to FlakeHub in order to allow authenticated fetching of flakes.
#[derive(Debug, Parser)]
pub(crate) struct LoginSubcommand {
    /// Read the FlakeHub token from a file.
    #[clap(long)]
    token_file: Option<std::path::PathBuf>,

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

pub async fn dnixd_uds() -> color_eyre::Result<SendRequest<axum::body::Body>> {
    let dnixd_state_dir = Path::new(&DETERMINATE_STATE_DIR);
    let dnixd_uds_socket_path: PathBuf = dnixd_state_dir.join(DETERMINATE_NIXD_SOCKET_NAME);

    let stream = TokioIo::new(UnixStream::connect(dnixd_uds_socket_path).await?);
    let (mut sender, conn): (SendRequest<Body>, _) =
        hyper::client::conn::http1::handshake(stream).await?;

    // NOTE(colemickens): for now we just drop the joinhandle and let it keep running
    let _join_handle = tokio::task::spawn(async move {
        if let Err(err) = conn.await {
            tracing::error!("Connection failed: {:?}", err);
        }
    });

    let request = http::Request::builder()
        .method(Method::GET)
        .uri("http://localhost/info")
        .body(axum::body::Body::empty())?;

    let response = sender.send_request(request).await?;

    if response.status() != StatusCode::OK {
        tracing::error!("failed to connect to determinate-nixd socket");
        return Err(eyre!("failed to connect to determinate-nixd socket"));
    }

    Ok(sender)
}

impl LoginSubcommand {
    async fn manual_login(&self) -> color_eyre::Result<()> {
        let dnixd_uds = match dnixd_uds().await {
            Ok(socket) => Some(socket),
            Err(err) => {
                tracing::debug!(
                    "failed to connect to determinate-nixd socket, will not attempt to use it: {:?}",
                    err
                );
                None
            }
        };

        let mut login_url = self.frontend_addr.clone();
        login_url.set_path("token/create");
        login_url.query_pairs_mut().append_pair(
            "description",
            &format!(
                "FlakeHub CLI on {}",
                gethostname::gethostname().to_string_lossy()
            ),
        );

        let mut token: Option<String> = if let Some(ref token_file) = self.token_file {
            Some(
                tokio::fs::read_to_string(token_file)
                    .await
                    .wrap_err("Reading the provided token file")?,
            )
        } else {
            println!("Log in to FlakeHub: {}", login_url);
            println!("And then follow the prompts below:");
            println!();
            crate::cli::cmd::init::prompt::Prompt::maybe_token("Paste your token here:")
        };

        let (token, status): (String, TokenStatus) = match token.take() {
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

        // NOTE: Keep an eye on any movement in the following issues / PRs. Them being resolved
        // means we may be able to ditch setting `netrc-file` in favor of `access-tokens`. (The
        // benefit is that `access-tokens` can be appended to, but `netrc-file` is a one-time thing
        // so if the user has their own `netrc-file`, Nix will decide which one wins.)
        // https://github.com/NixOS/nix/pull/9145 ("WIP: Support access-tokens for fetching tarballs from private sources")
        // https://github.com/NixOS/nix/issues/8635 ("Credentials provider support for builtins.fetch*")
        // https://github.com/NixOS/nix/issues/8439 ("--access-tokens option does nothing")

        if let Some(mut uds) = dnixd_uds {
            tracing::debug!("trying to update netrc via determinatenixd");

            let add_req = NetrcTokenAddRequest {
                token: token.clone(),
            };
            let add_req_json = serde_json::to_string(&add_req)?;
            let request = http::request::Builder::new()
                .uri("http://localhost/enroll-netrc-token")
                .method(Method::POST)
                .header("Content-Type", "application/json")
                .body(Body::from(add_req_json))?;
            let response = uds.send_request(request).await?;

            let body = response.into_body();
            let bytes = body.collect().await.unwrap_or_default().to_bytes();
            let text: String = String::from_utf8_lossy(&bytes).into();

            tracing::trace!("sent the add request: {:?}", text);
        } else {
            tracing::debug!(
                "failed to update netrc via determinatenixd, falling back to local-file approach"
            );

            // $XDG_CONFIG_HOME/fh/auth; basically ~/.config/fh/auth
            tokio::fs::write(user_auth_token_write_path()?, &token).await?;

            let xdg = xdg::BaseDirectories::new()?;

            let netrc_path = xdg.place_config_file("nix/netrc")?;

            // $XDG_CONFIG_HOME/nix/nix.conf; basically ~/.config/nix/nix.conf
            let nix_config_path = xdg.place_config_file("nix/nix.conf")?;

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
                netrc = netrc_path.display(),
                cache_addr = self.cache_addr,
                keys = CACHE_PUBLIC_KEYS.join(" "),
            );

            let user_nix_config_addition = format!(
                "\n\
                netrc-file = {netrc}\n\
                extra-substituters = {cache_addr}\n\
                extra-trusted-public-keys = {keys}\n\
                ",
                netrc = netrc_path.display(),
                cache_addr = self.cache_addr,
                keys = CACHE_PUBLIC_KEYS.join(" "),
            );
            let netrc_contents = crate::shared::netrc_contents(
                &self.frontend_addr,
                &self.api_addr,
                &self.cache_addr,
                &token,
            )?;

            update_netrc_file(&netrc_path, &netrc_contents).await?;

            // only update user_nix_config if we could not use determinatenixd
            upsert_user_nix_config(
                &nix_config_path,
                &netrc_path,
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
                    println!(
                        "sudo launchctl unload /Library/LaunchDaemons/org.nixos.nix-daemon.plist"
                    );
                    println!(
                        "sudo launchctl load /Library/LaunchDaemons/org.nixos.nix-daemon.plist"
                    );
                    println!();
                }
                #[cfg(target_os = "linux")]
                {
                    println!("Then restart the Nix daemon:\n");
                    println!("sudo systemctl restart nix-daemon.service");
                    println!();
                }
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
pub async fn upsert_user_nix_config(
    nix_config_path: &Path,
    netrc_path: &Path,
    netrc_contents: &str,
    user_nix_config_addition: &str,
    cache_addr: &url::Url,
) -> Result<(), color_eyre::eyre::Error> {
    let nix_config = nix_config_parser::NixConfig::parse_file(nix_config_path).unwrap_or_default();
    let mut merged_nix_config = nix_config_parser::NixConfig::new();
    merged_nix_config
        .settings_mut()
        .insert("netrc-file".to_string(), netrc_path.display().to_string());

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
            let nix_config_contents = tokio::fs::read_to_string(&nix_config_path).await?;
            nix_conf_write_success = match tokio::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&nix_config_path)
                .await
            {
                Ok(mut file) => {
                    let nix_config_contents = crate::shared::merge_nix_configs(
                        nix_config,
                        nix_config_contents,
                        merged_nix_config,
                    );
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

pub(crate) async fn user_auth_token_read_path() -> Result<PathBuf, FhError> {
    let write_path = user_auth_token_write_path();

    // If the user's personal token file exists, we use that first.
    if let Ok(ref path) = write_path {
        if tokio::fs::metadata(&path).await.is_ok() {
            return Ok(path.to_path_buf());
        }
    }

    // Either XDG failed to give us a path, or the user's token doesn't exist, so fall back
    // to the global token if that exists.
    let global_path =
        Path::new(crate::DETERMINATE_STATE_DIR).join(crate::DETERMINATE_NIXD_TOKEN_NAME);
    if tokio::fs::metadata(&global_path).await.is_ok() {
        return Ok(global_path);
    }

    // The global token didn't exist, and the user's token didn't exist,
    // let them now write to their own personal token file.
    write_path
}

pub(crate) fn user_auth_token_write_path() -> Result<PathBuf, FhError> {
    let xdg = xdg::BaseDirectories::new()?;
    // $XDG_CONFIG_HOME/flakehub/auth; basically ~/.config/flakehub/auth
    let token_path = xdg.place_config_file("flakehub/auth")?;

    Ok(token_path)
}
