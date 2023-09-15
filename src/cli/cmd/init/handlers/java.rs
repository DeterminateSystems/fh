use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{prompt_for_language, prompt_for_tool, Flake, Handler};

const JAVA_VERSIONS: &[&str] = &["19", "18", "17", "16", "15"];

pub(crate) struct Java;

impl Handler for Java {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["build.gradle", "pom.xml"]) && prompt_for_language("Java") {
            let java_version = Prompt::select("Which JDK version?", JAVA_VERSIONS);
            flake.dev_shell_packages.push(format!("jdk{java_version}"));

            if project.has_file("pom.xml") && prompt_for_tool("Maven") {
                flake.dev_shell_packages.push(String::from("maven"));
            }

            if project.has_file("build.gradle") && prompt_for_tool("Gradle") {
                flake.dev_shell_packages.push(String::from("gradle"));
            }
        }
    }
}
