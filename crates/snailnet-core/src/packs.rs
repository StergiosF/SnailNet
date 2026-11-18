//! Packs that update without a new build (PLAN.md §17.2): the site pack,
//! the app-family pack and the game-folder pack, shipped compiled in and
//! replaced by a newer **signed bundle** when the service has fetched one.
//!
//! A bundle is one TOML file, signed with the updater's minisign key:
//!
//! ```toml
//! format = 1
//! serial = 202611181530   # grows with every publish; newest wins
//! sites = '''…the text of assets/sites.toml…'''
//! families = '''…assets/families.toml…'''
//! games = '''…assets/games.toml…'''
//! ```
//!
//! A pack missing from the bundle stays the compiled-in one. A bundle is
//! used only when its signature checks out, every pack in it parses, and
//! its serial is above the compiled-in serial (`assets/packs-serial.txt`,
//! which the publish script bumps), so a new build never runs older lists.
//!
//! [`current`] is what the rest of SnailNet reads; [`install`] swaps it.

use std::path::Path;
use std::sync::{Arc, RwLock};

use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

use crate::classify::family::{Pack, PackError};
use crate::games::{GameError, GamePack};
use crate::sites::{SiteError, SitePack};

/// The bundle layout this build reads.
pub const FORMAT: u32 = 1;

/// The updater's public key (`tauri.conf.json` `plugins.updater.pubkey`,
/// base64-decoded: key id 8555215F5FF46A82). The same key signs updates
/// and packs, so there is one key to keep safe.
pub const PUBLIC_KEY: &str = "RWSCavRfXyFVhTrMkCqOy7ew/BomVFcnKK+e8E4ueo4ky07Bofznzzsz";

/// A bundle bigger than this is refused unread (today's is ~12 KB).
pub const MAX_BUNDLE: usize = 1 << 20;

/// The bundle and its signature: the names on the release and in the
/// service's data folder (`%ProgramData%\SnailNet`).
pub const FILE: &str = "packs.toml";
pub const SIG_FILE: &str = "packs.toml.minisig";

/// Where the service fetches them: the assets of the rolling GitHub
/// release `packs` (host, then the path of [`FILE`] and [`SIG_FILE`]).
pub const FEED_HOST: &str = "github.com";
pub const FEED_PATH: &str = "/StergiosF/SnailNet/releases/download/packs/";

const BUILTIN_SERIAL: &str = include_str!("../../../assets/packs-serial.txt");

