use std::collections::HashMap;

use handlebars::Handlebars;
use serde::Serialize;
use serde_json::Value;

use crate::cli::cmd::FhError;

use super::{dev_shell::DevShell, handlers::Input};

#[derive(Debug, Serialize)]
pub(crate) struct TemplateData {
    pub(crate) description: Option<String>,
    pub(crate) inputs: HashMap<String, Input>,
    pub(crate) systems: Vec<String>,
    pub(crate) dev_shells: HashMap<String, DevShell>,
    pub(crate) overlay_refs: Vec<String>,
    pub(crate) overlay_attrs: HashMap<String, String>,
    pub(crate) shell_hook: Option<String>,
    pub(crate) fh_version: String,
    // This is tricky to determine inside the template because we need to check that
    // either overlay_refs or overlay_attrs is non-empty, so we calculate that in Rust
    // and set a Boolean here instead
    pub(crate) has_overlays: bool,
    pub(crate) doc_comments: bool,
}

impl TemplateData {
    pub(crate) fn as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub(crate) fn validate(&self) -> Result<(), FhError> {
        if self.inputs.is_empty() {
            return Err(FhError::NoInputs);
        }

        Ok(())
    }

    pub(crate) fn render(&self) -> Result<String, FhError> {
        self.validate()?;

        let mut handlebars = Handlebars::new();
        handlebars
            .register_template_string("flake", include_str!("../../../../assets/flake.hbs"))
            .map_err(|err| FhError::Template(Box::new(err)))?;

        handlebars
            .render("flake", &self.as_json()?)
            .map_err(FhError::Render)
    }
}
