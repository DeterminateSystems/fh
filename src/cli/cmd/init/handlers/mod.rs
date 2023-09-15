use std::collections::HashMap;

mod go;
mod java;
mod javascript;
mod php;
mod python;
mod ruby;
mod rust;
mod system;
mod zig;

pub(crate) use go::Go;
pub(crate) use java::Java;
pub(crate) use javascript::JavaScript;
pub(crate) use php::Php;
pub(crate) use python::Python;
pub(crate) use ruby::Ruby;
pub(crate) use rust::Rust;
pub(crate) use system::System;
pub(crate) use zig::Zig;

use super::{dev_shell::DevShell, project::Project};

#[derive(Default)]
pub(crate) struct Flake {
    pub(crate) description: Option<String>,
    pub(crate) systems: Vec<String>,
    pub(crate) dev_shells: HashMap<String, DevShell>,
    pub(crate) inputs: HashMap<String, String>,
    pub(crate) overlay_refs: Vec<String>,
    pub(crate) overlay_attrs: HashMap<String, String>,
    pub(crate) dev_shell_packages: Vec<String>,
    pub(crate) env_vars: HashMap<String, String>,
}

pub(crate) trait Handler {
    fn handle(project: &Project, flake: &mut Flake);
}

// Helper functions
pub(self) fn version_as_attr(v: &str) -> String {
    v.replace('.', "")
}
