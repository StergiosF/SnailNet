//! `config.toml`: load with fallbacks, save atomically (PLAN.md §13.1).
//!
//! - **Save** writes `config.toml.tmp`, flushes it to disk, then swaps it in
//!   with `ReplaceFileW`, which moves the previous file to `config.toml.bak`.
//!   A crash mid-save leaves either the old file or the new one, never half.
//! - **Load** tries `config.toml`, then `config.toml.bak`, then the defaults.
//!   An unreadable `config.toml` is moved to `config.toml.bad` (so nothing
//!   the user wrote is lost) and replaced by whichever fallback worked.
//!   A file from a *newer* SnailNet is left alone: this build runs on
//!   defaults without overwriting it.

use std::io::{self, Write};

use snailnet_core::rules::{Config, RulesError};
use snailnet_win::fs::replace_file;

use crate::paths::DataDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `config.toml` as it was.
    File,
    /// `config.toml` was missing or unreadable; `config.toml.bak` was used
    /// and restored.
    Backup,
    /// Nothing usable; the defaults were used (and written, unless the file
    /// belongs to a newer SnailNet).
    Default,
}

#[derive(Debug)]
pub struct Loaded {
    pub config: Config,
    pub source: Source,
    /// What was wrong with the files that were skipped, for the log.
    pub problems: Vec<String>,
}

enum Read {
    Missing,
    Bad(RulesError),
    Good(Box<Config>),
}

fn read(path: &std::path::Path) -> io::Result<Read> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(match Config::parse(&text) {
            Ok(c) => Read::Good(Box::new(c)),
            Err(e) => Read::Bad(e),
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Read::Missing),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(Read::Bad(RulesError::Syntax(
            "the file is not UTF-8 text".into(),
        ))),
        Err(e) => Err(e),
    }
}

pub fn load(dir: &DataDir) -> io::Result<Loaded> {
    let mut problems = Vec::new();
    let primary = match read(&dir.config())? {
        Read::Good(config) => {
            return Ok(Loaded {
                config: *config,
                source: Source::File,
                problems,
            });
        }
        Read::Bad(e @ RulesError::TooNew(_)) => {
            problems.push(format!(
                "config.toml: {e}; running on defaults, file left as is"
            ));
            return Ok(Loaded {
                config: Config::default(),
                source: Source::Default,
                problems,
            });
        }
        Read::Bad(e) => {
            problems.push(format!("config.toml: {e}; moved to config.toml.bad"));
            std::fs::rename(dir.config(), dir.config_bad())?;
            false
        }
        Read::Missing => true,
    };
    let (config, source) = match read(&dir.config_bak())? {
        Read::Good(c) => (*c, Source::Backup),
        Read::Bad(e) => {
            problems.push(format!("config.toml.bak: {e}"));
            (Config::default(), Source::Default)
        }
        Read::Missing => (Config::default(), Source::Default),
    };
    if !(primary && source == Source::Default) {
        problems.push(match source {
            Source::Backup => "restored config.toml from config.toml.bak".into(),
            _ => "wrote a default config.toml".into(),
        });
    }
    write(dir, &config, false)?;
    Ok(Loaded {
        config,
        source,
        problems,
    })
}

/// Validate and save `config`; the file it replaces becomes the `.bak`.
pub fn save(dir: &DataDir, config: &Config) -> io::Result<()> {
    config
        .validate()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    write(dir, config, true)
}

