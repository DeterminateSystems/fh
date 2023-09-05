use std::path::PathBuf;

pub(super) struct Project {
    root: PathBuf,
}

impl Project {
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root }
    }

    // Languages
    pub(super) fn maybe_golang(&self) -> bool {
        self.has_file("go.mod")
    }

    pub(super) fn maybe_java(&self) -> bool {
        self.has_one_of(&["pom.xml", "build.gradle"])
    }

    pub(super) fn maybe_javascript(&self) -> bool {
        self.has_file("package.json")
    }

    pub(super) fn maybe_php(&self) -> bool {
        self.has_one_of(&["composer.json", "php.ini"])
    }

    pub(super) fn maybe_python(&self) -> bool {
        self.has_one_of(&["setup.py", "requirements.txt"])
    }

    pub(super) fn maybe_ruby(&self) -> bool {
        self.has_one_of(&["Gemfile", "config.ru", "Rakefile"])
    }

    pub(super) fn maybe_rust(&self) -> bool {
        self.has_file("Cargo.toml")
    }

    pub(super) fn maybe_zig(&self) -> bool {
        self.has_file("build.zig")
    }

    // Tools
    pub(super) fn maybe_gradle(&self) -> bool {
        self.has_file("build.gradle")
    }

    pub(super) fn maybe_maven(&self) -> bool {
        self.has_file("pom.xml")
    }

    pub(super) fn maybe_pnpm(&self) -> bool {
        self.has_file("pnpm-lock.yaml")
    }

    pub(super) fn maybe_yarn(&self) -> bool {
        self.has_file("yarn.lock")
    }

    // direnv
    pub(super) fn uses_direnv(&self) -> bool {
        self.has_file(".envrc")
    }

    // Helpers
    pub(super) fn has_file(&self, file: &str) -> bool {
        self.root.join(file).exists()
    }

    fn has_one_of(&self, files: &[&str]) -> bool {
        files.iter().any(|f| self.has_file(f))
    }
}
