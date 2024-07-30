use clap::Parser;

pub(super) const DARWIN_REBUILD_ACTION: &str = "activate";
pub(super) const NIX_DARWIN_SCRIPT: &str = "darwin-rebuild";

#[derive(Parser)]
pub(super) struct NixDarwin {
    /// The FlakeHub output reference for the nix-darwin configuration.
    /// References must take one of two forms: {org}/{flake}/{version_req}#{attr_path} or {org}/{flake}/{version_req}.
    /// If the latter, the attribute path defaults to darwinConfigurations.{devicename}.system, where devicename
    /// is the output of scutil --get LocalHostName.
    pub(super) output_ref: String,

    #[arg(
        long,
        short,
        env = "FH_APPLY_PROFILE",
        default_value = "/nix/var/nix/profiles/system"
    )]
    pub(super) profile: std::path::PathBuf,
}

impl super::ApplyType for NixDarwin {
    fn get_ref(&self) -> &str {
        &self.output_ref
    }

    fn default_ref(&self) -> String {
        format!("darwinConfigurations.{}", whoami::devicename(),)
    }
}
