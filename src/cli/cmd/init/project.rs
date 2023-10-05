use std::path::PathBuf;

pub(crate) struct Project {
    root: PathBuf,
}

impl Project {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // Helpers
    pub(super) fn has_file_or_directory(&self, path: &str) -> bool {
        self.root.join(path).exists()
    }

    pub(super) fn has_one_of(&self, files: &[&str]) -> bool {
        files.iter().any(|f| self.has_file_or_directory(f))
    }
}
