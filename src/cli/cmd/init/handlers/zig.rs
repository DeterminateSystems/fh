use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

pub(crate) struct Zig;

impl Handler for Zig {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("build.zig") && Prompt::bool("This seems to be a Zig project. Would you like to initialize your flake with built-in Zig dependencies?") {
            flake.dev_shell_packages.push(String::from("zig"));
        }
    }
}
