use crate::cli::cmd::init::prompt::Prompt;

use super::{Flake, Handler, Project};

const GO_VERSIONS: &[&str] = &["1.22", "1.23"];

pub(crate) struct Go;

impl Handler for Go {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("go.mod") && Prompt::for_language("Go") {
            let go_version = Prompt::select("Select a version of Go", GO_VERSIONS);
            let go_version_attr = format!("go_{}", go_version.replace(".", "_"));
            flake.dev_shell_packages.push(go_version_attr);
        }
    }
}
