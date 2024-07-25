use clap::Parser;

use crate::cli::{cmd::parse_release_ref, error::FhError};

pub(super) const HOME_MANAGER_SCRIPT: &str = "activate";

#[derive(Parser)]
pub(super) struct HomeManager {
    /// The FlakeHub output reference for the Home Manager configuration.
    /// References must take one of two forms: {org}/{flake}/{version_req}#{attr_path} or {org}/{flake}/{version_req}.
    /// If the latter, the attribute path defaults to homeConfigurations.{whoami}.
    pub(super) output_ref: String,
}

impl HomeManager {
    pub(super) fn output_ref(&self) -> Result<String, FhError> {
        parse_output_ref(&self.output_ref)
    }
}

// This function enables you to provide simplified paths:
//
// fh apply home-manager omnicorp/home/0.1
//
// Here, `omnicorp/systems/0.1` resolves to `omnicorp/systems/0.1#homeConfigurations.$(whoami)`.
// If you need to apply a configuration at a path that doesn't conform to this pattern, you
// can still provide an explicit path.
fn parse_output_ref(output_ref: &str) -> Result<String, FhError> {
    let username = whoami::username();

    Ok(match output_ref.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => parse_release_ref(output_ref)?,
        [release] => format!(
            "{}#homeConfigurations.{username}",
            parse_release_ref(release)?
        ),
        _ => return Err(FhError::MalformedOutputRef(output_ref.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use crate::cli::cmd::apply::home_manager::parse_output_ref;

    #[test]
    fn test_parse_home_manager_output_ref() {
        let username = whoami::username();

        let cases: Vec<(&str, String)> = vec![
            (
                "foo/bar/*",
                format!("foo/bar/*#homeConfigurations.{username}"),
            ),
            (
                "foo/bar/0.1.*",
                format!("foo/bar/0.1.*#homeConfigurations.{username}"),
            ),
            (
                "omnicorp/web/0.1.2#homeConfigurations.my-config",
                "omnicorp/web/0.1.2#homeConfigurations.my-config".to_string(),
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
