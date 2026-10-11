//! Onion's SD card, read the way MainUI reads it.
//!
//! Paths are kept card-relative and spelled the way MainUI spells them
//! (`Emu/GBA/../../Roms/GBA/x.gba`), because Onion hashes that exact string to
//! name romScreens. `Card::root` is the host directory standing in for
//! `/mnt/SDCARD`, so the same code runs on the desktop against a copied card.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::hash::fnv1a_pippip_yurii;

pub const DEVICE_ROOT: &str = "/mnt/SDCARD";
const RECENTS: &str = "Roms/recentlist.json";
const ROM_SCREENS: &str = "Saves/CurrentProfile/romScreens";
const PADSP: &str = "/mnt/SDCARD/miyoo/app/../lib/libpadsp.so";
/// MainUI's recents entry type for a game.
const RECENT_GAME: i64 = 5;

pub struct Card {
    pub root: PathBuf,
}

pub struct System {
    /// Emu folder name, e.g. `GBA`; also the key for cart renderers.
    pub id: String,
    pub label: String,
    /// Card-relative launch script.
    pub launch: String,
    pub games: Vec<Game>,
}

pub struct Game {
    pub stem: String,
    /// Card-relative, MainUI spelling.
    pub rom: String,
    /// Card-relative box art, if present.
    pub img: Option<String>,
    /// Slot label art, falling back to Onion box art when absent.
    pub label: Option<String>,
    /// Optional per-game shell override from Slot's cart_shell config.
    pub shell: Option<slot_store::ShellChoice>,
}

impl Card {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Card { root: root.into() }
    }

    pub fn host(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn device(rel: &str) -> String {
        format!("{DEVICE_ROOT}/{rel}")
    }

    /// Every Emu folder with a readable config and at least one ROM.
    pub fn systems(&self) -> Vec<System> {
        let Ok(dir) = fs::read_dir(self.host("Emu")) else {
            return Vec::new();
        };
        let mut ids: Vec<String> = dir
            .flatten()
            .filter(|e| e.path().join("config.json").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        ids.sort();
        let shells = slot_store::cart_shell::layered(
            &fs::read_to_string(self.host("Config/cart_shell.txt")).unwrap_or_default(),
            &fs::read_to_string(self.host("Labels/cart_shell.txt")).unwrap_or_default(),
        );
        ids.into_iter()
            .filter_map(|id| self.system(&id, &shells))
            .filter(|s| !s.games.is_empty())
            .collect()
    }

    fn system(
        &self,
        id: &str,
        shells: &std::collections::HashMap<String, slot_store::ShellChoice>,
    ) -> Option<System> {
        let emu = format!("Emu/{id}");
        let text = fs::read_to_string(self.host(&format!("{emu}/config.json"))).ok()?;
        let config: Value = serde_json::from_str(&text).ok()?;
        let field = |k: &str| config.get(k).and_then(Value::as_str).map(str::to_owned);

        let rompath = format!("{emu}/{}", field("rompath")?);
        let imgpath = field("imgpath").map(|p| format!("{emu}/{p}"));
        let exts: Vec<String> = field("extlist")
            .unwrap_or_default()
            .split('|')
            .filter(|e| !e.is_empty())
            .map(str::to_ascii_lowercase)
            .collect();

        let mut games: Vec<Game> = fs::read_dir(self.host(&rompath))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|name| !name.starts_with('.'))
            .filter_map(|name| {
                let path = Path::new(&name);
                let ext = path.extension()?.to_str()?.to_ascii_lowercase();
                exts.contains(&ext).then(|| {
                    let stem = path.file_stem()?.to_str()?.to_owned();
                    let img = imgpath
                        .as_ref()
                        .map(|dir| format!("{dir}/{stem}.png"))
                        .filter(|p| self.host(p).is_file());
                    let label = find_label(&self.host(&format!("Labels/{id}")), &stem)
                        .and_then(|path| {
                            path.strip_prefix(&self.root)
                                .ok()
                                .map(|relative| relative.to_string_lossy().into_owned())
                        })
                        .or_else(|| img.clone());
                    Some(Game {
                        rom: format!("{rompath}/{name}"),
                        shell: shells.get(&slot_store::cart_shell::key(&stem)).copied(),
                        label,
                        stem,
                        img,
                    })
                })?
            })
            .collect();
        games.sort_by_key(|g| g.stem.to_lowercase());

        Some(System {
            id: id.to_owned(),
            label: field("label").unwrap_or_else(|| id.to_owned()),
            launch: format!(
                "{emu}/{}",
                field("launch").unwrap_or_else(|| "launch.sh".into())
            ),
            games,
        })
    }

    /// What MainUI writes to `/tmp/cmd_to_run.sh` for a game.
    pub fn launch_cmd(system: &System, game: &Game) -> String {
        format!(
            "LD_PRELOAD={PADSP} \"{}\" \"{}\"",
            Card::device(&system.launch),
            Card::device(&game.rom)
        )
    }

    /// Put the game at the top of MainUI's recents, as MainUI does on launch.
    /// keymon names the romScreen after the top entry, and GameSwitcher reads it.
    pub fn push_recent(&self, system: &System, game: &Game) -> io::Result<()> {
        let rompath = Card::device(&game.rom);
        let mut entry = serde_json::Map::new();
        entry.insert("label".into(), game.stem.clone().into());
        entry.insert("launch".into(), Card::device(&system.launch).into());
        entry.insert("type".into(), RECENT_GAME.into());
        if let Some(img) = &game.img {
            entry.insert("imgpath".into(), Card::device(img).into());
        }
        entry.insert("rompath".into(), rompath.clone().into());

        let path = self.host(RECENTS);
        let old = fs::read_to_string(&path).unwrap_or_default();
        let mut out = Value::Object(entry).to_string();
        out.push('\n');
        for line in old.lines().filter(|l| !l.trim().is_empty()) {
            if recent_rompath(line).as_deref() != Some(rompath.as_str()) {
                out.push_str(line);
                out.push('\n');
            }
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, out)
    }

    pub fn top_recent(&self) -> Option<String> {
        let text = fs::read_to_string(self.host(RECENTS)).ok()?;
        recent_rompath(text.lines().next()?)
    }

    /// keymon's screenshot of the game, keyed by the recents rompath spelling.
    pub fn romscreen(&self, device_rompath: &str) -> PathBuf {
        let hash = fnv1a_pippip_yurii(device_rompath.as_bytes());
        self.host(&format!("{ROM_SCREENS}/{hash}.png"))
    }
}

