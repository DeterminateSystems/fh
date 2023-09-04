use std::path::PathBuf;

pub(super) struct Project {
    root: PathBuf,
}

impl Project {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // Languages
    pub(super) fn maybe_python(&self) -> bool {
        self.has_one_of(&["setup.py", "requirements.txt"])
    }

    pub(super) fn maybe_javascript(&self) -> bool {
        self.has_file("package.json")
    }

    pub(super) fn maybe_golang(&self) -> bool {
        self.has_file("go.mod")
    }

    pub(super) fn maybe_rust(&self) -> bool {
        self.has_file("Cargo.toml")
    }

    pub(super) fn maybe_zig(&self) -> bool {
        self.has_file("build.zig")
    }

    // Helpers
    pub(super) fn has_file(&self, file: &str) -> bool {
        self.root.join(file).exists()
    }

    #[allow(dead_code)]
    fn has_one_of(&self, files: &[&str]) -> bool {
        files.iter().any(|f| self.has_file(f))
    }
}
