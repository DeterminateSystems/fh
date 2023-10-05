use std::{fmt::Display, process::exit};

use inquire::{
    ui::{Color, RenderConfig, StyleSheet, Styled},
    Confirm, MultiSelect, Select, Text,
};
use lazy_static::lazy_static;

lazy_static! {
    static ref MAGENTA_TEXT: StyleSheet = StyleSheet::default().with_fg(Color::DarkMagenta);
    static ref GREY_TEXT: StyleSheet = StyleSheet::default().with_fg(Color::Grey);
    static ref PROMPT_CONFIG: RenderConfig = RenderConfig::default()
        .with_prompt_prefix(Styled::new(">").with_fg(Color::LightBlue))
        .with_selected_option(Some(*MAGENTA_TEXT))
        .with_answer(*GREY_TEXT)
        .with_help_message(*GREY_TEXT);
}

pub(super) struct Prompt;

impl Prompt {
    pub(super) fn bool(msg: &str) -> bool {
        match Confirm::new(msg)
            .with_render_config(*PROMPT_CONFIG)
            .prompt()
        {
            Ok(b) => b,
            Err(_) => exit(1),
        }
    }

    pub(super) fn select(msg: &str, options: &[&str]) -> String {
        let result = Select::new(msg, options.to_vec())
            .with_render_config(*PROMPT_CONFIG)
            .prompt();

        match result {
            Ok(s) => s.to_string(),
            Err(_) => exit(1),
        }
    }

    pub(super) fn guided_multi_select(
        msg: &str,
        thing: &str,
        options: Vec<MultiSelectOption>,
    ) -> Vec<String> {
        let defaults = options
            .iter()
            .enumerate()
            .filter(|(_idx, option)| option.is_default_selection())
            .map(|(idx, _)| idx)
            .collect::<Vec<usize>>();

        let result = MultiSelect::new(msg, options)
            .with_default(&defaults)
            .with_render_config(*PROMPT_CONFIG)
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
            Err(_) => exit(1),
        }
    }

    pub(super) fn multi_select(msg: &str, options: &[&str]) -> Vec<String> {
        let result = MultiSelect::new(msg, options.to_vec())
            .with_render_config(*PROMPT_CONFIG)
            .prompt();

        match result {
            Ok(s) => s.iter().map(|s| String::from(*s)).collect(),
            Err(_) => exit(1),
        }
    }

    pub(super) fn maybe_string(msg: &str) -> Option<String> {
        let result = Text::new(msg).with_render_config(*PROMPT_CONFIG).prompt();

        match result {
            Ok(s) => {
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            }
            Err(_) => exit(1),
        }
    }

    pub(super) fn for_language(lang: &str) -> bool {
        Self::bool(&format!("This seems to be a {lang} project. Would you like to initialize your flake with some standard dependencies for {lang}?"))
    }

    pub(super) fn for_tool(tool: &str) -> bool {
        Self::bool(&format!(
            "This seems to be a {tool} project. Would you like to add it to your environment?"
        ))
    }
}

#[derive(Clone)]
pub(super) struct MultiSelectOption(
    pub(super) &'static str,
    pub(super) &'static str,
    pub(super) bool,
);

impl MultiSelectOption {
    pub fn is_default_selection(&self) -> bool {
        self.2
    }
}

impl Display for MultiSelectOption {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} — {}", self.0, self.1)
    }
}
