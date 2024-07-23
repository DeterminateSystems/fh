use std::fmt::Display;

use clap::{Parser, ValueEnum};

#[derive(Parser)]
pub(super) struct NixOS {
    /// The command to run from the profile's switch-to-configuration script.
    /// Takes the form: switch-to-configuration <cmd>.
    #[arg(
        long,
        env = "FH_APPLY_NIXOS_CMD",
        name = "CMD",
        default_value = "switch"
    )]
    pub(super) run: Verb,

    /// The FlakeHub output reference to apply to the system profile.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    pub(super) output_ref: String,
}

// For available commands, see
// https://github.com/NixOS/nixpkgs/blob/12100837a815473e96c9c86fdacf6e88d0e6b113/nixos/modules/system/activation/switch-to-configuration.pl#L85-L88
#[derive(Clone, Debug, ValueEnum)]
pub enum Verb {
    Switch,
    Boot,
    Test,
    DryActivate,
}

impl Display for Verb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Switch => "switch",
                Self::Boot => "boot",
                Self::Test => "test",
                Self::DryActivate => "dry-activate",
            }
        )
    }
}
