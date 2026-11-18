//! Is this program a game? (PLAN.md §11.5 "Game detection".) The folder
//! test: an exe under a game launcher's library is a game. The app adds
//! the other test (exclusive fullscreen) from Windows itself.

/// Library folders of the game launchers, lowercase, `\`-separated.
const LIBRARIES: &[&str] = &[
    r"\steamapps\common\",
    r"\epic games\",
    r"\ea games\",
    r"\origin games\",
    r"\ubisoft game launcher\games\",
    r"\riot games\",
    r"\gog galaxy\games\",
    r"\gog games\",
    r"\xboxgames\",
    r"\battle.net\games\",
];

/// The launchers themselves live beside their games; they aren't games.
const LAUNCHERS: &[&str] = &[r"\epic games\launcher\", r"\riot games\riot client\"];

/// Whether the exe at `path` sits in a game launcher's library.
pub fn in_game_library(path: &str) -> bool {
    let p = path.replace('/', "\\").to_lowercase();
    LIBRARIES.iter().any(|l| p.contains(l)) && !LAUNCHERS.iter().any(|l| p.contains(l))
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
        ] {
            assert!(in_game_library(p), "{p}");
        }
    }

    #[test]
    fn launchers_and_other_programs_are_not() {
        for p in [
            r"C:\Program Files (x86)\Steam\steam.exe",
            r"C:\Program Files (x86)\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            r"C:\Riot Games\Riot Client\RiotClientServices.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        ] {
            assert!(!in_game_library(p), "{p}");
        }
    }
}
