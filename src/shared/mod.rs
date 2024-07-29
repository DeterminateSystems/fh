use std::path::Path;

use color_eyre::eyre::Context as _;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct DaemonInfoReponse {
    pub supported_features: Vec<String>,
}

#[derive(Deserialize, Serialize)]
pub struct NetrcTokenAddRequest {
    pub token: String,
    pub netrc_lines: String,
}

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
