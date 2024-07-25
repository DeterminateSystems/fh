use std::fmt::Display;

use clap::{Parser, ValueEnum};

use crate::cli::{cmd::parse_release_ref, error::FhError};

pub(super) const NIXOS_PROFILE: &str = "system";
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

impl NixOs {
    pub(super) fn output_ref(&self) -> Result<String, FhError> {
        parse_output_ref(&self.output_ref)
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

// This function enables you to provide simplified paths:
//
// fh apply nixos omnicorp/systems/0.1
//
// Here, `omnicorp/systems/0.1` resolves to `omnicorp/systems/0.1#nixosConfigurations.$(hostname)`.
// If you need to apply a configuration at a path that doesn't conform to this pattern, you
// can still provide an explicit path.
fn parse_output_ref(output_ref: &str) -> Result<String, FhError> {
    let hostname = gethostname::gethostname().to_string_lossy().to_string();

    Ok(match output_ref.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => parse_release_ref(output_ref)?,
        [release] => format!(
            "{}#nixosConfigurations.{hostname}",
            parse_release_ref(release)?
        ),
        _ => return Err(FhError::MalformedOutputRef(output_ref.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::cmd::apply::nixos::parse_output_ref;

    #[test]
    fn test_parse_nixos_output_ref() {
        let hostname = gethostname::gethostname().to_string_lossy().to_string();

        let cases: Vec<(&str, String)> = vec![
            (
                "foo/bar/*",
                format!("foo/bar/*#nixosConfigurations.{hostname}"),
            ),
            (
                "foo/bar/0.1.*",
                format!("foo/bar/0.1.*#nixosConfigurations.{hostname}"),
            ),
            (
                "omnicorp/web/0.1.2#nixosConfigurations.auth-server",
                "omnicorp/web/0.1.2#nixosConfigurations.auth-server".to_string(),
            ),
            (
                "omnicorp/web/0.1.2#packages.x86_64-linux.default",
                "omnicorp/web/0.1.2#packages.x86_64-linux.default".to_string(),
            ),
        ];

        for case in cases {
            assert_eq!(parse_output_ref(case.0).unwrap(), case.1);
        }
    }
}
