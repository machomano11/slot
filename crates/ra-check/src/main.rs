//! ra-check: which games in a folder RetroAchievements recognises, and for the ones it
//! doesn't, whether a different version of the ROM would be.
//!
//! Each file is hashed by rcheevos itself (the copy slot builds), so the hash is the one an
//! emulator would look the game up by. The hashes are then checked against
//! RetroAchievements' lists, fetched with the user's web API key.
mod names;
mod ra;
mod scan;
mod systems;

use std::collections::HashMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use systems::System;

const USAGE: &str = "\
ra-check: which of your games RetroAchievements supports

Usage: ra-check [options] <folder or file>...

Point it at your Games folder (with GBA, SNES, PS... folders inside), at one system's
folder, or at single files. Systems are told apart by folder name, or by file extension
when that is enough.

Options:
  --key <key>       Your RetroAchievements web API key (retroachievements.org/settings,
                    \"Keys\"). Saved for next time. Or set RA_API_KEY.
  --system <name>   Treat every file as this system (GBA, GB, GBC, NES, SNES, MD, GG,
                    SMS, NGP, WS, SEGACD, 32X, PS, N64, DS).
  --only-problems   List only the games that are not supported.
  --csv <file>      Also write the results as a spreadsheet (opens in Numbers or Excel).
  --refresh         Fetch RetroAchievements' lists again now (they are kept for 3 days).
  --hashes          Only print each file's hash; no key or network needed.
  --forget-key      Delete the saved key.
  -h, --help        This text.
";

struct Options {
    paths: Vec<PathBuf>,
    key: Option<String>,
    system: Option<&'static System>,
    only_problems: bool,
    csv: Option<PathBuf>,
    refresh: bool,
    hashes_only: bool,
}

fn parse() -> Result<Options, String> {
    let mut options = Options {
        paths: Vec::new(),
        key: None,
        system: None,
        only_problems: false,
        csv: None,
        refresh: false,
        hashes_only: false,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.to_string_lossy().as_ref() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!(
                    "ra-check {} (rcheevos {})",
                    env!("CARGO_PKG_VERSION"),
                    slot_cheevos::version()
                );
                std::process::exit(0);
            }
            "--key" => options.key = Some(value("--key")?.to_string_lossy().trim().to_string()),
            "--system" => {
                let name = value("--system")?.to_string_lossy().into_owned();
                options.system =
                    Some(systems::by_name(&name).ok_or(format!("unknown system \"{name}\""))?);
            }
            "--only-problems" => options.only_problems = true,
            "--csv" => options.csv = Some(value("--csv")?.into()),
            "--refresh" => options.refresh = true,
            "--hashes" => options.hashes_only = true,
            "--forget-key" => {
                let _ = std::fs::remove_file(ra::key_file());
                println!("Forgot the saved key.");
                std::process::exit(0);
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown option {other}"))
            }
            _ => options.paths.push(arg.into()),
        }
    }
    if options.paths.is_empty() {
        return Err("tell me which folder to check".into());
    }
    for path in &options.paths {
        if !path.exists() {
            return Err(format!("{} doesn't exist", path.display()));
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("ra-check: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ra-check: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What RetroAchievements makes of one file.
enum Verdict {
    Supported(usize),
    /// RetroAchievements knows the file, but the game has no achievements yet.
    NoSet(usize),
    /// The hash is not one RetroAchievements takes, but the title is a game it has.
    WrongVersion(usize, Vec<ra::Supported>),
    Unknown,
    Failed(String),
}

struct Checked {
    game: scan::Game,
    hash: Result<String, String>,
    verdict: Verdict,
}

fn run(options: Options) -> Result<(), String> {
    let (games, unknown) = scan::find(&options.paths, options.system);
    if games.is_empty() {
        return Err(
            "found no games there. Point me at your Games folder, or a system's \
             folder inside it."
                .into(),
        );
    }
    let hashes = hash_all(&games);
    if options.hashes_only {
        for (game, hash) in games.iter().zip(&hashes) {
            match hash {
                Ok(h) => println!("{}\t{h}\t{}", game.system.name, game.path.display()),
                Err(e) => println!("{}\t-\t{}\t({e})", game.system.name, game.path.display()),
            }
        }
        return Ok(());
    }
    let client = ra::Client::new(api_key(options.key)?, options.refresh);

    // One list per console present, fetched once.
    let mut lists: HashMap<u32, Catalogue> = HashMap::new();
    for console in games.iter().map(|g| g.system.console) {
        if let std::collections::hash_map::Entry::Vacant(entry) = lists.entry(console) {
            eprint!("Fetching RetroAchievements' {} list...\r", name_of(console));
            entry.insert(Catalogue::new(client.games(console)?));
        }
    }
    clear_line();

    let mut checked = Vec::new();
    for (game, hash) in games.into_iter().zip(hashes) {
        let catalogue = &lists[&game.system.console];
        let verdict = match &hash {
            Err(e) => Verdict::Failed(e.clone()),
            Ok(h) => match catalogue.by_hash.get(h) {
                Some(&i) if catalogue.games[i].achievements > 0 => Verdict::Supported(i),
                Some(&i) => Verdict::NoSet(i),
                None => match catalogue.by_title(&game.path) {
                    Some(i) if catalogue.games[i].achievements > 0 => {
                        let supported =
                            client.supported(catalogue.games[i].id).unwrap_or_else(|e| {
                                eprintln!("ra-check: {e}");
                                Vec::new()
                            });
                        Verdict::WrongVersion(i, supported)
                    }
                    Some(i) => Verdict::NoSet(i),
                    None => Verdict::Unknown,
                },
            },
        };
        checked.push(Checked {
            game,
            hash,
            verdict,
        });
    }

    report(&checked, &lists, &unknown, options.only_problems);
    if let Some(path) = &options.csv {
        write_csv(path, &checked, &lists).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("\nWrote {}", path.display());
    }
    Ok(())
}

fn hash_all(games: &[scan::Game]) -> Vec<Result<String, String>> {
    let progress = std::io::stderr().is_terminal();
    let mut out = Vec::with_capacity(games.len());
    for (i, game) in games.iter().enumerate() {
        if progress {
            eprint!(
                "\x1b[2KHashing {}/{}: {}\r",
                i + 1,
                games.len(),
                file_name(&game.path)
            );
        }
        out.push(scan::hash(game));
    }
    clear_line();
    out
}

fn clear_line() {
    if std::io::stderr().is_terminal() {
        eprint!("\x1b[2K");
    }
}

/// The key from --key (saved for next time), RA_API_KEY, the saved one, or asked for.
fn api_key(given: Option<String>) -> Result<String, String> {
    let file = ra::key_file();
    if let Some(key) = given.filter(|k| !k.is_empty()) {
        save_key(&file, &key);
        return Ok(key);
    }
    if let Some(key) = std::env::var("RA_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
    {
        return Ok(key.trim().to_string());
    }
    if let Some(key) = std::fs::read_to_string(&file)
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
    {
        return Ok(key);
    }
    if !std::io::stdin().is_terminal() {
        return Err(
            "I need your RetroAchievements web API key: run with --key <key>. \
             It's under \"Keys\" at retroachievements.org/settings."
                .into(),
        );
    }
    eprintln!(
        "I need your RetroAchievements web API key, once. Find it under \"Keys\" at\n\
         https://retroachievements.org/settings, then paste it here and press Return."
    );
    eprint!("Web API key: ");
    let _ = std::io::stderr().flush();
    let mut key = String::new();
    std::io::stdin()
        .read_line(&mut key)
        .map_err(|e| e.to_string())?;
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("no key given".into());
    }
    save_key(&file, &key);
    Ok(key)
}

fn save_key(file: &Path, key: &str) {
    let saved = file
        .parent()
        .map(std::fs::create_dir_all)
        .unwrap_or(Ok(()))
        .and_then(|_| std::fs::write(file, key));
    match saved {
        Ok(()) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600));
            }
            eprintln!(
                "Saved the key in {} (ra-check --forget-key deletes it).",
                file.display()
            );
        }
        Err(e) => eprintln!("ra-check: couldn't save the key ({e}); pass --key next time."),
    }
}

