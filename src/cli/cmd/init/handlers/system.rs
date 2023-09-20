use crate::cli::cmd::init::{
    project::Project,
    prompt::{MultiSelectOption, Prompt},
};

use super::{Flake, Handler};

#[cfg(target_os = "linux")]
const IS_LINUX: bool = true;
#[cfg(not(target_os = "linux"))]
const IS_LINUX: bool = false;

#[cfg(target_os = "macos")]
const IS_MACOS: bool = true;
#[cfg(not(target_os = "macos"))]
const IS_MACOS: bool = false;

const SYSTEMS: &[MultiSelectOption] = &[
    MultiSelectOption(
        "x86_64-linux",
        "Linux on a 64-bit x86 processor, like Intel or AMD",
        IS_LINUX,
    ),
    MultiSelectOption(
        "aarch64-darwin",
        "macOS on Apple Silicon, like the M1 or M2 chips",
        IS_MACOS,
    ),
    MultiSelectOption("x86_64-darwin", "macOS on Intel CPUs", false),
    MultiSelectOption("aarch64-linux", "Linux on a 64-bit Arm processor", false),
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
