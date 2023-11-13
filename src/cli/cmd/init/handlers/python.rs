use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{version_as_attr_default, Flake, Handler};

const PYTHON_VERSIONS: &[&str] = &["3.11", "3.10", "3.09"];
const PYTHON_TOOLS: &[&str] = &["pip", "virtualenv", "pipenv"];

pub(crate) struct Python;

impl Handler for Python {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["setup.py", "requirements.txt"]) && Prompt::for_language("Python") {
            let python_version = Prompt::select("Select a version of Python", PYTHON_VERSIONS);
            let python_version_attr = version_as_attr_default(&python_version);
            flake
                .dev_shell_packages
                .push(format!("python{python_version_attr}"));
            let python_tools = Prompt::multi_select(
                "You can add any of these Python tools to your environment if you wish",
                PYTHON_TOOLS,
            );
            let tools_pkgs = format!(
                "(with python{python_version_attr}Packages; [ {} ])",
                python_tools.join(" ")
            );
            flake.dev_shell_packages.push(tools_pkgs);
        }
    }
}
