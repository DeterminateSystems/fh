mod project;
mod prompt;

use clap::Parser;
use color_eyre::eyre::Result;
use handlebars::Handlebars;
use project::Project;
use prompt::Prompt;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use super::{CommandExecute, FhError};

const CARGO_TOOLS: &[&str] = &[
    "audit", "bloat", "cross", "edit", "outdated", "udeps", "watch",
];
const NODE_VERSIONS: &[&str] = &["18", "16", "14"];
const JS_PACKAGING_TOOLS: &[&str] = &["pnpm", "yarn"];
const PYTHON_VERSIONS: &[&str] = &["3.11", "3.10", "3.09"];
const PYTHON_TOOLS: &[&str] = &["pip", "virtualenv", "pipenv"];
const RUBY_VERSIONS: &[&str] = &["3.2", "3.1"];
const GO_VERSIONS: &[&str] = &["20", "19", "18", "17"];
const COMMON_TOOLS: &[&str] = &["curl", "Git", "jq", "wget"];
const SYSTEMS: &[&str] = &[
    "x86_64-linux",
    "aarch64-linux",
    "x86_64-darwin",
    "aarch64-darwin",
];

// Helper functions
fn version_as_attr(v: &str) -> String {
    v.replace('.', "")
}

/// Create a new flake.nix using an interactive initializer.
#[derive(Parser)]
pub(crate) struct InitSubcommand {
    #[clap(long, short, default_value = ".")]
    root: PathBuf,

    #[clap(long, short, default_value = "./flake.nix")]
    output: PathBuf,
}

#[async_trait::async_trait]
impl CommandExecute for InitSubcommand {
    async fn execute(self) -> Result<ExitCode> {
        let mut inputs: HashMap<String, String> = HashMap::new();
        let mut dev_shell_packages: Vec<String> = Vec::new();
        let mut dev_shells: HashMap<String, DevShell> = HashMap::new();
        let mut overlay_refs: Vec<String> = Vec::new();
        let mut overlay_attrs: HashMap<String, String> = HashMap::new();
        let mut packages: HashMap<String, String> = HashMap::new();

        if self.output.exists() && !Prompt::bool("A flake.nix already exists in the current directory. Would you like to overwrite it?")? {
            println!("Exiting. Let's a build a new flake soon, though :)");
            return Ok(ExitCode::SUCCESS);
        }

        println!("Let's build a Nix flake!");

        let project = Project::new(self.root);
        let description = Prompt::maybe_string("A description for your flake")?;

        let systems = Prompt::multi_select("Which systems would you like to support?", SYSTEMS)?;

        let nixpkgs = Prompt::bool("Do you want to include a Nixpkgs input?")?;

        if nixpkgs {
            inputs.insert(
                String::from("nixpkgs"),
                String::from("https://flakehub.com/f/NixOS/nixpkgs/*.tar.gz"), // TODO: make this more granular
            );
        }

        if Prompt::bool(
            "Do you want to add the commonly used Nix formatter (nixpkgs-fmt) to your environment?",
        )? {
            dev_shell_packages.push(String::from("nixpkgs-fmt"));
        }

        // Go projects
        if project.maybe_golang() && Prompt::bool("This seems to be a Go project. Would you like to initialize your flake with built-in Go dependencies?")? {
            if inputs.get("nixpkgs").is_none() && Prompt::bool(
                "You'll need a Nixpkgs input for Go projects. Would you like to add one?",
            )? {
                inputs.insert(
                    String::from("nixpkgs"),
                    String::from("https://flakehub.com/f/NixOS/nixpkgs/*.tar.gz"), // TODO: make this more granular
                );
            }
            let go_version = Prompt::select("Select a version of Go", GO_VERSIONS)?;
            dev_shell_packages.push(format!("go_1_{go_version}"));

            if Prompt::bool("Would you like to provide a Go package output?")? {
                let name = Prompt::string("Enter the name of the output", "default")?;
                packages.insert(name, String::from("pkgs.buildGoPackage {}"));
            }
        }

        // JavaScript projects
        if project.maybe_javascript() && Prompt::bool("This seems to be a JavaScript project. Would you like to initialize your flake with built-in JavaScript dependencies?")? {
            let version =
                Prompt::select("Select a version of Node.js", NODE_VERSIONS)?;
            dev_shell_packages.push(format!("nodejs-{version}_x"));

            for tool in Prompt::multi_select(
                "Which packaging tools would you like to install (npm is already included)",
                JS_PACKAGING_TOOLS,
            )? {
                dev_shell_packages.push(format!("nodePackages.{tool}"));
            }
        }

        // Python projects
        if project.maybe_python() && Prompt::bool("This seems to be a Python project. Would you like to initialize your flake with built-in Python dependencies?")? {
            let python_version = Prompt::select("Select a version of Python", PYTHON_VERSIONS)?;
            let python_version_attr = version_as_attr(&python_version);
            dev_shell_packages.push(format!("python{python_version_attr}"));
            let python_tools = Prompt::multi_select(
                "You can add any of these Python tools to your environment if you wish",
                PYTHON_TOOLS,
            )?;
            let tools_pkgs = format!("(with python{python_version_attr}Packages; [ {} ])", python_tools.join(" "));
            dev_shell_packages.push(tools_pkgs);
        }

        // Ruby projects
        if project.maybe_ruby() && Prompt::bool("This seems to be a Ruby project. Would you like to initialize your flake with built-in Ruby dependencies?")? {
            let ruby_version = Prompt::select("Select a version of Ruby", RUBY_VERSIONS)?;
            let ruby_version_attr = version_as_attr(&ruby_version);
            dev_shell_packages.push(format!("ruby_{ruby_version_attr}"));
        }

        // Rust projects
        if project.maybe_rust() && Prompt::bool("This seems to be a Rust project. Would you like to initialize your flake with built-in Rust dependencies?")? {
            // Add Rust overlay
            inputs.insert(
                String::from("rust-overlay"),
                String::from("github:oxalica/rust-overlay"),
            );
            overlay_refs.push(String::from("rust-overlay.overlays.default"));

            // Add an overlay for inferring a toolchain
            let rust_toolchain_func = String::from(if project.has_file("rust-toolchain") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain"
            } else if project.has_file("rust-toolchain.toml") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml"
            } else {
                // TODO: make this more granular
                "prev.rust-bin.latest.default"
            });

