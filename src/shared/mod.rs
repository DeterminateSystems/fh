use std::path::Path;

use color_eyre::eyre::Context as _;
use serde::{Deserialize, Serialize};

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
