use std::process::ExitCode;

use clap::Parser;
use color_eyre::eyre::WrapErr;

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
pub(crate) struct TokenStatus {
    gh_name: String,
    #[serde(deserialize_with = "i64_to_local_datetime")]
    expires_at: chrono::DateTime<chrono::Local>,
}

impl std::fmt::Display for TokenStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Logged in: true")?;
        writeln!(f, "GitHub user name: {}", self.gh_name)?;
        writeln!(f, "Token expires at: {}", self.expires_at)?;

        Ok(())
    }
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
        let status = get_status_from_auth_file(self.api_addr).await?;
        print!("{status}");

        Ok(ExitCode::SUCCESS)
    }
}

pub(crate) async fn get_status_from_auth_file(
    api_addr: url::Url,
) -> color_eyre::Result<TokenStatus> {
    let auth_token_path = crate::cli::cmd::login::auth_token_path()?;
    let token = tokio::fs::read_to_string(auth_token_path).await?;
    let token = token.trim();

    get_status_from_auth_token(api_addr, token).await
}

pub(crate) async fn get_status_from_auth_token(
    api_addr: url::Url,
    token: &str,
) -> color_eyre::Result<TokenStatus> {
    let mut cli_status = api_addr;
    cli_status.set_path("/cli/status");

    let res = reqwest::Client::new()
        .get(cli_status)
        .header("Authorization", &format!("Bearer {token}"))
        .send()
        .await
        .wrap_err("Failed to send request")?;
    let res = res
        .error_for_status()
        .wrap_err("Request was unsuccessful")?;
    let token_status: TokenStatus = res
        .json()
        .await
        .wrap_err("Failed to get TokenStatus from response (wasn't JSON, or was invalid JSON?)")?;

    Ok(token_status)
}
