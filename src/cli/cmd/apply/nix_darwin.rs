use clap::Parser;

use crate::cli::{cmd::parse_release_ref, error::FhError};

pub(super) const NIX_DARWIN_SCRIPT: &str = "darwin-rebuild";
pub(super) const NIX_DARWIN_PROFILE: &str = "system";

#[derive(Parser)]
pub(super) struct NixDarwin {
    /// The FlakeHub output reference for the nix-darwin configuration.
    /// References must take one of two forms: {org}/{flake}/{version_req}#{attr_path} or {org}/{flake}/{version_req}.
    /// If the latter, the attribute path defaults to darwinConfigurations.{devicename}.system, where devicename
    /// is the output of scutil --get LocalHostName.
    pub(super) output_ref: String,

    /// The command or commands to pass to darwin-rebuild.
    #[arg(trailing_var_arg = true, default_value = "activate")]
    pub(super) command: Vec<String>,
}

impl NixDarwin {
    pub(super) fn output_ref(&self) -> Result<String, FhError> {
        parse_output_ref(&self.output_ref)
    }
}

// This function enables you to provide simplified paths:
//
// fh apply nix-darwin omnicorp/home/0.1
//
// Here, `omnicorp/systems/0.1` resolves to `omnicorp/systems/0#darwinConfigurations.$(devicename).system`.
// If you need to apply a configuration at a path that doesn't conform to this pattern, you
// can still provide an explicit path.
fn parse_output_ref(output_ref: &str) -> Result<String, FhError> {
    let devicename = whoami::devicename();

    Ok(match output_ref.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => parse_release_ref(output_ref)?,
        [release] => format!(
            "{}#darwinConfigurations.{devicename}.system",
            parse_release_ref(release)?
        ),
        _ => return Err(FhError::MalformedOutputRef(output_ref.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::cmd::apply::nix_darwin::parse_output_ref;

    #[test]
    fn test_parse_nixos_output_ref() {
        let devicename = whoami::devicename();

        let cases: Vec<(&str, String)> = vec![
            (
                "foo/bar/*",
                format!("foo/bar/*#darwinConfigurations.{devicename}.system"),
            ),
            (
                "foo/bar/0.1.*",
                format!("foo/bar/0.1.*#darwinConfigurations.{devicename}.system"),
            ),
            (
                "omnicorp/web/0.1.2#darwinConfigurations.my-config",
                "omnicorp/web/0.1.2#darwinConfigurations.my-config".to_string(),
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
