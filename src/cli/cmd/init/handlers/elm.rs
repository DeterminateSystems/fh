use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::Handler;

pub(crate) struct Elm;

impl Handler for Elm {
    fn handle(project: &Project, flake: &mut super::Flake) {
        if project.has_file_or_directory("elm.json") && Prompt::for_language("Elm") {
            flake
                .dev_shell_packages
                .push(String::from("elmPackages.elm"));
        }
    }
}
