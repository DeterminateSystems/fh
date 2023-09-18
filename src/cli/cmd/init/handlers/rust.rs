use crate::cli::cmd::init::prompt::Prompt;

use super::{prompt_for_language, Flake, Handler, Input, Project};

const CARGO_TOOLS: &[&str] = &[
    "audit", "bloat", "cross", "edit", "outdated", "udeps", "watch",
];

pub(crate) struct Rust;

impl Handler for Rust {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("Cargo.toml") && prompt_for_language("Rust") {
            flake.inputs.insert(
                String::from("rust-overlay"),
                Input {
                    reference: String::from("github:oxalica/rust-overlay"),
                    follows: Some(String::from("nixpkgs")),
                },
            );

            flake
                .overlay_refs
                .push(String::from("rust-overlay.overlays.default"));

            let rust_toolchain_func = String::from(if project.has_file("rust-toolchain") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain"
            } else if project.has_file("rust-toolchain.toml") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml"
            } else {
                // TODO: make this more granular
                "prev.rust-bin.stable.latest.default"
            });

            flake
                .overlay_attrs
                .insert(String::from("rustToolchain"), rust_toolchain_func);

            flake.dev_shell_packages.push(String::from("rustToolchain"));

            // Add cargo-* tools
            for tool in Prompt::multi_select(
                "You can add any of these Cargo tools to your environment if you wish",
                CARGO_TOOLS,
            ) {
                flake.dev_shell_packages.push(format!("cargo-{tool}"));
            }

            if Prompt::bool("Would you like to add Rust Analyzer to the environment?") {
                flake.dev_shell_packages.push(String::from("rust-analyzer"));
            }

            if Prompt::bool(
                "Would you like to enable Rust backtrace in the environment (RUST_BACKTRACE = \"1\")?",
            ) {
                flake
                    .env_vars
                    .insert(String::from("RUST_BACKTRACE"), String::from("1"));
            }
        }
    }
}
