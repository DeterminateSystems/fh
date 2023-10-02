use crate::cli::cmd::init::project::Project;

use super::{prompt_for_language, Handler};

pub(crate) struct Elm;

impl Handler for Elm {
    fn handle(project: &Project, flake: &mut super::Flake) {
        if project.has_file("elm.json") && prompt_for_language("Elm") {
            flake
                .dev_shell_packages
                .push(String::from("elmPackages.elm"));
        }
    }
}
