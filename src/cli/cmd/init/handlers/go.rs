use crate::cli::cmd::init::prompt::Prompt;

use super::{Flake, Handler, Project};

const GO_VERSIONS: &[&str] = &["20", "19", "18", "17"];

pub(crate) struct Go;

impl Handler for Go {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("go.mod") && Prompt::bool("This seems to be a Go project. Would you like to initialize your flake with built-in Go dependencies?") {
            let go_version = Prompt::select("Select a version of Go", GO_VERSIONS);
            flake.dev_shell_packages.push(format!("go_1_{go_version}"));
        }
    }
}