            overlay_attrs.insert(String::from("rustToolchain"), rust_toolchain_func);
            dev_shell_packages.push(String::from("rustToolchain"));

            // Add cargo-* tools
            for tool in Prompt::multi_select(
                "You can add any of these Cargo tools to your environment if you wish",
                CARGO_TOOLS,
            )? {
                dev_shell_packages.push(format!("cargo-{tool}"));
            }

            if Prompt::bool("Do you want to add Rust Analyzer to the environment?")? {
                dev_shell_packages.push(String::from("rust-analyzer"));
            }
        }

        // Zig projects
        if project.maybe_zig() && Prompt::bool("This seems to be a Zig project. Would you like to initialize your flake with built-in Zig dependencies?")? {
            dev_shell_packages.push(String::from("zig"));
        }

        // Other tools
        for tool in Prompt::multi_select(
            "You can add any of these standard utilities to your environment if you wish",
            COMMON_TOOLS,
        )? {
            let attr = tool.to_lowercase();
            dev_shell_packages.push(attr);
        }

        // Add the default devShell
        dev_shells.insert(
            String::from("default"),
            DevShell {
                packages: dev_shell_packages,
            },
        );

        let flake = Flake {
            description,
            inputs,
            systems,
            dev_shells,
            overlay_refs: overlay_refs.clone(),
            overlay_attrs: overlay_attrs.clone(),
            has_overlays: overlay_refs.len() + overlay_attrs.keys().len() > 0,
        };

        flake.validate()?;

        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("flake", include_str!("../../../../assets/flake.hbs"))
            .map_err(Box::new)?;
        let data: Value = serde_json::to_value(flake)?;
        let output = handlebars.render("flake", &data)?;

        let mut flake_dot_nix = File::create(self.output)?;
        flake_dot_nix.write_all(output.as_bytes())?;

        if !project.has_file(".envrc") && Prompt::bool("Are you a direnv user? Select yes if you'd like to add a .envrc file to this project")?{
            let mut envrc = File::create(".envrc")?;
            envrc.write_all(b"use flake")?;
        } else {
            println!(
                "Your flake is ready to go! Run `nix flake show` to see which outputs it provides."
            );
        }

        Ok(ExitCode::SUCCESS)
    }
}

#[derive(Debug, Serialize)]
struct Flake {
    description: Option<String>,
    inputs: HashMap<String, String>,
    systems: Vec<String>,
    dev_shells: HashMap<String, DevShell>,
    overlay_refs: Vec<String>,
    overlay_attrs: HashMap<String, String>,
    has_overlays: bool,
}

impl Flake {
    fn validate(&self) -> Result<(), FhError> {
        if self.inputs.is_empty() {
            return Err(FhError::NoInputs);
        }

        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct DevShell {
    packages: Vec<String>,
}
