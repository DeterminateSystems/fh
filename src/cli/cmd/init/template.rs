use std::collections::HashMap;

use handlebars::{handlebars_helper, Handlebars};
use serde::Serialize;
use serde_json::Value;

use crate::cli::cmd::FhError;

use super::{dev_shell::DevShell, handlers::Input};

#[derive(Debug, Serialize)]
pub(super) struct TemplateData {
    pub(super) description: Option<String>,
    pub(super) inputs: HashMap<String, Input>,
    pub(super) systems: Vec<String>,
    pub(super) dev_shells: HashMap<String, DevShell>,
    pub(super) overlay_refs: Vec<String>,
    pub(super) overlay_attrs: HashMap<String, String>,
    pub(super) shell_hook: Option<String>,
    pub(super) fh_version: String,
    // This is tricky to determine inside the template because we need to check that
    // either overlay_refs or overlay_attrs is non-empty, so we calculate that in Rust
    // and set a Boolean here instead
    pub(super) has_overlays: bool,
    pub(super) doc_comments: bool,
}

handlebars_helper!(is_false: |b: bool| !b);

impl TemplateData {
    pub(super) fn as_json(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub(super) fn validate(&self) -> Result<(), FhError> {
        if self.inputs.is_empty() {
            return Err(FhError::NoInputs);
        }

        Ok(())
    }

    pub(super) fn render(&self) -> Result<String, FhError> {
        self.validate()?;

        let mut handlebars = Handlebars::new();

        // This helper is necessary because Handlebars `not x` in Handlebars could mean `x = false` or it
        // could mean that `x` is not defined. So we need a specific handler for the Boolean case.
        handlebars.register_helper("is_false", Box::new(is_false));

        handlebars
            .register_template_string("flake", include_str!("../../../../assets/flake.hbs"))
            .map_err(|err| FhError::Template(Box::new(err)))?;

        handlebars
            .render("flake", &self.as_json()?)
            .map_err(FhError::Render)
    }
}
