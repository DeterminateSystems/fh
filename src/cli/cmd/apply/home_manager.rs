use clap::Parser;

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

    fn profile_path(&self) -> Option<&std::path::Path> {
        None
    }

    fn requires_root(&self) -> bool {
        false
    }

    fn relative_path(&self) -> &std::path::Path {
        std::path::Path::new("activate")
    }

    fn action(&self) -> Option<String> {
        None
    }
}
