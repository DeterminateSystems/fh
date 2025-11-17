use std::path::{Path, PathBuf};

use color_eyre::eyre::Context as _;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use url::Url;

pub async fn update_netrc_file(
    netrc_file_path: &Path,
    netrc_contents: &str,
) -> color_eyre::Result<()> {
    tokio::fs::write(netrc_file_path, &netrc_contents)
        .await
        .wrap_err("failed to update netrc file contents")
}

pub fn netrc_contents(
    frontend_addr: &url::Url,
    backend_addr: &url::Url,
    cache_addr: &url::Url,
    token: &str,
) -> color_eyre::Result<String> {
    let contents = format!(
        "\
        machine {frontend_host} login flakehub password {token}\n\
        machine {backend_host} login flakehub password {token}\n\
        machine {cache_host} login flakehub password {token}\n\
        ",
        frontend_host = frontend_addr
            .host_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("frontend_addr had no host"))?,
        backend_host = backend_addr
            .host_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("api_addr had no host"))?,
        cache_host = cache_addr
            .host_str()
            .ok_or_else(|| color_eyre::eyre::eyre!("cache_addr had no host"))?,
        token = token,
    );
    Ok(contents)
}

// NOTE(cole-h): Adapted from
// https://github.com/DeterminateSystems/nix-installer/blob/0b0172547c4666f6b1eacb6561a59d6b612505a3/src/action/base/create_or_merge_nix_config.rs#L284
const NIX_CONF_COMMENT_CHAR: char = '#';
pub fn merge_nix_configs(
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

            if let Some(merged_value) = merged_nix_config.settings_mut().shift_remove(name) {
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
            existing_nix_config.settings_mut().shift_remove(&to_remove);
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

pub async fn create_temp_netrc(
    dir: &Path,
    host_url: &Url,
    token: &str,
) -> color_eyre::Result<PathBuf> {
    let host = host_url.host_str().expect("Malformed URL: missing host");

    let path = dir.join("netrc");

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .await?;

    let contents = format!("machine {host} login flakehub password {token}\n");

    file.write_all(contents.as_bytes()).await?;

    Ok(path)
}
