use std::fmt::Display;

use clap::{Parser, ValueEnum};

use crate::cli::error::FhError;

#[derive(Parser)]
pub(super) struct NixOS {
    /// The FlakeHub output reference to apply to the system profile.
    /// References must be of this form: {org}/{flake}/{version_req}#{attr_path}
    pub(super) output_ref: String,

    /// The command to run from the profile's switch-to-configuration script.
    /// Takes the form: switch-to-configuration <action>.
    #[clap(name = "ACTION", default_value = "switch")]
    pub(super) action: Verb,
}

impl NixOS {
    pub(super) fn output_ref(&self) -> Result<String, FhError> {
        parse_output_ref(&self.output_ref)
    }
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

// This function enables you to provide simplified paths:
//
// fh apply nixos omnicorp/systems/0
//
// Here, `omnicorp/systems/0`` resolves to `omnicorp/systems/0#nixosConfigurations.$(hostname)`.
// If you need to apply a configuration at a path that doesn't conform to this pattern, you
// can still provide an explicit path.
fn parse_output_ref(path: &str) -> Result<String, FhError> {
    let hostname = gethostname::gethostname().to_string_lossy().to_string();

    Ok(match path.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => path.to_string(),
        [release] => format!("{release}#nixosConfigurations.{hostname}"),
        _ => return Err(FhError::MalformedNixOSConfigPath(path.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::cmd::apply::nixos::parse_output_ref;

    #[test]
    fn test_parse_profile_path() {
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
