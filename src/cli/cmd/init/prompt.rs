use std::fmt::Display;

use inquire::{Confirm, MultiSelect, Select, Text};

pub(super) struct Prompt;

impl Prompt {
    pub(super) fn bool(msg: &str) -> bool {
        Confirm::new(msg).prompt().unwrap()
    }

    pub(super) fn select(msg: &str, options: &[&str]) -> String {
        Select::new(msg, options.to_vec())
            .prompt()
            .unwrap()
            .to_string()
    }

    pub(super) fn guided_multi_select(
        msg: &str,
        thing: &str,
        options: Vec<MultiSelectOption>,
    ) -> Vec<String> {
        MultiSelect::new(msg, options)
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
            .prompt()
            .unwrap()
            .iter()
            .map(|s| s.0.to_owned())
            .collect()
    }

    pub(super) fn multi_select(msg: &str, options: &[&str]) -> Vec<String> {
        MultiSelect::new(msg, options.to_vec())
            .prompt()
            .unwrap()
            .iter()
            .map(|s| String::from(*s))
            .collect()
    }

    pub(super) fn maybe_string(msg: &str) -> Option<String> {
        let result = Text::new(msg).prompt().unwrap();
        if result.is_empty() {
            None
        } else {
            Some(result)
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