/// One console's games, indexed by hash and by title.
struct Catalogue {
    games: Vec<ra::Game>,
    by_hash: HashMap<String, usize>,
    titles: HashMap<String, Vec<usize>>,
}

impl Catalogue {
    fn new(games: Vec<ra::Game>) -> Self {
        let mut by_hash = HashMap::new();
        let mut titles: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, game) in games.iter().enumerate() {
            for hash in &game.hashes {
                by_hash.insert(hash.clone(), i);
            }
            for key in names::title_keys(&game.title) {
                titles.entry(key).or_default().push(i);
            }
        }
        Catalogue {
            games,
            by_hash,
            titles,
        }
    }

    /// The game the file's name says it is: of games sharing the title, the one with the
    /// most achievements (the main release over a "~Hack~" of the same name).
    fn by_title(&self, path: &Path) -> Option<usize> {
        let stem = path.file_stem()?.to_string_lossy();
        let found = self.titles.get(&names::key(&stem))?;
        found.iter().copied().max_by_key(|&i| {
            let game = &self.games[i];
            (!game.title.starts_with('~'), game.achievements)
        })
    }
}

fn name_of(console: u32) -> &'static str {
    systems::SYSTEMS
        .iter()
        .find(|s| s.console == console)
        .map_or("?", |s| s.name)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

