use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler, version_as_attr};

const RUBY_VERSIONS: &[&str] = &["3.2", "3.1"];

pub(crate) struct Ruby;

impl Handler for Ruby {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["Gemfile", "config.ru", "Rakefile"]) && Prompt::for_language("Ruby")
        {
            let ruby_version = Prompt::select("Select a version of Ruby", RUBY_VERSIONS);
            let ruby_version_attr = version_as_attr(&ruby_version, "_");
            flake
                .dev_shell_packages
                .push(format!("ruby_{ruby_version_attr}"));
        }
    }
}
