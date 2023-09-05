use std::fmt::Display;

use inquire::{Confirm, MultiSelect, Select, Text};

use crate::cli::cmd::FhError;

pub(super) struct Prompt;

impl Prompt {
    pub(super) fn bool(msg: &str) -> Result<bool, FhError> {
        Confirm::new(msg).prompt().map_err(FhError::Interactive)
    }

    pub(super) fn select(msg: &str, options: &[&str]) -> Result<String, FhError> {
        Ok(Select::new(msg, options.to_vec()).prompt()?.to_string())
    }

    pub(super) fn guided_multi_select(
        msg: &str,
        thing: &str,
        options: Vec<MultiSelectOption>,
    ) -> Result<Vec<String>, FhError> {
        let selected: Vec<String> = MultiSelect::new(msg, options)
            .with_formatter(&|opts| {
                format!(
                    "You selected {} {}{}: {}",
                    opts.len(),
                    thing,
                    if opts.len() > 1 { "s" } else { "" },
                    opts.iter()
                        .map(|opt| opt.value.0)
                        .collect::<Vec<&str>>()
                        .join(", ")
                )
            })
            .prompt()?
            .iter()
            .map(|s| s.0.to_owned())
            .collect();

        Ok(selected)
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

#[derive(Clone)]
pub(super) struct MultiSelectOption(pub(super) &'static str, pub(super) &'static str);

impl Display for MultiSelectOption {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} — {}", self.0, self.1)
    }
}
