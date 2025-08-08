use crate::{
    cli::cmd::{init::prompt::Prompt, list::FLAKEHUB_WEB_ROOT},
    flakehub_url,
};

use super::{Flake, Handler, Input, Project};

const CARGO_TOOLS: &[&str] = &["bloat", "edit", "outdated", "udeps", "watch"];

pub(crate) struct Rust;

impl Handler for Rust {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("Cargo.toml") && Prompt::for_language("Rust") {
            flake.inputs.insert(
                String::from("rust-overlay"),
                Input::new(
                    flakehub_url!(FLAKEHUB_WEB_ROOT, "f", "oxalica", "rust-overlay", "0.1.*")
                        .as_str(),
                    Some("nixpkgs"),
                ),
            );

            flake
                .overlay_refs
                .push(String::from("rust-overlay.overlays.default"));

            let rust_toolchain_func = String::from(if project.has_file("rust-toolchain") {
                "(final.rust-bin.fromRustupToolchainFile ./rust-toolchain)"
            } else if project.has_file("rust-toolchain.toml") {
                "(final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)"
            } else {
                // TODO: make this more granular
                "final.rust-bin.stable.latest.default"
            });

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

                let rust_toolchain_func_with_override = format!(
                    "{rust_toolchain_func}.override {{ extensions = [ \"rust-src\"]; }}"
                );

                flake.overlay_attrs.insert(
                    String::from("rustToolchain"),
                    rust_toolchain_func_with_override,
                );

                flake.env_vars.insert(
                    String::from("RUST_SRC_PATH"),
                    String::from("${pkgs.rustToolchain}/lib/rustlib/src/rust/library"),
                );
            } else {
                flake
                    .overlay_attrs
                    .insert(String::from("rustToolchain"), rust_toolchain_func);
            }

            if Prompt::bool(
                "Would you like to enable Rust backtrace in the environment (RUST_BACKTRACE = \"1\")?",
            ) {
                flake
                    .env_vars
                    .insert(String::from("RUST_BACKTRACE"), String::from("1"));
            }

            if project.has_file("Cross.toml") && Prompt::bool("This project appears to use cross-rs. Would you like to add the cargo-cross tool to your environment?") {
                flake.dev_shell_packages.push(String::from("cargo-cross"));
            }

            if project.has_file("deny.toml") && Prompt::bool("This project appears to use cargo-deny. Would you like to add it to your environment?") {
                flake.dev_shell_packages.push(String::from("cargo-deny"));
            }

            if project.has_file("audit.toml") && Prompt::bool("This project appears to use cargo-audit. Would you like to add it to your environment?") {
                flake.dev_shell_packages.push(String::from("cargo-audit"));
            }
        }
    }
}