struct Paint {
    on: bool,
}

impl Paint {
    fn new() -> Self {
        Paint {
            on: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }
    fn wrap(&self, code: &str, text: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }
    fn green(&self, t: &str) -> String {
        self.wrap("32", t)
    }
    fn yellow(&self, t: &str) -> String {
        self.wrap("33", t)
    }
    fn red(&self, t: &str) -> String {
        self.wrap("31", t)
    }
    fn dim(&self, t: &str) -> String {
        self.wrap("2", t)
    }
    fn bold(&self, t: &str) -> String {
        self.wrap("1", t)
    }
}

/// How many supported files to list for a game in the wrong version.
const LIST: usize = 6;

fn report(
    checked: &[Checked],
    lists: &HashMap<u32, Catalogue>,
    unknown: &[scan::Unknown],
    only_problems: bool,
) {
    let paint = Paint::new();
    let mut totals = [0usize; 5];
    let mut system = "";
    for c in checked {
        let games = &lists[&c.game.system.console].games;
        let slot = match c.verdict {
            Verdict::Supported(_) => 0,
            Verdict::WrongVersion(..) => 1,
            Verdict::NoSet(_) => 2,
            Verdict::Unknown => 3,
            Verdict::Failed(_) => 4,
        };
        totals[slot] += 1;
        if only_problems && slot == 0 {
            continue;
        }
        if c.game.system.name != system {
            system = c.game.system.name;
            println!("\n{}", paint.bold(system));
        }
        let file = file_name(&c.game.path);
        match &c.verdict {
            Verdict::Supported(i) => {
                let g = &games[*i];
                println!(
                    "  {} {file}  {}",
                    paint.green("✓"),
                    paint.dim(&format!("{} · {} achievements", g.title, g.achievements))
                );
            }
            Verdict::WrongVersion(i, supported) => {
                let g = &games[*i];
                println!(
                    "  {} {file}  {}",
                    paint.red("✗"),
                    paint.yellow(&format!(
                        "different version needed for {} ({} achievements)",
                        g.title, g.achievements
                    ))
                );
                let mut listed: Vec<&ra::Supported> = supported.iter().collect();
                // Files that work as they are, before ones needing a patch.
                listed.sort_by_key(|s| s.patch.is_some());
                for s in listed.iter().take(LIST) {
                    let patch = if s.patch.is_some() {
                        "  (after RetroAchievements' patch)"
                    } else {
                        ""
                    };
                    println!(
                        "      {} {}{}",
                        paint.dim("works:"),
                        s.name,
                        paint.dim(patch)
                    );
                }
                if listed.len() > LIST {
                    println!(
                        "      {}",
                        paint.dim(&format!("…and {} more", listed.len() - LIST))
                    );
                }
            }
            Verdict::NoSet(i) => println!(
                "  {} {file}  {}",
                paint.dim("·"),
                paint.dim(&format!("{} has no achievements yet", games[*i].title))
            ),
            Verdict::Unknown => println!(
                "  {} {file}  {}",
                paint.dim("?"),
                paint.dim(&format!(
                    "not on RetroAchievements (hash {})",
                    c.hash.as_deref().unwrap_or("-")
                ))
            ),
            Verdict::Failed(e) => println!(
                "  {} {file}  {}",
                paint.red("!"),
                paint.red(&format!("couldn't hash: {e}"))
            ),
        }
    }
    if !unknown.is_empty() {
        println!(
            "\n{}",
            paint.bold("Skipped: can't tell which system (put them in a folder named after it, or use --system)")
        );
        for u in unknown {
            println!("  {}", u.path.display());
        }
    }
    let [ok, wrong, no_set, missing, failed] = totals;
    println!(
        "\n{} {}   {} {}   {} {}   {} {}{}",
        paint.green("✓"),
        format_args!("{ok} supported"),
        paint.red("✗"),
        format_args!("{wrong} need a different version"),
        paint.dim("·"),
        format_args!("{no_set} without achievements yet"),
        paint.dim("?"),
        format_args!("{missing} not on RetroAchievements"),
        if failed > 0 {
            format!("   ! {failed} couldn't be read")
        } else {
            String::new()
        }
    );
}

fn write_csv(
    path: &Path,
    checked: &[Checked],
    lists: &HashMap<u32, Catalogue>,
) -> std::io::Result<()> {
    // The byte order mark tells Excel the file is UTF-8.
    let mut out = String::from(
        "\u{feff}System,File,Result,RetroAchievements game,Game ID,Achievements,Hash,Versions that work\n",
    );
    for c in checked {
        let games = &lists[&c.game.system.console].games;
        let (result, game, works) = match &c.verdict {
            Verdict::Supported(i) => ("supported", Some(&games[*i]), String::new()),
            Verdict::WrongVersion(i, s) => (
                "different version needed",
                Some(&games[*i]),
                s.iter()
                    .map(|s| {
                        if s.patch.is_some() {
                            format!("{} (patch)", s.name)
                        } else {
                            s.name.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
            Verdict::NoSet(i) => ("no achievements yet", Some(&games[*i]), String::new()),
            Verdict::Unknown => ("not on RetroAchievements", None, String::new()),
            Verdict::Failed(e) => ("couldn't hash", None, e.clone()),
        };
        let row = [
            c.game.system.name.to_string(),
            c.game.path.display().to_string(),
            result.to_string(),
            game.map(|g| g.title.clone()).unwrap_or_default(),
            game.map(|g| g.id.to_string()).unwrap_or_default(),
            game.map(|g| g.achievements.to_string()).unwrap_or_default(),
            c.hash.clone().unwrap_or_default(),
            works,
        ];
        let row: Vec<String> = row.iter().map(|f| csv_field(f)).collect();
        out.push_str(&row.join(","));
        out.push('\n');
    }
    std::fs::write(path, out)
}

fn csv_field(field: &str) -> String {
    if field.contains([',', '"', '\n']) {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}
