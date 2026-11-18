//! Where the service keeps its files (PLAN.md §13.1).

use std::io;
use std::path::{Path, PathBuf};

use snailnet_win::fs::{DATA_DIR_SDDL, protect_dir};

/// The service's data folder and the files in it.
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%ProgramData%\SnailNet` (normally `C:\ProgramData\SnailNet`).
    pub fn system() -> Self {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        Self::new(base.join("SnailNet"))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The apps SnailNet has met (PLAN.md §11.4).
    pub fn known_apps(&self) -> PathBuf {
        self.root.join("known-apps.txt")
    }

    pub fn config(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub(crate) fn config_tmp(&self) -> PathBuf {
        self.root.join("config.toml.tmp")
    }

    pub fn config_bak(&self) -> PathBuf {
        self.root.join("config.toml.bak")
    }

    /// Where an unreadable `config.toml` is moved, so it isn't lost.
    pub fn config_bad(&self) -> PathBuf {
        self.root.join("config.toml.bad")
    }

    /// Present while the service runs; left behind by a crash.
    pub(crate) fn running_marker(&self) -> PathBuf {
        self.root.join("running")
    }

    /// Times of recent crashes and fail-opens, one Unix time per line.
    pub(crate) fn crash_log(&self) -> PathBuf {
        self.root.join("crashes.txt")
    }

    /// The newest signed pack bundle fetched (PLAN.md §17.2), and its
    /// minisign signature. The app reads them too (Users may read).
    pub fn packs(&self) -> PathBuf {
        self.root.join(snailnet_core::packs::FILE)
    }

    pub fn packs_sig(&self) -> PathBuf {
        self.root.join(snailnet_core::packs::SIG_FILE)
    }

    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// Create the folder and its `logs` subfolder.
    pub fn create(&self) -> io::Result<()> {
        std::fs::create_dir_all(self.logs())
    }

    /// Create the folder and give it the service ACL: SYSTEM and
    /// Administrators write, Users read (§13.1). Needs admin rights.
    pub fn create_protected(&self) -> io::Result<()> {
        self.create()?;
        protect_dir(&self.root, DATA_DIR_SDDL)
    }
}
