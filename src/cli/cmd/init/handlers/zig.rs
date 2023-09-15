use crate::cli::cmd::init::project::Project;

use super::{prompt_for_language, Flake, Handler};

pub(crate) struct Zig;

impl Handler for Zig {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("build.zig") && prompt_for_language("Zig") {
            flake.dev_shell_packages.push(String::from("zig"));
        }
    }
}