fn find_label(dir: &Path, stem: &str) -> Option<PathBuf> {
    let wanted = format!("{stem}.png");
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            (name.eq_ignore_ascii_case(&wanted) && entry.path().is_file()).then(|| entry.path())
        })
        .next()
}

fn recent_rompath(line: &str) -> Option<String> {
    let entry: Value = serde_json::from_str(line).ok()?;
    entry.get("rompath")?.as_str().map(str::to_owned)
}

/// Resolve `.` and `..` without touching the filesystem, so MainUI's
/// `Emu/GBA/../../Roms/GBA/x.gba` compares equal to runtime.sh's realpath.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let joined = parts.join("/");
    if path.starts_with('/') {
        format!("/{joined}")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_mainui_paths() {
        assert_eq!(
            normalize("/mnt/SDCARD/Emu/GBA/../../Roms/GBA/x.gba"),
            "/mnt/SDCARD/Roms/GBA/x.gba"
        );
    }

    #[test]
    fn launch_cmd_is_seen_as_a_game_by_runtime_sh() {
        let system = System {
            id: "GBA".into(),
            label: "GBA".into(),
            launch: "Emu/GBA/launch.sh".into(),
            games: Vec::new(),
        };
        let game = Game {
            stem: "x".into(),
            rom: "Emu/GBA/../../Roms/GBA/x.gba".into(),
            img: None,
            label: None,
            shell: None,
        };
        let cmd = Card::launch_cmd(&system, &game);
        assert_eq!(
            cmd,
            "LD_PRELOAD=/mnt/SDCARD/miyoo/app/../lib/libpadsp.so \
             \"/mnt/SDCARD/Emu/GBA/launch.sh\" \"/mnt/SDCARD/Emu/GBA/../../Roms/GBA/x.gba\""
        );
        // check_is_game in runtime.sh
        assert!(cmd.contains("/../../Roms/"));
    }
}
