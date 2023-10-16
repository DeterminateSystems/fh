use std::process::ExitCode;

use clap::Parser;

use super::CommandExecute;

// TODO: make status and login subcommands of a `auth` subcommand?
/// Check your FlakeHub token status.
#[derive(Debug, Parser)]
pub(crate) struct StatusSubcommand {
    #[clap(from_global)]
    api_addr: url::Url,

    #[clap(from_global)]
    frontend_addr: url::Url,
}

#[derive(Debug, serde::Deserialize)]
struct TokenStatus {
    gh_name: String,
    #[serde(deserialize_with = "i64_to_local_datetime")]
    expires_at: chrono::DateTime<chrono::Local>,
}

fn i64_to_local_datetime<'de, D>(
    deserializer: D,
) -> Result<chrono::DateTime<chrono::Local>, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let timestamp: i64 = serde::Deserialize::deserialize(deserializer)?;
    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp, 0)
        .ok_or_else(|| color_eyre::eyre::eyre!("Received an invalid timestamp (out-of-range)"))
        .map_err(serde::de::Error::custom)?;
    let expires_at = chrono::DateTime::<chrono::Local>::from(expires_at);

    Ok(expires_at)
}

#[async_trait::async_trait]
impl CommandExecute for StatusSubcommand {
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        get_status(self.api_addr).await?;

        Ok(ExitCode::SUCCESS)
    }
}

pub(crate) async fn get_status(api_addr: url::Url) -> color_eyre::Result<()> {
    let auth_token_path = crate::cli::cmd::login::auth_token_path()?;
    let token = tokio::fs::read_to_string(auth_token_path).await?;
    let token = token.trim();

    let mut cli_status = api_addr;
    cli_status.set_path("/cli/status");

    let token_status: TokenStatus = reqwest::Client::new()
        .get(cli_status)
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await?
        .json()
        .await?;

    println!("Logged in: {}", true);
    println!("GitHub user name: {}", token_status.gh_name);
    println!("Token expires at: {}", token_status.expires_at);

    Ok(())
}
