//! Is this program a game? (PLAN.md §11.5 "Game detection".) The folder
//! test: an exe under a game launcher's library is a game. The app adds
//! the other test (exclusive fullscreen) from Windows itself.

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum GameError {
    #[error("game pack is not valid TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("game pack: {0}")]
    Invalid(String),
}

/// The game launchers' library folders: `assets/games.toml`, or a newer
/// signed pack (PLAN.md §17.2, [`crate::packs`]).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamePack {
    /// Folders whose exes are games; lowercase, `\`-separated.
    libraries: Vec<String>,
    /// Folders inside a library that hold a launcher, not a game.
    #[serde(default)]
    launchers: Vec<String>,
    /// Exe file names that are games wherever they're installed
    /// (Battle.net games, Roblox, standalone installs); lowercase.
    #[serde(default)]
    exes: Vec<String>,
}

const BUILTIN_PACK: &str = include_str!("../../../assets/games.toml");

impl GamePack {
    pub fn builtin() -> Self {
        Self::parse(BUILTIN_PACK).expect("assets/games.toml is checked by the tests")
    }

    /// Entries are lowercased and `/` becomes `\`; each must start and
    /// end with `\` (whole folder names) and have a name between.
    pub fn parse(text: &str) -> Result<Self, GameError> {
        let mut pack: GamePack = toml::from_str(text)?;
        if pack.libraries.is_empty() {
            return Err(GameError::Invalid("no libraries".into()));
        }
        for e in pack.exes.iter_mut() {
            *e = e.trim().to_lowercase();
            if e.contains(['\\', '/']) || !e.ends_with(".exe") || e.len() < 5 {
                return Err(GameError::Invalid(format!(
                    "{e:?} must be an exe file name"
                )));
            }
        }
        for e in pack.libraries.iter_mut().chain(pack.launchers.iter_mut()) {
            *e = e.replace('/', "\\").to_lowercase();
            if e.len() < 3 || !e.starts_with('\\') || !e.ends_with('\\') {
                return Err(GameError::Invalid(format!(
                    "{e:?} must start and end with \\"
                )));
            }
        }
        Ok(pack)
    }

    /// Whether the exe at `path` is a game: its file name is a known
    /// game's, or it sits in a game launcher's library (and not in a
    /// launcher's own folder there).
    pub fn is_game_path(&self, path: &str) -> bool {
        let p = path.replace('/', "\\").to_lowercase();
        let exe = p.rsplit('\\').next().unwrap_or(&p);
        if self.exes.iter().any(|e| e == exe) {
            return true;
        }
        self.libraries.iter().any(|l| p.contains(l.as_str()))
            && !self.launchers.iter().any(|l| p.contains(l.as_str()))
    }
}

/// Whether the exe at `path` sits in a game launcher's library, by the
/// packs in use ([`crate::packs::current`]).
pub fn in_game_library(path: &str) -> bool {
    crate::packs::current().games.is_game_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_in_launcher_libraries_are_games() {
        for p in [
            r"C:\Program Files (x86)\Steam\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe",
            r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe",
            r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe",
            r"C:\Riot Games\VALORANT\live\VALORANT.exe",
            r"C:\XboxGames\Forza Horizon 5\Content\ForzaHorizon5.exe",
            "c:/gog games/witcher 3/bin/x64/witcher3.exe",
            // Any Steam library, or no launcher at all.
            r"E:\Games\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe",
            r"D:\CS2\game\bin\win64\cs2.exe",
            r"C:\Program Files (x86)\Overwatch\_retail_\Overwatch.exe",
            r"C:\Users\a\AppData\Local\Roblox\Versions\version-1\RobloxPlayerBeta.exe",
            r"C:\Program Files\Rockstar Games\Grand Theft Auto V\GTA5.exe",
            r"D:\Games\Some Indie Game\game.exe",
        ] {
            assert!(in_game_library(p), "{p}");
        }
    }

    #[test]
    fn the_pack_parses_and_checks_its_entries() {
        let p = GamePack::parse("libraries = ['/My Games/']").unwrap();
        assert!(p.is_game_path(r"D:\my games\x\x.exe"));
        assert!(GamePack::parse("libraries = []").is_err());
        assert!(GamePack::parse("libraries = ['steamapps']").is_err());
        assert!(GamePack::parse(r"libraries = ['\\']").is_err());
        assert!(GamePack::parse("libraries = ['/a/']\nother = 1").is_err());
        assert!(GamePack::parse("libraries = ['/a/']\nexes = ['x']").is_err());
        assert!(GamePack::parse("libraries = ['/a/']\nexes = ['a/x.exe']").is_err());
        let p = GamePack::parse("libraries = ['/a/']\nexes = ['Game.EXE']").unwrap();
        assert!(p.is_game_path(r"E:\Anywhere\game.exe"));
        assert!(!p.is_game_path(r"E:\Anywhere\game2.exe"));
        assert!(GamePack::builtin().libraries.len() >= 10);
    }

    #[test]
    fn launchers_and_other_programs_are_not() {
        for p in [
            r"C:\Program Files (x86)\Steam\steam.exe",
            r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"C:\Riot Games\Riot Client\RiotClientServices.exe",
            r"C:\Program Files\Rockstar Games\Launcher\Launcher.exe",
            r"C:\Program Files (x86)\Battle.net\Battle.net.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        ] {
            assert!(!in_game_library(p), "{p}");
        }
    }
}
