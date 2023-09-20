use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{prompt_for_language, prompt_for_tool, Flake, Handler};

const NODE_VERSIONS: &[&str] = &["18", "16", "14"];

pub(crate) struct JavaScript;

impl Handler for JavaScript {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("package.json") && prompt_for_language("JavaScript") {
            if project.has_file("bunfig.toml")
                && Prompt::bool(
                    "This seems to be a Bun project. Would you like to add it to your environment?",
                )
            {
                flake.dev_shell_packages.push(String::from("bun"));
            }

            if Prompt::bool("Is this a Node.js project?") {
                let version = Prompt::select("Select a version of Node.js", NODE_VERSIONS);
                flake.dev_shell_packages.push(format!("nodejs-{version}_x"));
            }

            if project.has_file("pnpm-lock.yaml") && prompt_for_tool("pnpm") {
                flake
                    .dev_shell_packages
                    .push(String::from("nodePackages.pnpm"));
            }

            if project.has_file("yarn.lock") && prompt_for_tool("Yarn") {
                flake
                    .dev_shell_packages
                    .push(String::from("nodePackages.yarn"));
            }
        }
    }
}
