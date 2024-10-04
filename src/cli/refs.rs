use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{cli::cmd::list::FLAKEHUB_WEB_ROOT, flakehub_url};

use super::error::FhError;

// Parses a flake reference as a string to construct paths of the form:
// https://api.flakehub.com/f/{org}/{flake}/{version_constraint}/output/{attr_path}
pub(crate) struct FlakeOutputRef {
    pub(crate) org: String,
    pub(crate) project: String,
    pub(crate) version_constraint: String,
    pub(crate) attr_path: String,
}

impl Display for FlakeOutputRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}/{}#{}",
            self.org, self.project, self.version_constraint, self.attr_path
        )
    }
}

impl TryFrom<String> for FlakeOutputRef {
    type Error = FhError;

    fn try_from(output_ref: String) -> Result<Self, Self::Error> {
        let parts: Vec<&str> = output_ref.split('#').collect();

        if let Some(release_parts) = parts.first() {
            let Some(attr_path) = parts.get(1) else {
                Err(FhError::MalformedFlakeOutputRef(
                    output_ref,
                    String::from("missing the output attribute path"),
                ))?
            };

            match release_parts.split('/').collect::<Vec<_>>()[..] {
                [org, project, version_constraint] => {
                    validate_segment(org, "org")?;
                    validate_segment(project, "project")?;
                    validate_segment(version_constraint, "version constraint")?;
                    validate_segment(attr_path, "attribute path")?;

                    Ok(FlakeOutputRef {
                        org: org.to_string(),
                        project: project.to_string(),
                        version_constraint: version_constraint.to_string(),
                        attr_path: attr_path.to_string(),
                    })
                }
                _ => Err(FhError::MalformedFlakeOutputRef(
                    output_ref,
                    String::from(
                        "release reference must be of the form {{org}}/{{project}}/{{version_req}}",
                    ),
                )),
            }
        } else {
            Err(FhError::MalformedFlakeOutputRef(
                output_ref,
                String::from(
                    "must be of the form {{org}}/{{project}}/{{version_req}}#{{attr_path}}",
                ),
            ))
        }
    }
}

pub(crate) fn parse_flake_output_ref_with_default_path(
    frontend_addr: &url::Url,
    output_ref: &str,
    default_path: &str,
) -> Result<FlakeOutputRef, FhError> {
    let with_default_output_path = match output_ref.split('#').collect::<Vec<_>>()[..] {
        [_release, _output_path] => output_ref.to_string(),
        [_release] => format!("{}#{}", output_ref, default_path),
        _ => {
            return Err(FhError::MalformedFlakeOutputRef(
                output_ref.to_string(),
                String::from(
                    "must be of the form {{org}}/{{project}}/{{version_req}}#{{attr_path}}",
                ),
            ))
        }
    };

    parse_flake_output_ref(frontend_addr, &with_default_output_path)
}

pub(crate) fn parse_flake_output_ref(
    frontend_addr: &url::Url,
    output_ref: &str,
) -> Result<FlakeOutputRef, FhError> {
    // Ensures that users can use both forms:
    // 1. https://flakehub/f/{org}/{project}/{version_req}#{output}
    // 2. {org}/{project}/{version_req}#{output}
    let output_ref = String::from(
        output_ref
            .strip_prefix(frontend_addr.join("f/")?.as_str())
            .unwrap_or(output_ref),
    );

    output_ref.try_into()
}

// Simple flake refs are of the form {org}/{project}, for example NixOS/nixpkgs
#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct SimpleFlakeRef {
    pub(crate) org: String,
    pub(crate) project: String,
}

impl SimpleFlakeRef {
    pub(crate) fn url(&self) -> url::Url {
        flakehub_url!(FLAKEHUB_WEB_ROOT, "flake", &self.org, &self.project)
    }
}

impl Display for SimpleFlakeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.org, self.project)
    }
}

impl TryFrom<String> for SimpleFlakeRef {
    type Error = FhError;

    fn try_from(flake_ref: String) -> Result<Self, Self::Error> {
        let (org, project) = match flake_ref.split('/').collect::<Vec<_>>()[..] {
            // `nixos/nixpkgs`
            [org, repo] => (org, repo),
            _ => {
                return Err(FhError::Parse(format!(
                    "flake ref {flake_ref} invalid; must be of the form {{org}}/{{project}}"
                )))
            }
        };
        Ok(Self {
            org: String::from(org),
            project: String::from(project),
        })
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct VersionRef {
    pub(crate) version: semver::Version,
    pub(crate) simplified_version: semver::Version,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct OrgRef {
    pub(crate) name: String,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct ReleaseRef {
    pub(crate) version: String,
}

/*
// Ensure that release refs are of the form {org}/{project}/{version_req}
fn parse_release_ref(flake_ref: &str) -> Result<String, FhError> {
    match flake_ref.split('/').collect::<Vec<_>>()[..] {
        [org, project, version_req] => {
            validate_segment(org)?;
            validate_segment(project)?;
            validate_segment(version_req)?;

            Ok(flake_ref.to_string())
        }
        _ => Err(FhError::FlakeParse(format!(
            "flake ref {flake_ref} invalid; must be of the form {{org}}/{{project}}/{{version_req}}"
        ))),
    }
}
*/

// Ensure that orgs, project names, and the like don't contain whitespace.
// This function may apply other validations in the future.
fn validate_segment(s: &str, field: &str) -> Result<(), FhError> {
    if s.chars().any(char::is_whitespace) {
        return Err(FhError::Parse(format!(
            "{} in path segment contains whitespace: \"{}\"",
            field, s
        )));
    }

    Ok(())
}
