use crate::cli::cmd::init::prompt::Prompt;

use super::{prompt_for_language, Flake, Handler, Project};

const GO_VERSIONS: &[&str] = &["20", "19", "18", "17"];

pub(crate) struct Go;

impl Handler for Go {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("go.mod") && prompt_for_language("Go") {
            let go_version = Prompt::select("Select a version of Go", GO_VERSIONS);
            flake.dev_shell_packages.push(format!("go_1_{go_version}"));
        }
    }
}