fn write(dir: &DataDir, config: &Config, keep_backup: bool) -> io::Result<()> {
    let tmp = dir.config_tmp();
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(config.to_toml().as_bytes())?;
        f.sync_all()?;
    }
    if keep_backup {
        replace_file(&dir.config(), &tmp, &dir.config_bak())
    } else {
        // Restoring: config.toml is absent (missing or moved to .bad), so
        // this is a plain move and the .bak we restored from stays put.
        std::fs::rename(&tmp, dir.config())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snailnet_core::rules::{Rule, Target};

    fn dir() -> (tempfile::TempDir, DataDir) {
        let t = tempfile::tempdir().unwrap();
        let d = DataDir::new(t.path());
        d.create().unwrap();
        (t, d)
    }

    fn with_line(mbps: f64) -> Config {
        let mut c = Config::default();
        c.line.down_mbps = Some(mbps);
        c
    }

    fn text(p: std::path::PathBuf) -> String {
        std::fs::read_to_string(p).unwrap()
    }

    #[test]
    fn first_run_writes_defaults() {
        let (_t, d) = dir();
        let l = load(&d).unwrap();
        assert_eq!(l.source, Source::Default);
        assert_eq!(l.config, Config::default());
        assert!(l.problems.is_empty(), "{:?}", l.problems);
        assert_eq!(Config::parse(&text(d.config())).unwrap(), Config::default());
        // Second start reads the file.
        assert_eq!(load(&d).unwrap().source, Source::File);
    }

    #[test]
    fn save_keeps_the_previous_file_as_bak() {
        let (_t, d) = dir();
        save(&d, &with_line(10.0)).unwrap();
        assert!(!d.config_bak().exists());
        save(&d, &with_line(20.0)).unwrap();
        assert_eq!(load(&d).unwrap().config, with_line(20.0));
        assert_eq!(
            Config::parse(&text(d.config_bak())).unwrap(),
            with_line(10.0)
        );
        assert!(!d.config_tmp().exists());
    }

    #[test]
    fn save_refuses_an_invalid_config() {
        let (_t, d) = dir();
        let mut c = Config::default();
        c.profiles[0].rules.push(Rule {
            target: Target::family("a"),
            share: 9_600,
            priority: Default::default(),
            ceiling: None,
            enabled: true,
        });
        assert_eq!(
            save(&d, &c).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!d.config().exists());
    }

    #[test]
    fn corrupt_config_falls_back_to_bak() {
        let (_t, d) = dir();
        save(&d, &with_line(10.0)).unwrap();
        save(&d, &with_line(20.0)).unwrap();
        std::fs::write(d.config(), "version = 1\nactive_profile = [").unwrap();
        let l = load(&d).unwrap();
        assert_eq!(l.source, Source::Backup);
        assert_eq!(l.config, with_line(10.0));
        assert_eq!(l.problems.len(), 2, "{:?}", l.problems);
        // The broken file is kept for the user; the good one is back.
        assert_eq!(text(d.config_bad()), "version = 1\nactive_profile = [");
        assert_eq!(Config::parse(&text(d.config())).unwrap(), with_line(10.0));
        assert!(d.config_bak().exists());
        assert_eq!(load(&d).unwrap().source, Source::File);
    }

    #[test]
    fn missing_config_uses_bak() {
        let (_t, d) = dir();
        save(&d, &with_line(10.0)).unwrap();
        save(&d, &with_line(20.0)).unwrap();
        std::fs::remove_file(d.config()).unwrap();
        let l = load(&d).unwrap();
        assert_eq!((l.source, l.config), (Source::Backup, with_line(10.0)));
    }

    #[test]
    fn both_bad_means_defaults() {
        let (_t, d) = dir();
        std::fs::write(d.config(), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(d.config_bak(), "nonsense").unwrap();
        let l = load(&d).unwrap();
        assert_eq!((l.source, &l.config), (Source::Default, &Config::default()));
        assert_eq!(l.problems.len(), 3, "{:?}", l.problems);
        assert!(d.config_bad().exists());
        assert_eq!(load(&d).unwrap().source, Source::File);
    }

    #[test]
    fn a_newer_config_is_left_alone() {
        let (_t, d) = dir();
        let newer = "version = 7\nwhatever = true\n";
        std::fs::write(d.config(), newer).unwrap();
        let l = load(&d).unwrap();
        assert_eq!((l.source, &l.config), (Source::Default, &Config::default()));
        assert!(l.problems[0].contains("newer"));
        assert_eq!(text(d.config()), newer);
        assert!(!d.config_bad().exists());
    }
}
