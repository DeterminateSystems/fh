use std::path::PathBuf;

pub(crate) struct Project {
    root: PathBuf,
}

impl Project {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // Helpers
    pub(crate) fn has_file(&self, file: &str) -> bool {
        self.root.join(file).exists() && PathBuf::from(file).is_file()
    }

    pub(crate) fn has_directory(&self, dir: &str) -> bool {
        self.root.join(dir).exists() && PathBuf::from(dir).is_dir()
    }

    pub(crate) fn has_one_of(&self, files: &[&str]) -> bool {
        files.iter().any(|f| self.has_file(f))
    }
}
