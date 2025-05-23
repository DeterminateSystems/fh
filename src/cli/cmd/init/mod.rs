pub(crate) mod dev_shell;
pub(crate) mod handlers;
pub(crate) mod project;
pub(crate) mod prompt;
pub(crate) mod template;

use clap::Parser;
use color_eyre::eyre::Result;
use prompt::Prompt;
use std::{
    fs::write,
    io::IsTerminal,
    path::PathBuf,
    process::{exit, Command, ExitCode},
};
use url::Url;

use crate::{
    cli::{
        cmd::{init::handlers::Elm, list::FLAKEHUB_WEB_ROOT},
        error::FhError,
    },
    flakehub_url,
};

use super::FlakeHubClient;

use self::{
    dev_shell::DevShell,
    handlers::{
        Elixir, Flake, Go, Handler, Input, Java, JavaScript, Php, Python, Ruby, Rust, System,
        Tools, Zig,
    },
    project::Project,
    template::TemplateData,
};

use super::CommandExecute;

// Nixpkgs references
const NIXPKGS_LATEST: &str = "latest stable (currently 24.11)";
const NIXPKGS_24_11: &str = "24.11";
const NIXPKGS_UNSTABLE: &str = "unstable";
const NIXPKGS_SPECIFIC: &str = "select a specific release (not recommended in most cases)";

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

