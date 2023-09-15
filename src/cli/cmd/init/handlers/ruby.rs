use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{version_as_attr, Flake, Handler};

const RUBY_VERSIONS: &[&str] = &["3.2", "3.1"];

pub(crate) struct Ruby;

impl Handler for Ruby {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["Gemfile", "config.ru", "Rakefile"]) && Prompt::bool("This seems to be a Ruby project. Would you like to initialize your flake with built-in Ruby dependencies?") {
            let ruby_version = Prompt::select("Select a version of Ruby", RUBY_VERSIONS);
            let ruby_version_attr = version_as_attr(&ruby_version);
            flake.dev_shell_packages.push(format!("ruby_{ruby_version_attr}"));
        }
    }
}
