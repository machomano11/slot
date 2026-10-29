//! Finding the games under the folders given, and hashing each the way RetroAchievements does.
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::systems::{self, System};

/// One game file found.
pub struct Game {
    pub path: PathBuf,
    pub system: &'static System,
}

/// A file skipped because its system can't be told, to list at the end.
pub struct Unknown {
    pub path: PathBuf,
}

pub fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Every game under `roots`. `forced` puts every file in that system, whatever its folder.
pub fn find(roots: &[PathBuf], forced: Option<&'static System>) -> (Vec<Game>, Vec<Unknown>) {
    let mut games = Vec::new();
    let mut unknown = Vec::new();
    for root in roots {
        let inherited = forced.or_else(|| {
            // A system folder named directly ("Games/GBA") or a file inside one.
            let dir = if root.is_dir() {
                Some(root.as_path())
            } else {
                root.parent()
            };
            dir.and_then(|d| d.file_name())
                .and_then(|n| systems::by_folder(&n.to_string_lossy()))
        });
        if root.is_dir() {
            walk(root, inherited, forced.is_some(), &mut games, &mut unknown);
        } else {
            consider(root, inherited, &[], &mut games, &mut unknown);
        }
    }
    games.sort_by(|a, b| {
        (a.system.name, a.path.to_string_lossy().to_lowercase())
            .cmp(&(b.system.name, b.path.to_string_lossy().to_lowercase()))
    });
    (games, unknown)
}

fn walk(
    dir: &Path,
    system: Option<&'static System>,
    forced: bool,
    games: &mut Vec<Game>,
    unknown: &mut Vec<Unknown>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("ra-check: can't read {}", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| !hidden(p))
        .collect();
    paths.sort();
    let names: Vec<String> = paths
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        .collect();
    for path in &paths {
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let inner = if forced {
                system
            } else {
                systems::by_folder(&name).or(system)
            };
            walk(path, inner, forced, games, unknown);
        } else {
            consider(path, system, &names, games, unknown);
        }
    }
}

/// macOS' "._" resource files, ".Trashes" and the like.
fn hidden(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with('.'))
}

fn consider(
    path: &Path,
    system: Option<&'static System>,
    siblings: &[String],
    games: &mut Vec<Game>,
    unknown: &mut Vec<Unknown>,
) {
    let ext = extension(path);
    if ext == "zip" {
        // Which system is decided by what is inside, when the folder doesn't say.
        let system = system.or_else(|| {
            zip_entry(path, None)
                .and_then(|(name, _)| systems::by_extension(&extension(Path::new(&name))))
        });
        match system {
            Some(s) if !s.disc => games.push(Game {
                path: path.into(),
                system: s,
            }),
            Some(_) => {}
            None => unknown.push(Unknown { path: path.into() }),
        }
        return;
    }
    // A file whose extension names another system ("x.gb" among GBC games) is that system's.
    let system = match system {
        Some(s) if s.extensions.contains(&ext.as_str()) => s,
        Some(_) => match systems::by_extension(&ext) {
            Some(s) => s,
            None => return,
        },
        None => match systems::by_extension(&ext) {
            Some(s) => s,
            None => {
                if ["bin", "chd", "cue", "iso"].contains(&ext.as_str()) {
                    unknown.push(Unknown { path: path.into() });
                }
                return;
            }
        },
    };
    if system.disc && ext == "bin" {
        return;
    }
    // A .bin with a .cue beside it is a disc track, not a cartridge.
    if !system.disc && ext == "bin" && siblings.iter().any(|n| n.ends_with(".cue")) {
        return;
    }
    games.push(Game {
        path: path.into(),
        system,
    });
}

/// The name and contents of the game inside a zip: the first file with one of the
/// system's extensions, or with any known one when no system is given.
fn zip_entry(path: &Path, system: Option<&System>) -> Option<(String, Vec<u8>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut fallback = None;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index(i) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_string();
        let ext = extension(Path::new(&name));
        let fits = match system {
            Some(s) => s.extensions.contains(&ext.as_str()),
            None => systems::by_extension(&ext).is_some(),
        };
        if fits {
            fallback = Some(i);
            break;
        }
        if fallback.is_none() && !name.starts_with('.') {
            fallback = Some(i);
        }
    }
    if system.is_none() {
        let entry = archive.by_index(fallback?).ok()?;
        return Some((entry.name().rsplit('/').next()?.to_string(), Vec::new()));
    }
    let mut entry = archive.by_index(fallback?).ok()?;
    let name = entry.name().rsplit('/').next()?.to_string();
    let mut data = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut data).ok()?;
    Some((name, data))
}

/// The game's RetroAchievements hash, or why it has none.
pub fn hash(game: &Game) -> Result<String, String> {
    if extension(&game.path) == "zip" {
        let (name, data) = zip_entry(&game.path, Some(game.system))
            .ok_or("can't read the zip, or nothing in it is a game for this system")?;
        return slot_cheevos::hash_bytes(&name, &data, game.system.console);
    }
    slot_cheevos::hash_file(&game.path, game.system.console)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn finds_games_by_folder_and_extension() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        touch(&root.join("Games/GBA/a.gba"), b"x");
        touch(&root.join("Games/GBA/._a.gba"), b"x");
        touch(&root.join("Games/GBA/a.sav"), b"x");
        touch(&root.join("Games/MD/b.bin"), b"x");
        touch(&root.join("Games/PS/c.cue"), b"x");
        touch(&root.join("Games/PS/c.bin"), b"x");
        touch(&root.join("Games/PS/c.m3u"), b"x");
        touch(&root.join("loose/d.sfc"), b"x");
        touch(&root.join("loose/e.chd"), b"x");
        let (games, unknown) = find(&[root.to_path_buf()], None);
        let found: Vec<_> = games
            .iter()
            .map(|g| {
                (
                    g.system.name,
                    g.path.file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("GBA", "a.gba".to_string()),
                ("MD", "b.bin".to_string()),
                ("PS", "c.cue".to_string()),
                ("SNES", "d.sfc".to_string()),
            ]
        );
        assert_eq!(unknown.len(), 1);
    }

    #[test]
    fn a_system_folder_given_directly() {
        let dir = tempfile::tempdir().unwrap();
        let gba = dir.path().join("GBA");
        touch(&gba.join("a.gba"), b"x");
        let (games, _) = find(&[gba], None);
        assert_eq!(games.len(), 1);
    }

    #[test]
    fn hashes_a_rom_inside_a_zip_like_the_rom() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let rom: Vec<u8> = (0..4096u32).map(|i| (i * 7) as u8).collect();
        touch(&dir.path().join("GBA/a.gba"), &rom);
        let zip_path = dir.path().join("GBA/b.zip");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        zip.start_file("b.gba", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&rom).unwrap();
        zip.finish().unwrap();
        let (games, _) = find(&[dir.path().join("GBA")], None);
        let hashes: Vec<_> = games.iter().map(|g| hash(g).unwrap()).collect();
        assert_eq!(hashes.len(), 2);
        assert_eq!(hashes[0], hashes[1]);
    }
}
