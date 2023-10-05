use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

pub(crate) struct Zig;

impl Handler for Zig {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file_or_directory("build.zig") && Prompt::for_language("Zig") {
            flake.dev_shell_packages.push(String::from("zig"));
        }
    }
}
