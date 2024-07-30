use std::fmt::Display;

use clap::{Parser, ValueEnum};

pub(super) const NIXOS_PROFILE: &str = "/nix/var/nix/profiles/system";
pub(super) const NIXOS_SCRIPT: &str = "switch-to-configuration";

#[derive(Parser)]
pub(super) struct NixOs {
    /// The FlakeHub output reference to apply to the system profile.
    /// References must take one of two forms: {org}/{flake}/{version_req}#{attr_path} or {org}/{flake}/{version_req}.
    /// If the latter, the attribute path defaults to nixosConfigurations.{hostname}.
    pub(super) output_ref: String,

    /// The command to run from the profile's switch-to-configuration script.
    /// Takes the form: switch-to-configuration <action>.
    #[clap(name = "ACTION", default_value = "switch")]
    pub(super) action: NixOsAction,
}

impl super::ApplyType for NixOs {
    fn get_ref(&self) -> &str {
        &self.output_ref
    }

    fn default_ref(&self) -> String {
        format!(
            "nixosConfigurations.{}",
            gethostname::gethostname().to_string_lossy()
        )
    }
}

// For available commands, see
// https://github.com/NixOS/nixpkgs/blob/12100837a815473e96c9c86fdacf6e88d0e6b113/nixos/modules/system/activation/switch-to-configuration.pl#L85-L88
#[derive(Clone, Debug, ValueEnum)]
pub enum NixOsAction {
    Switch,
    Boot,
    Test,
    DryActivate,
}

impl Display for NixOsAction {
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
