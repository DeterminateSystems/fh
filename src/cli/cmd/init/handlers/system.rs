use crate::cli::cmd::init::{
    project::Project,
    prompt::{MultiSelectOption, Prompt},
};

use super::{Flake, Handler};

const SYSTEMS: &[MultiSelectOption] = &[
    MultiSelectOption(
        "x86_64-linux",
        "Linux on a 64-bit x86 processor, like Intel or AMD",
    ),
    MultiSelectOption("aarch64-linux", "Linux on a 64-bit Arm processor"),
    MultiSelectOption("x86_64-darwin", "macOS on Intel CPUs"),
    MultiSelectOption(
        "aarch64-darwin",
        "macOS on Apple Silicon, like the M1 or M2 chips",
    ),
];

pub(crate) struct System;

fn get_systems() -> Vec<String> {
    let selected = Prompt::guided_multi_select(
        "Which systems would you like to support?",
        "system",
        SYSTEMS.to_vec(),
    );

    if selected.is_empty() {
        println!("❌ You need to select at least one system to support");
        #[allow(clippy::needless_return)]
        return get_systems();
    } else {
        selected
    }
}

impl Handler for System {
    fn handle(_: &Project, flake: &mut Flake) {
        let systems = get_systems();
        flake.systems = systems;
    }
}
