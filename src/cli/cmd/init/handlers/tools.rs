use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

const COMMON_TOOLS: &[&str] = &["curl", "git", "jq", "wget"];

pub(crate) struct Tools;

impl Handler for Tools {
    fn handle(project: &Project, flake: &mut Flake) {
        for tool in Prompt::multi_select(
            "Add any of these standard utilities to your environment if you wish",
            COMMON_TOOLS,
        ) {
            let attr = tool.to_lowercase();
            flake.dev_shell_packages.push(attr);
        }

        if project.has_one_of(&["WORKSPACE", ".bazelrc", ".bazelversion", "BUILD.bazel"])
            && Prompt::for_tool("Bazel")
        {
            flake.dev_shell_packages.push(String::from("bazel"));
        }

        if project.has_file("netlify.toml") && Prompt::bool("This project appears to deploy to Netlify. Would you like to add the Netlify CLI to your environment?") {
            flake.dev_shell_packages.push(String::from("netlify-cli"));
        }

        if project.has_file("fly.toml") && Prompt::bool("This project appears to deploy to Fly. Would you like to add the Fly CLI to your environment?") {
            flake.dev_shell_packages.push(String::from("flyctl"));
        }

        if project.has_file("sqlx-data.json") && Prompt::bool("This project appears to use sqlx for Rust. Would you like to add the sqlx CLI to your environment?") {
            flake.dev_shell_packages.push(String::from("sqlx-cli"));
        }

        if project.has_file("Tiltfile") && Prompt::for_tool("Tilt") {
            flake.dev_shell_packages.push(String::from("tilt"));
        }

        if project.has_one_of(&["buf.yaml", "buf.lock", "buf.gen.yaml", "buf.work.yaml"])
            && Prompt::for_tool("Buf")
        {
            flake.dev_shell_packages.push(String::from("buf"));
        }

        if project.has_file("Pulumi.yaml") && Prompt::for_tool("Pulumi") {
            flake.dev_shell_packages.push(String::from("pulumi"));
        }

        if project.has_file(".shellcheckrc") && Prompt::for_tool("ShellCheck") {
            flake.dev_shell_packages.push(String::from("shellcheck"));
        }
    }
}
