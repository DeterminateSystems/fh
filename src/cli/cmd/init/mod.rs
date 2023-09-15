mod dev_shell;
mod project;
mod prompt;
mod template;

use clap::Parser;
use color_eyre::eyre::Result;
use project::Project;
use prompt::Prompt;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use url::Url;

use super::FlakeHubClient;

use self::{dev_shell::DevShell, prompt::MultiSelectOption, template::TemplateData};

use super::{CommandExecute, FhError};

const CARGO_TOOLS: &[&str] = &[
    "audit", "bloat", "cross", "edit", "outdated", "udeps", "watch",
];
const NODE_VERSIONS: &[&str] = &["18", "16", "14"];
const PYTHON_VERSIONS: &[&str] = &["3.11", "3.10", "3.09"];
const PYTHON_TOOLS: &[&str] = &["pip", "virtualenv", "pipenv"];
const RUBY_VERSIONS: &[&str] = &["3.2", "3.1"];
const GO_VERSIONS: &[&str] = &["20", "19", "18", "17"];
const COMMON_TOOLS: &[&str] = &["curl", "Git", "jq", "wget"];
const SYSTEMS: &[MultiSelectOption] = &[
    MultiSelectOption(
        "x86_64-linux",
        "Linux on a 64-bit x86 processor, like Intel or AMD",
    ),
    MultiSelectOption("aarch64-linux", "Linux on a 64-bit Arm processor"),
    MultiSelectOption("x86_64-darwin", "macOS on Intel CPUs"),
    MultiSelectOption(
        "aarch64-darwin",
        "macOS on Apple Silicon, like the M1 or M2 chips",
    ),
];
const PHP_VERSIONS: &[&str] = &["8.3", "8.2", "8.1", "8.0", "7.4", "7.3"];
const JAVA_VERSIONS: &[&str] = &["19", "18", "17", "16", "15"];

// Helper functions
fn version_as_attr(v: &str) -> String {
    v.replace('.', "")
}

/// Create a new flake.nix using an opinionated interactive initializer.
#[derive(Parser)]
pub(crate) struct InitSubcommand {
    #[clap(long, short, default_value = ".")]
    root: PathBuf,

    #[clap(long, short, default_value = "./flake.nix")]
    output: PathBuf,

    #[clap(from_global)]
    api_addr: url::Url,
}

#[async_trait::async_trait]
impl CommandExecute for InitSubcommand {
    async fn execute(self) -> Result<ExitCode> {
        let mut inputs: HashMap<String, String> = HashMap::new();
        let mut dev_shell_packages: Vec<String> = Vec::new();
        let mut dev_shells: HashMap<String, DevShell> = HashMap::new();
        let mut overlay_refs: Vec<String> = Vec::new();
        let mut overlay_attrs: HashMap<String, String> = HashMap::new();
        let mut env_vars: HashMap<String, String> = HashMap::new();

        if self.output.exists() && !Prompt::bool("A flake.nix already exists in the current directory. Would you like to overwrite it?")? {
            println!("Exiting. Let's a build a new flake soon, though :)");
            return Ok(ExitCode::SUCCESS);
        }

        println!("Let's build a Nix flake!");

        let project = Project::new(self.root);
        let description = Prompt::maybe_string("An optional description for your flake")?;

        fn get_systems() -> Result<Vec<String>, FhError> {
            let selected = Prompt::guided_multi_select(
                "Which systems would you like to support?",
                "system",
                SYSTEMS.to_vec(),
            )?;

            if selected.is_empty() {
                println!("❌ You need to select at least one system to support");
                #[allow(clippy::needless_return)]
                return get_systems();
            } else {
                Ok(selected)
            }
        }

        let systems = get_systems()?;

        // We could conceivably create a version of `fh init` Nixpkgs included only if certain other choices
        // are made. But for the time being so much relies on it that we don't have a great opt-out story,
        // so best to just include it in all flakes.
        let nixpkgs_version = match Prompt::select(
            "Which Nixpkgs version would you like to include?",
            &[
                "23.05",
                "latest",
                "unstable",
                "select a specific release (not recommended in most cases)",
            ],
        )?
        .as_str()
        {
            // MAYBE: find an enum-based approach to this
            "23.05" => String::from("0.2305.*"),
            "latest" => String::from("*"),
            "unstable" => String::from("0.1.*"),
            "select a specific release (not recommended in most cases)" => {
                select_nixpkgs(&self.api_addr).await?
            }
            _ => String::from("*"), // Unreachable
        };

        inputs.insert(
            String::from("nixpkgs"),
            format!("https://flakehub.com/f/NixOS/nixpkgs/{nixpkgs}.tar.gz"),
        );

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
        }

        // Java projects
        if project.maybe_java() && Prompt::bool("This seems to be a Java project. Would you like to initialize your flake with built-in Java dependencies?")? {
            let java_version = Prompt::select("Which JDK version?", JAVA_VERSIONS)?;
            dev_shell_packages.push(format!("jdk{java_version}"));

            if project.maybe_maven() && Prompt::bool("This seems to be a Maven project. Would you like to add it to your environment")? {
                dev_shell_packages.push(String::from("maven"));
            }

            if project.maybe_gradle() && Prompt::bool("This seems to be a Gradle project. Would you like to add it to your environment")? {
                dev_shell_packages.push(String::from("gradle"));
            }
        }