impl CommandExecute for InitSubcommand {
    async fn execute(self) -> Result<ExitCode> {
        if !std::io::stdout().is_terminal() {
            println!("fh init can only be used in a terminal; exiting");
            exit(1);
        } else {
            let mut flake = Flake::default();

            if self.output.exists() && !Prompt::bool("A flake.nix already exists in the current directory. Would you like to overwrite it?") {
                println!("Exiting. Let's a build a new flake soon, though :)");
                return Ok(ExitCode::SUCCESS);
            }

            println!("Let's build a Nix flake!");

            let project = Project::new(self.root);
            flake.description = Prompt::maybe_string("An optional description for your flake:");

            // Supported systems
            System::handle(&project, &mut flake);

            // We could conceivably create a version of `fh init` with Nixpkgs included only if certain other
            // choices are made. But for the time being so much relies on it that we don't have a great opt-out story,
            // so best to just include it in all flakes.
            let nixpkgs_version = match Prompt::select(
                "Which Nixpkgs version would you like to include?",
                &[
                    NIXPKGS_LATEST,
                    NIXPKGS_24_11,
                    NIXPKGS_UNSTABLE,
                    NIXPKGS_SPECIFIC,
                ],
            )
            .as_str()
            {
                // MAYBE: find an enum-based approach to this
                NIXPKGS_LATEST => flakehub_url!(FLAKEHUB_WEB_ROOT, "f", "NixOS", "nixpkgs", "*"),
                NIXPKGS_24_11 => {
                    flakehub_url!(FLAKEHUB_WEB_ROOT, "f", "NixOS", "nixpkgs", "0.2411.*")
                }
                NIXPKGS_UNSTABLE => {
                    flakehub_url!(FLAKEHUB_WEB_ROOT, "f", "NixOS", "nixpkgs", "0.1.*")
                }
                NIXPKGS_SPECIFIC => select_nixpkgs(self.api_addr.as_ref()).await?,
                // Just in case
                _ => return Err(FhError::Unreachable(String::from("nixpkgs selection")).into()),
            };

            flake.inputs.insert(
                String::from("nixpkgs"),
                Input::new(nixpkgs_version.as_ref(), None),
            );

            flake.inputs.insert(
                String::from("flake-schemas"),
                Input::new(
                    flakehub_url!(
                        FLAKEHUB_WEB_ROOT,
                        "f",
                        "DeterminateSystems",
                        "flake-schemas",
                        "*"
                    )
                    .as_str(),
                    None,
                ),
            );

            // Languages
            Elixir::handle(&project, &mut flake);
            Elm::handle(&project, &mut flake);
            Go::handle(&project, &mut flake);
            Java::handle(&project, &mut flake);
            JavaScript::handle(&project, &mut flake);
            Php::handle(&project, &mut flake);
            Python::handle(&project, &mut flake);
            Ruby::handle(&project, &mut flake);
            Rust::handle(&project, &mut flake);
            Zig::handle(&project, &mut flake);

            // Other tools
            Tools::handle(&project, &mut flake);

            // Nix formatter
            if Prompt::bool(
                "Would you like to add our recommended Nix formatter (nixpkgs-fmt) to your environment?",
            ) {
                flake.dev_shell_packages.push(String::from("nixpkgs-fmt"));
            }

            flake.doc_comments = Prompt::bool("Would you like to add doc comments to your flake that explain the meaning of different aspects of the flake?");

            if Prompt::bool("Would you like to add any environment variables?") {
                loop {
                    let name = Prompt::maybe_string("Variable name:");
                    if let Some(name) = name {
                        let value = Prompt::maybe_string("Variable value:");
                        if let Some(value) = value {
                            flake.env_vars.insert(name, value);
                            if !Prompt::bool("Enter another variable?") {
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

            if Prompt::bool("Would you like to add a shell hook that runs every time you enter your Nix development environment?") {
                loop {
                    let hook = Prompt::maybe_string(
                        "Enter the hook here:",
                    );

                    if let Some(hook) = hook {
                        flake.shell_hook = Some(hook);
                        break;
                    } else if !Prompt::bool("You didn't enter a hook. Would you like to try again?") {
                        break;
                    }
                }
            }

            // If the dev shell will be empty, prompt users to ensure that they still want a flake
            if flake.dev_shell_packages.is_empty() {
                if !Prompt::bool("The Nix development environment you've chosen doesn't have any packages in it. Would you still like to create a flake?") {
                    println!("See you next time!");
                }
                return Ok(ExitCode::SUCCESS);
            }

            flake.dev_shells.insert(
                String::from("default"),
                DevShell {
                    packages: flake.dev_shell_packages,
                    env_vars: flake.env_vars,
                },
            );

            let data = TemplateData {
                description: flake.description,
                inputs: flake.inputs,
                systems: flake.systems,
                dev_shells: flake.dev_shells,
                overlay_refs: flake.overlay_refs.clone(),
                overlay_attrs: flake.overlay_attrs.clone(),
                has_overlays: flake.overlay_refs.len() + flake.overlay_attrs.keys().len() > 0,
                fh_version: env!("CARGO_PKG_VERSION").to_string(),
                doc_comments: flake.doc_comments,
                shell_hook: flake.shell_hook,
            };

            let flake_string = data.render()?;

            write(self.output, flake_string)?;

            if project.has_directory(".git")
                && command_exists("git")
                && Prompt::bool("Would you like to add your new Nix file to Git?")
            {
                Command::new("git")
                    .args(["add", "--intent-to-add", "flake.nix"])
                    .output()?;
            }

            if !project.has_file(".envrc")
                && Prompt::bool("Would you like to add a .envrc file so that you can use direnv in this project?")
            {
                write(PathBuf::from(".envrc"), String::from("use flake"))?;

                if Prompt::bool("You'll need to run `direnv allow` to activate direnv in this project. Would you like to do that now?") {
                    if command_exists("direnv") {
                        Command::new("direnv").arg("allow").output()?;
                    } else {
                        println!("It looks like direnv isn't installed. Skipping `direnv allow`.");
                    }
                }
            }

            println!(
                "Your flake is ready to go! Run `nix flake show` to see which outputs it provides."
            );

            Ok(ExitCode::SUCCESS)
        }
    }
}

pub(super) fn command_exists(cmd: &str) -> bool {
    Command::new(cmd).output().is_ok()
}

async fn select_nixpkgs(api_addr: &str) -> Result<Url, FhError> {
    let releases = FlakeHubClient::releases(api_addr, "NixOS", "nixpkgs").await?;
    let releases: Vec<&str> = releases.iter().map(|r| r.version.as_str()).collect();
    let release = Prompt::select("Choose one of the following Nixpkgs releases:", &releases);
    let version = format!("{release}.tar.gz");
    Ok(flakehub_url!(
        FLAKEHUB_WEB_ROOT,
        "f",
        "NixOS",
        "nixpkgs",
        &version
    ))
}
