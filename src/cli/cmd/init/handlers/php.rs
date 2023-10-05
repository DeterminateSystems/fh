use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{version_as_attr, Flake, Handler};

const PHP_VERSIONS: &[&str] = &["8.3", "8.2", "8.1", "8.0", "7.4", "7.3"];

pub(crate) struct Php;

impl Handler for Php {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["composer.json", "php.ini"]) && Prompt::for_language("PHP") {
            flake.inputs.insert(
                String::from("loophp"),
                super::Input {
                    reference: String::from("https://flakehub.com/f/loophp/nix-shell/0.1.*.tar.gz"),
                    follows: Some(String::from("nixpkgs")),
                },
            );
            flake
                .overlay_refs
                .push(String::from("loophp.overlays.default"));
            let php_version = Prompt::select("Select a version of PHP", PHP_VERSIONS);
            let php_version_attr = version_as_attr(&php_version);
            flake
                .dev_shell_packages
                .push(format!("php{php_version_attr}"));
        }
    }
}
