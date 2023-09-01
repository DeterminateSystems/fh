use clap::Parser;
use color_eyre::eyre::Result;
use inquire::{Confirm, MultiSelect, Select, Text};
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

const GO_VERSIONS: &[&str] = &["20", "19", "18", "16"];

const SYSTEMS: &[&str] = &[
    "x86_64-linux",
    "aarch64-linux",
    "x86_64-darwin",
    "aarch64-darwin",
];

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
        let mut overlays: Vec<Overlay> = Vec::new();

        if self.output.exists() && !Prompt::bool("A flake.nix already exists in the current directory. Would you like to overwrite it?")? {
            println!("Exiting. Let's a build a new flake soon, though :)");
            return Ok(ExitCode::SUCCESS);
        }

        println!("Let's build a Nix flake!");

        let project = Project::new(self.root);
        let description = Prompt::maybe_string("A description for your flake")?;

        let systems =
            MultiSelect::new("Which systems would you like to support?", SYSTEMS.to_vec())
                .prompt()?
                .iter()
                .map(|s| String::from(*s))
                .collect();

        let nixpkgs = Confirm::new("Do you want to include a Nixpkgs input?")
            .with_default(false)
            .prompt()?;

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

        // Rust projects
        if project.is_rust_project() && Prompt::bool("This seems to be a Rust project. Would you like to initialize your flake with built-in Rust dependencies?")? {
            // Add Rust overlay
            inputs.insert(
                String::from("rust-overlay"),
                String::from("github:oxalica/rust-overlay"),
            );
            overlays.push(Overlay::Raw(String::from("rust-overlay.overlays.default")));

            // Add an overlay for inferring a toolchain
            let rust_toolchain_func = String::from(if project.file_exists("rust-toolchain") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain"
            } else if project.file_exists("rust-toolchain.toml") {
                "prev.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml"
            } else {
                // TODO: make this more granular
                "prev.rust-bin.latest.default"
            });
            overlays.push(Overlay::KV(
                String::from("rustToolchain"),
                rust_toolchain_func,
            ));
            dev_shell_packages.push(String::from("rustToolchain"));

            // Add cargo-* tools
            for tool in MultiSelect::new(
                "You can add any of these Cargo tools to your environment if you wish",
                CARGO_TOOLS.to_vec(),
            )
            .prompt()?
            .iter()
            {
                dev_shell_packages.push(format!("cargo-{tool}"));
            }

            if Prompt::bool("Do you want to add Rust Analyzer to the environment?")? {
                dev_shell_packages.push(String::from("rust-analyzer"));
            }
        }

        // Go projects
        if project.is_go_project() && Prompt::bool("This seems to be a Go project. Would you like to initialize your flake with built-in Go dependencies?")?{
            if inputs.get("nixpkgs").is_none() {
                if Prompt::bool(
                    "You'll need a Nixpkgs input for Go projects. Would you like to add one?",
                )? {
                    inputs.insert(
                        String::from("nixpkgs"),
                        String::from("https://flakehub.com/f/NixOS/nixpkgs/*.tar.gz"), // TODO: make this more granular
                    );
                }
            }

            let go_version =
            Select::new("Select a version of Go", GO_VERSIONS.to_vec()).prompt()?;
            dev_shell_packages.push(format!("go_1_{go_version}"));
        }

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
            overlays,
        };

        flake.validate()?;

        let mut handlebars = handlebars::Handlebars::new();
        handlebars
            .register_template_string("flake", include_str!("../../../assets/flake.hbs"))
            .map_err(Box::new)?;
        let data: Value = serde_json::to_value(flake)?;
        let output = handlebars.render("flake", &data)?;

        let mut flake_dot_nix = File::create(self.output)?;
        flake_dot_nix.write_all(output.as_bytes())?;

        if !project.file_exists(".envrc") && Prompt::bool("Are you a direnv user? Select yes if you'd like to add a .envrc file to this project")?{
            let mut envrc = File::create(".envrc")?;
            envrc.write_all(b"use flake")?;
        }

        println!(
            "Your flake is ready to go! Run `nix flake show` to see which outputs it provides."
        );

        Ok(ExitCode::SUCCESS)
    }
}

struct Prompt;

impl Prompt {
    fn bool(msg: &str) -> Result<bool, FhError> {
        Confirm::new(msg).prompt().map_err(FhError::Interactive)
    }

    fn maybe_string(msg: &str) -> Result<Option<String>, FhError> {
        match Text::new(msg).prompt() {
            Ok(text) => Ok(if text.is_empty() { None } else { Some(text) }),
            Err(e) => Err(FhError::Interactive(e)),
        }
    }
}

#[derive(Debug, Serialize)]
struct Flake {
    description: Option<String>,
    inputs: HashMap<String, String>,
    systems: Vec<String>,
    dev_shells: HashMap<String, DevShell>,
    overlays: Vec<Overlay>,
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

#[derive(Debug, Serialize)]
enum Overlay {
    Raw(String),
    KV(String, String),
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn is_go_project(&self) -> bool {
        self.has_file("go.mod")
    }

    fn is_rust_project(&self) -> bool {
        self.has_file("Cargo.toml")
    }

    fn has_file(&self, file: &str) -> bool {
        self.root.join(file).exists()
    }

    #[allow(dead_code)]
    fn has_one_of(&self, files: &[&str]) -> bool {
        files.iter().any(|f| self.has_file(f))
    }

    fn file_exists(&self, file: &str) -> bool {
        self.root.join(file).exists()
    }
}
