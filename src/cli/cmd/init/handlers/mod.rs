use std::collections::HashMap;

mod go;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust;
mod system;
mod tools;
mod zig;

pub(crate) use go::Go;
pub(crate) use java::Java;
pub(crate) use javascript::JavaScript;
pub(crate) use php::Php;
pub(crate) use python::Python;
pub(crate) use ruby::Ruby;
pub(crate) use rust::Rust;
use serde_derive::Serialize;
pub(crate) use system::System;
pub(crate) use tools::Tools;
pub(crate) use zig::Zig;

use super::{dev_shell::DevShell, project::Project, prompt::Prompt};

#[derive(Debug, Serialize)]
pub(crate) struct Input {
    pub(crate) reference: String,
    pub(crate) follows: Option<String>,
}

#[derive(Default)]
pub(crate) struct Flake {
    pub(crate) description: Option<String>,
    pub(crate) systems: Vec<String>,
    pub(crate) dev_shells: HashMap<String, DevShell>,
    pub(crate) inputs: HashMap<String, Input>,
    pub(crate) overlay_refs: Vec<String>,
    pub(crate) overlay_attrs: HashMap<String, String>,
    pub(crate) dev_shell_packages: Vec<String>,
    pub(crate) env_vars: HashMap<String, String>,
}

pub(crate) trait Handler {
    fn handle(project: &Project, flake: &mut Flake);
}

// Helper functions
fn version_as_attr(v: &str) -> String {
    v.replace('.', "")
}

fn prompt_for_language(lang: &str) -> bool {
    Prompt::bool(&format!("This seems to be a {lang} project. Would you like to initialize your flake with built-in {lang} dependencies?"))
}

fn prompt_for_tool(tool: &str) -> bool {
    Prompt::bool(&format!(
        "This seems to be a {tool} project. Would you like to add it to your environment?"
    ))
}