#[derive(Debug, thiserror::Error)]
pub enum PacksError {
    #[error("the pack bundle is over {MAX_BUNDLE} bytes")]
    TooBig,
    #[error("the pack bundle's signature doesn't check out: {0}")]
    Signature(String),
    #[error("the pack bundle is not UTF-8")]
    Utf8,
    #[error("the pack bundle is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("pack bundle format {0} is not the one this build reads ({FORMAT})")]
    Format(u32),
    #[error(transparent)]
    Sites(#[from] SiteError),
    #[error("family pack: {0}")]
    Families(#[from] PackError),
    #[error(transparent)]
    Games(#[from] GameError),
    #[error("reading the saved packs: {0}")]
    Io(#[from] std::io::Error),
}

/// The bundle saved in `dir` (the service's data folder), checked.
/// `Ok(None)` when none is saved.
pub fn read_saved(dir: &Path) -> Result<Option<Packs>, PacksError> {
    let body = match std::fs::read(dir.join(FILE)) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let sig = std::fs::read_to_string(dir.join(SIG_FILE))?;
    Packs::from_signed(&body, &sig).map(Some)
}

/// The packs in use.
#[derive(Debug, Clone)]
pub struct Packs {
    /// [`builtin_serial`] for the compiled-in packs.
    pub serial: u64,
    pub sites: SitePack,
    pub families: Pack,
    pub games: GamePack,
}

/// Unknown keys are ignored, so a later format-1 bundle may carry more.
#[derive(Deserialize)]
struct Bundle {
    format: u32,
    serial: u64,
    sites: Option<String>,
    families: Option<String>,
    games: Option<String>,
}

/// The serial of the compiled-in packs.
pub fn builtin_serial() -> u64 {
    BUILTIN_SERIAL
        .trim()
        .parse()
        .expect("assets/packs-serial.txt holds one number")
}

impl Packs {
    pub fn builtin() -> Self {
        Self {
            serial: builtin_serial(),
            sites: SitePack::builtin(),
            families: Pack::builtin(),
            games: GamePack::builtin(),
        }
    }

    /// A bundle signed with [`PUBLIC_KEY`]; `sig` is the `.minisig` text.
    pub fn from_signed(body: &[u8], sig: &str) -> Result<Self, PacksError> {
        Self::from_signed_with(PUBLIC_KEY, body, sig)
    }

    /// As [`Packs::from_signed`], with another key (tests).
    pub fn from_signed_with(key: &str, body: &[u8], sig: &str) -> Result<Self, PacksError> {
        if body.len() > MAX_BUNDLE {
            return Err(PacksError::TooBig);
        }
        let bad = |e: minisign_verify::Error| PacksError::Signature(e.to_string());
        let key = PublicKey::from_base64(key).map_err(bad)?;
        let sig = Signature::decode(sig).map_err(bad)?;
        // `false`: only prehashed (BLAKE2b) signatures, what `tauri signer
        // sign` and minisign make by default.
        key.verify(body, &sig, false).map_err(bad)?;
        let text = std::str::from_utf8(body).map_err(|_| PacksError::Utf8)?;
        Self::parse(text)
    }

    /// The bundle's contents, with no signature check.
    fn parse(text: &str) -> Result<Self, PacksError> {
        let b: Bundle = toml::from_str(text)?;
        if b.format != FORMAT {
            return Err(PacksError::Format(b.format));
        }
        Ok(Self {
            serial: b.serial,
            sites: match b.sites {
                Some(t) => SitePack::parse(&t)?,
                None => SitePack::builtin(),
            },
            families: match b.families {
                Some(t) => Pack::parse(&t)?,
                None => Pack::builtin(),
            },
            games: match b.games {
                Some(t) => GamePack::parse(&t)?,
                None => GamePack::builtin(),
            },
        })
    }

    /// Whether `id` names a site in these packs or the compiled-in ones.
    /// Both count, so a rule made with either stays valid.
    pub fn knows_site(&self, id: &str) -> bool {
        self.sites.sites.iter().any(|s| s.id == id)
            || (self.serial != builtin_serial()
                && SitePack::builtin().sites.iter().any(|s| s.id == id))
    }
}

static CURRENT: RwLock<Option<Arc<Packs>>> = RwLock::new(None);

/// The packs in use: the compiled-in ones until [`install`] is called.
pub fn current() -> Arc<Packs> {
    if let Some(p) = CURRENT.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return p.clone();
    }
    let mut w = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    w.get_or_insert_with(|| Arc::new(Packs::builtin())).clone()
}

/// Use `packs` from now on, if it is newer than both the packs in use and
/// the compiled-in ones. True if it was installed.
pub fn install(packs: Packs) -> bool {
    let mut w = CURRENT.write().unwrap_or_else(|e| e.into_inner());
    let now = w.as_ref().map_or(builtin_serial(), |p| p.serial);
    if packs.serial <= now.max(builtin_serial()) {
        return false;
    }
    *w = Some(Arc::new(packs));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // A throwaway key pair made with `tauri signer generate` for these
    // tests only; the fixture is signed with its private key, which was
    // deleted.
    const TEST_KEY: &str = include_str!("../tests/data/packs-test.pub");
    const BODY: &str = include_str!("../tests/data/packs-test.toml");
    const SIG: &str = include_str!("../tests/data/packs-test.toml.minisig");

    #[test]
    fn the_builtin_packs_and_serial_load() {
        let p = Packs::builtin();
        assert_eq!(p.serial, builtin_serial());
        assert!(p.knows_site("youtube"));
        assert!(!p.knows_site("not-a-site"));
    }

    #[test]
    fn a_signed_bundle_loads() {
        let p = Packs::from_signed_with(TEST_KEY.trim(), BODY.as_bytes(), SIG).unwrap();
        assert_eq!(p.serial, 900_000_000_001);
        assert!(p.knows_site("example-video"));
        assert!(p.knows_site("youtube"), "built-in sites stay known");
        assert!(p.games.is_game_path(r"C:\Test Library\Game\game.exe"));
        // No `families` in the bundle: the compiled-in pack.
        assert_eq!(p.families.ids().count(), Pack::builtin().ids().count());
    }

    #[test]
    fn a_changed_bundle_or_another_key_is_refused() {
        let changed = BODY.replace("900000000001", "900000000002");
        let e = Packs::from_signed_with(TEST_KEY.trim(), changed.as_bytes(), SIG).unwrap_err();
        assert!(matches!(e, PacksError::Signature(_)), "{e}");
        let e = Packs::from_signed(BODY.as_bytes(), SIG).unwrap_err();
        assert!(matches!(e, PacksError::Signature(_)), "{e}");
        let e = Packs::from_signed_with(TEST_KEY.trim(), BODY.as_bytes(), "junk").unwrap_err();
        assert!(matches!(e, PacksError::Signature(_)), "{e}");
        let big = vec![b' '; MAX_BUNDLE + 1];
        assert!(matches!(
            Packs::from_signed_with(TEST_KEY.trim(), &big, SIG),
            Err(PacksError::TooBig)
        ));
    }

    #[test]
    fn bundle_contents_are_checked() {
        assert!(matches!(
            Packs::parse("format = 2\nserial = 5"),
            Err(PacksError::Format(2))
        ));
        let p = Packs::parse("format = 1\nserial = 5\nlater = 'ignored'").unwrap();
        assert_eq!(p.serial, 5);
        assert!(Packs::parse("format = 1\nserial = 5\ngames = 'libraries = []'").is_err());
        assert!(Packs::parse("format = 1\nserial = 5\nsites = 'x'").is_err());
        assert!(Packs::parse("format = 1").is_err());
    }

    /// `scripts/publish-packs.ps1` runs this on what it made, with
    /// `SNAILNET_PACKS_DIR` set: the real key must accept it.
    #[test]
    #[ignore]
    fn a_published_bundle_checks_out() {
        let dir = std::env::var("SNAILNET_PACKS_DIR").expect("SNAILNET_PACKS_DIR");
        let p = read_saved(Path::new(&dir))
            .unwrap()
            .expect("packs.toml in that folder");
        assert!(p.serial > 0);
        println!(
            "serial {}: {} sites, {} families",
            p.serial,
            p.sites.sites.len(),
            p.families.ids().count()
        );
    }

    #[test]
    fn only_a_newer_bundle_is_installed() {
        // The only test that installs (the packs are process-wide).
        let base = current().serial;
        let mut old = Packs::builtin();
        old.serial = base;
        assert!(!install(old));
        let mut new = Packs::builtin();
        new.serial = base + 10;
        assert!(install(new));
        assert_eq!(current().serial, base + 10);
        let mut between = Packs::builtin();
        between.serial = base + 5;
        assert!(!install(between), "never goes back");
    }
}
