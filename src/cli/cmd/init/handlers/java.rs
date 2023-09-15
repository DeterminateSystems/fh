use crate::cli::cmd::init::{project::Project, prompt::Prompt};

use super::{Flake, Handler};

const JAVA_VERSIONS: &[&str] = &["19", "18", "17", "16", "15"];

pub(crate) struct Java;

impl Handler for Java {
    fn handle(project: &Project, flake: &mut Flake) {
        if project.has_one_of(&["build.gradle", "pom.xml"]) && Prompt::bool("This seems to be a Java project. Would you like to initialize your flake with built-in Java dependencies?") {
            let java_version = Prompt::select("Which JDK version?", JAVA_VERSIONS);
            flake.dev_shell_packages.push(format!("jdk{java_version}"));

            if project.has_file("pom.xml") && Prompt::bool("This seems to be a Maven project. Would you like to add it to your environment") {
                flake.dev_shell_packages.push(String::from("maven"));
            }

            if project.has_file("build.gradle") && Prompt::bool("This seems to be a Gradle project. Would you like to add it to your environment") {
                flake.dev_shell_packages.push(String::from("gradle"));
            }
        }
    }
}
