use clap::Parser;

pub(super) const HOME_MANAGER_SCRIPT: &str = "activate";

#[derive(Parser)]
pub(super) struct HomeManager {
    /// The FlakeHub output reference for the Home Manager configuration.
    /// References must take one of two forms: {org}/{flake}/{version_req}#{attr_path} or {org}/{flake}/{version_req}.
    /// If the latter, the attribute path defaults to homeConfigurations.{whoami}.
    pub(super) output_ref: String,
}

impl super::ApplyType for HomeManager {
    fn get_ref(&self) -> &str {
        &self.output_ref
    }
    fn default_ref(&self) -> String {
        format!("homeConfigurations.{}", whoami::username())
    }
}
