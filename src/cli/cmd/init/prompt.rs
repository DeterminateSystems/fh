use inquire::{Confirm, MultiSelect, Select, Text};

use crate::cli::cmd::FhError;

pub(super) struct Prompt;

impl Prompt {
    pub(super) fn bool(msg: &str) -> Result<bool, FhError> {
        Confirm::new(msg).prompt().map_err(FhError::Interactive)
    }

    pub(super) fn string(msg: &str, default: &str) -> Result<String, FhError> {
        match Text::new(msg).prompt() {
            Ok(text) => Ok(if text.is_empty() {
                String::from(default)
            } else {
                text
            }),
            Err(e) => Err(FhError::Interactive(e)),
        }
    }

    pub(super) fn select(msg: &str, options: &[&str]) -> Result<String, FhError> {
        Ok(Select::new(msg, options.to_vec()).prompt()?.to_string())
    }

    pub(super) fn multi_select(msg: &str, options: &[&str]) -> Result<Vec<String>, FhError> {
        Ok(MultiSelect::new(msg, options.to_vec())
            .prompt()?
            .iter()
            .map(|s| String::from(*s))
            .collect())
    }

    pub(super) fn maybe_string(msg: &str) -> Result<Option<String>, FhError> {
        match Text::new(msg).prompt() {
            Ok(text) => Ok(if text.is_empty() { None } else { Some(text) }),
            Err(e) => Err(FhError::Interactive(e)),
        }
    }
}
