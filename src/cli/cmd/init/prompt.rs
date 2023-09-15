use std::{fmt::Display, process::exit};

use inquire::{Confirm, MultiSelect, Select, Text};

pub(super) struct Prompt;

impl Prompt {
    pub(super) fn bool(msg: &str) -> bool {
        match Confirm::new(msg).prompt() {
            Ok(b) => b,
            Err(_) => exit(0),
        }
    }

    pub(super) fn select(msg: &str, options: &[&str]) -> String {
        let result = Select::new(msg, options.to_vec()).prompt();

        match result {
            Ok(s) => s.to_string(),
            Err(_) => exit(0),
        }
    }

    pub(super) fn guided_multi_select(
        msg: &str,
        thing: &str,
        options: Vec<MultiSelectOption>,
    ) -> Vec<String> {
        let result = MultiSelect::new(msg, options)
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
            .prompt();

        match result {
            Ok(s) => s.iter().map(|s| s.0.to_owned()).collect(),
            Err(_) => exit(0),
        }
    }

    pub(super) fn multi_select(msg: &str, options: &[&str]) -> Vec<String> {
        let result = MultiSelect::new(msg, options.to_vec()).prompt();

        match result {
            Ok(s) => s.iter().map(|s| String::from(*s)).collect(),
            Err(_) => exit(0),
        }
    }

    pub(super) fn maybe_string(msg: &str) -> Option<String> {
        let result = Text::new(msg).prompt();

        match result {
            Ok(s) => {
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            Err(_) => exit(0),
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
