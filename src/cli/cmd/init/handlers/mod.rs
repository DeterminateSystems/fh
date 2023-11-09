use serde::Serialize;
use std::collections::HashMap;

pub(crate) mod elm;
pub(crate) mod go;
pub(crate) mod java;
pub(crate) mod javascript;
pub(crate) mod php;
pub(crate) mod python;
pub(crate) mod ruby;
pub(crate) mod rust;
pub(crate) mod system;
pub(crate) mod tools;
pub(crate) mod zig;

pub(crate) use elm::Elm;
pub(crate) use go::Go;
pub(crate) use java::Java;
pub(crate) use javascript::JavaScript;
pub(crate) use php::Php;
pub(crate) use python::Python;
pub(crate) use ruby::Ruby;
pub(crate) use rust::Rust;
pub(crate) use system::System;
pub(crate) use tools::Tools;
pub(crate) use zig::Zig;

use super::{dev_shell::DevShell, project::Project};

#[derive(Debug, Serialize)]
pub(crate) struct Input {
    pub(crate) reference: String,
    pub(crate) follows: Option<String>,
}

impl Input {
    pub(crate) fn new(reference: &str, follows: Option<&str>) -> Self {
        Self {
            reference: String::from(reference),
            follows: follows.map(|f| f.to_owned()),
        }
    }
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
    pub(crate) shell_hook: Option<String>,
    pub(crate) doc_comments: bool,
}

pub(crate) trait Handler {
    fn handle(project: &Project, flake: &mut Flake);
}

// Helper functions
fn version_as_attr(v: &str, substring: &str) -> String {
    v.replace('.', substring)
}

fn version_as_attr_default(v: &str) -> String {
    version_as_attr(v, "")
}
