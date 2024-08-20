use std::{collections::HashMap, process::ExitCode};

use clap::Parser;
use serde::{Deserialize, Serialize, Serializer};

use super::{parse_release_ref, print_json, CommandExecute, FlakeHubClient};

/// Display all output paths that are derivations in the specified flake release.
#[derive(Debug, Parser)]
pub(crate) struct PathsSubcommand {
    /// TODO
    release_ref: String,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for PathsSubcommand {
    #[tracing::instrument(skip_all)]
    async fn execute(self) -> color_eyre::Result<ExitCode> {
        let release_ref = parse_release_ref(&self.release_ref)?;

        let paths = FlakeHubClient::paths(self.api_addr.as_ref(), &release_ref).await?;

        tracing::debug!(
            r#ref = release_ref.to_string(),
            "Successfully fetched output paths for release"
        );

        print_json(paths)?;

        Ok(ExitCode::SUCCESS)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum PathNode {
    Path(String),
    PathMap(HashMap<String, PathNode>),
}

// The custom serializer converts empty maps into nulls
impl Serialize for PathNode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            PathNode::Path(s) => s.serialize(serializer),
            PathNode::PathMap(map) => {
                if map.is_empty() {
                    serializer.serialize_none()
                } else {
                    map.serialize(serializer)
                }
            }
        }
    }
}