        // JavaScript projects
        if project.maybe_javascript() && Prompt::bool("This seems to be a JavaScript project. Would you like to initialize your flake with built-in JavaScript dependencies?")? {
            if project.maybe_bun() && Prompt::bool("This seems to be a Bun project. Would you like to add it to your environment?")? {
                dev_shell_packages.push(String::from("bun"));
            }

            if Prompt::bool("Is this a Node.js project?")? {
                let version =
                    Prompt::select("Select a version of Node.js", NODE_VERSIONS)?;
                dev_shell_packages.push(format!("nodejs-{version}_x"));
            }

            if project.maybe_pnpm() && Prompt::bool("This seems to be a pnpm project. Would you like to add it to your environment?")? {
                dev_shell_packages.push(String::from("nodePackages.pnpm"));
            }

            if project.maybe_yarn() && Prompt::bool("This seems to be a Yarn project. Would you like to add it to your environment?")? {
                dev_shell_packages.push(String::from("nodePackages.yarn"));
            }
        }

        // PHP projects
        if project.maybe_php() && Prompt::bool("This seems to be a PHP project. Would you like to initialize your flake with built-in PHO dependencies?")? {
            inputs.insert(String::from("loophp"), String::from("https://flakehub.com/f/loophp/nix-shell/0.1.*.tar.gz"));
            overlay_refs.push(String::from("loophp.overlays.default"));
            let php_version = Prompt::select("Select a version of PHP", PHP_VERSIONS)?;
            let php_version_attr = version_as_attr(&php_version);
            dev_shell_packages.push(format!("php{php_version_attr}"));
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
                "prev.rust-bin.stable.latest.default"
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

            if Prompt::bool("Would you like to add Rust Analyzer to the environment?")? {
                dev_shell_packages.push(String::from("rust-analyzer"));
            }

            if Prompt::bool("Would you like to enable Rust backtrace in the environment?")? {
                env_vars.insert(String::from("RUST_BACKTRACE"), String::from("1"));
            }
        }

        // Zig projects
        if project.maybe_zig() && Prompt::bool("This seems to be a Zig project. Would you like to initialize your flake with built-in Zig dependencies?")? {
            dev_shell_packages.push(String::from("zig"));
        }

        // Other tools
        for tool in Prompt::multi_select(
            "Add any of these standard utilities to your environment if you wish",
            COMMON_TOOLS,
        )? {
            let attr = tool.to_lowercase();
            dev_shell_packages.push(attr);
        }

        // Nix formatter
        if Prompt::bool(
            "Would you like to add our recommended Nix formatter (nixpkgs-fmt) to your environment?",
        )? {
            dev_shell_packages.push(String::from("nixpkgs-fmt"));
        }

        let doc_comments =
            Prompt::bool("Would you like to add helpful doc comments to your flake?")?;

        if Prompt::bool("Would you like to add any environment variables?")? {
            loop {
                let name = Prompt::maybe_string("Variable name")?;
                if let Some(name) = name {
                    let value = Prompt::maybe_string("Variable value")?;
                    if let Some(value) = value {
                        env_vars.insert(name, value);
                        if !Prompt::bool("Enter another variable?")? {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        // If the dev shell will be empty, prompt users to ensure that they still want a flake
        if dev_shell_packages.is_empty() {
            if !Prompt::bool("The Nix development environment you've chosen doesn't have any packages in it. Would you still like to create a flake?")? {
                println!("See you next time!");
            }
            return Ok(ExitCode::SUCCESS);
        }

        dev_shells.insert(
            String::from("default"),
            DevShell {
                packages: dev_shell_packages,
                env_vars,
            },
        );

        let data = TemplateData {
            description,
            inputs,
            systems,
            dev_shells,
            overlay_refs: overlay_refs.clone(),
            overlay_attrs: overlay_attrs.clone(),
            has_overlays: overlay_refs.len() + overlay_attrs.keys().len() > 0,
            doc_comments,
        };

        let flake_string = data.render()?;

        write_file(self.output, flake_string)?;

        if !project.uses_direnv() && Prompt::bool("Would you like to add a .envrc file for use with direnv?")? {
            write_file(PathBuf::from(".envrc"), String::from("use flake"))?;
        } else {
            println!(
                "Your flake is ready to go! Run `nix flake show` to see which outputs it provides."
            );
        }

        Ok(ExitCode::SUCCESS)
    }
}

fn write_file(path: PathBuf, content: String) -> Result<(), FhError> {
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

async fn select_nixpkgs(api_addr: &Url) -> Result<String, FhError> {
    let client = &FlakeHubClient::new(api_addr)?;
    let releases = client.releases("NixOS", "nixpkgs").await?;
    let releases: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
    let release = Prompt::select("Choose one of the following Nixpkgs releases", &releases)?;
    Ok(release)
}