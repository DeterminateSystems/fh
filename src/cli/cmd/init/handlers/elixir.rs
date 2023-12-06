use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

const ELIXIR_LATEST: &str = "elixir_1_15";
const ERLANG_LATEST: &str = "erlang_26";

pub(crate) struct Elixir;

impl Handler for Elixir {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_file("mix.exs") && Prompt::for_language("Elixir") {
            flake.dev_shell_packages.push(String::from(ELIXIR_LATEST));
            flake.dev_shell_packages.push(String::from("elixir_ls"));
            flake.dev_shell_packages.push(String::from(ERLANG_LATEST));

            if Prompt::bool("Would you like to add Livebook to the environment?") {
                flake.dev_shell_packages.push(String::from("livebook"));
            }
        }
    }
}
