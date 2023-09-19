use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

const COMMON_TOOLS: &[&str] = &["curl", "git", "jq", "wget"];

pub(crate) struct Tools;

impl Handler for Tools {
    fn handle(_: &Project, flake: &mut Flake) {
        for tool in Prompt::multi_select(
            "Add any of these standard utilities to your environment if you wish",
            COMMON_TOOLS,
        ) {
            let attr = tool.to_lowercase();
            flake.dev_shell_packages.push(attr);
        }
    }
}
