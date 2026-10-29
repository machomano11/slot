//! Cloud sync: keeps `Saves` and `States` in step with a WebDAV folder named
//! in [`ACCOUNT_FILE`], in the background: at start-up, as soon as a game is
//! closed, and every few minutes. While a game runs, its own save and states
//! are only ever sent, never replaced; the next run after it closes brings in
//! anything newer from the server.
//!
//! The ROMs in `Games` and their art in `Labels` go only when asked, from
//! HOME > Cloud Sync, which shows the run's progress through a [`Panel`].
//! Games that arrive join the game list without a restart. A game deleted
//! from the server (on another device, say) isn't deleted here, nor sent
//! back: the panel lists it and asks whether to delete it here too, or keep
//! it on this device only. A game deleted here comes back from the server.
//!
//! The panel also picks which systems this device syncs: saves, states,
//! games and art of the others stay on the server. The choice is kept with
//! this device's id in [`DEVICE_FILE`], and each run checks in on the server
//! with it (see [`slot_cloud::devices`]).
//!
//! Nothing happens without that file, so a card without it behaves exactly
//! as before.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slot_cloud::devices::{self, Device};
use slot_cloud::{Account, Answer, Card, Error, Report, Tally, GAMES};
use slot_store::{ini, Platform};

use crate::app::App;

/// `url` of the WebDAV folder, plus `username` and `password`; `roms=0` and
/// `art=0` leave those out.
pub const ACCOUNT_FILE: &str = "Config/cloudsync.ini";
/// This device's `id`, and the `systems` it syncs (all it plays when missing).
pub const DEVICE_FILE: &str = "Config/cloudsync-device.ini";
const MANIFEST: &str = "Config/cloudsync.manifest";
const EVERY: Duration = Duration::from_secs(10 * 60);
const RETRY: Duration = Duration::from_secs(60);
const TITLE: &str = "Cloud sync";

/// What HOME > Cloud Sync sees of sync, and how it asks for a full run.
#[derive(Debug, Default)]
pub struct Panel {
    /// The running job's progress.
    pub tally: Tally,
    asked: AtomicBool,
    stop: AtomicBool,
    refresh: AtomicBool,
    /// Systems switched on or off on the screen, waiting to be saved.
    flips: Mutex<Vec<String>>,
    /// What to do with games deleted from the server, for the next full run.
    answers: Mutex<BTreeMap<String, Answer>>,
    shown: Mutex<Shown>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    /// A run is going; true when it is a full one.
    pub running: Option<bool>,
    /// A full run was asked for and waits for the running one.
    pub waiting: bool,
    /// How the last full run ended.
    pub last: Option<String>,
    /// Why the last run couldn't reach the server, if it couldn't.
    pub offline: Option<String>,
    /// This device, with the systems it syncs.
    pub me: Device,
    /// The other devices on the server, as of the last run.
    pub devices: Vec<Device>,
    /// Games deleted from the server and still on the card, as of the last
    /// full run, waiting for an answer.
    pub gone: Vec<String>,
}

impl Shown {
    pub fn syncs(&self, system: &str) -> bool {
        self.me.systems.iter().any(|s| s == system)
    }
}

impl Panel {
    /// Asks for a full run: saves, states, games and art.
    pub fn sync_all(&self) {
        self.stop.store(false, Ordering::Relaxed);
        self.asked.store(true, Ordering::Relaxed);
    }

    /// Stops a full run, between files or mid-file.
    pub fn stop(&self) {
        self.asked.store(false, Ordering::Relaxed);
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Switches syncing `system` (a platform's folder name) on or off.
    pub fn flip(&self, system: &str) {
        self.show(|s| match s.me.systems.iter().position(|x| x == system) {
            Some(i) => {
                s.me.systems.remove(i);
            }
            None => s.me.systems.push(system.to_string()),
        });
        if let Ok(mut f) = self.flips.lock() {
            f.push(system.to_string());
        }
    }

    /// Answers for games deleted from the server, and a full run to carry
    /// them out.
    pub fn answer(&self, answers: impl IntoIterator<Item = (String, Answer)>) {
        if let Ok(mut a) = self.answers.lock() {
            a.extend(answers);
        }
        self.show(|s| s.gone.clear());
        self.sync_all();
    }

    /// Asks for a quick run, to see which devices are on the server.
    pub fn refresh(&self) {
        self.refresh.store(true, Ordering::Relaxed);
    }

    pub fn shown(&self) -> Shown {
        let mut s = self.shown.lock().map(|s| s.clone()).unwrap_or_default();
        s.waiting = self.asked.load(Ordering::Relaxed);
        s
    }

    pub(crate) fn show(&self, f: impl FnOnce(&mut Shown)) {
        if let Ok(mut s) = self.shown.lock() {
            f(&mut s);
        }
    }
}

pub struct CloudSync {
    root: PathBuf,
    panel: Arc<Panel>,
    account: Account,
    me: Device,
    job: Option<Job>,
    next: Option<Instant>,
    /// The running game: its stem and platform folder.
    game: Option<(String, &'static str)>,
    failing: bool,
    /// Games came down and the game list hasn't been read again yet.
    rescan: bool,
    /// How many files the last run had no room for, so a full card is said once.
    no_room: usize,
    /// The same for files that failed, so a file the server keeps refusing is said once.
    failed: usize,
}

/// A run's result, and the devices it found on the server.
type Finished = (Result<Report, Error>, Option<Vec<Device>>);

struct Job {
    /// Games and art too, asked for from the panel.
    full: bool,
    cancel: Arc<AtomicBool>,
    done: Receiver<Finished>,
    /// The game whose files this run leaves alone, if any.
    held: Option<(String, &'static str)>,
    /// The deleted games this run was told what to do with.
    answered: Vec<String>,
}

impl CloudSync {
    pub fn boot(root: &Path) -> Option<CloudSync> {
        let account = Account::from_ini(&ini::read(root, ACCOUNT_FILE))?;
        let me = this_device(root);
        eprintln!(
            "slot: cloud: syncing {} with {} as {}",
            me.systems.join(","),
            account.url,
            me.id
        );
        let panel: Arc<Panel> = Arc::default();
        panel.show(|s| s.me = me.clone());
        Some(CloudSync {
            root: root.to_path_buf(),
            panel,
            account,
            me,
            job: None,
            next: None,
            game: None,
            failing: false,
            rescan: false,
            no_room: 0,
            failed: 0,
        })
    }

    /// For HOME > Cloud Sync.
    pub fn panel(&self) -> Arc<Panel> {
        self.panel.clone()
    }

    /// A game is starting. A run that might replace its files stops between
    /// files; the game's own changes go out on the next timed run.
    pub fn game_started(&mut self, stem: &str, platform: Platform) {
        let game = (stem.to_string(), platform.dir_name());
        if let Some(job) = &self.job {
            if job.held.as_ref() != Some(&game) {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }
        self.game = Some(game);
        self.next = Some(Instant::now() + EVERY);
    }

    /// Call every frame with whether a game is still running.
    pub fn update(&mut self, playing: bool, app: &mut App) {
        if !playing && self.game.take().is_some() {
            // The game's save was just written: send it now.
            self.next = None;
        }
        self.poll(app);
        let flips = self
            .panel
            .flips
            .lock()
            .map(|mut f| std::mem::take(&mut *f))
            .unwrap_or_default();
        if !flips.is_empty() {
            self.flip(&flips);
        }
        if let Some(job) = &self.job {
            if job.full && self.panel.stop.swap(false, Ordering::Relaxed) {
                job.cancel.store(true, Ordering::Relaxed);
            }
        } else if self.panel.refresh.swap(false, Ordering::Relaxed) {
            self.next = None;
        }
        if self.job.is_none() && !playing && self.panel.asked.swap(false, Ordering::Relaxed) {
            self.panel.stop.store(false, Ordering::Relaxed);
            self.start(true);
        }
        if self.job.is_none() && self.next.is_none_or(|t| Instant::now() >= t) {
            self.start(false);
        }
        app.set_cloud_busy(self.job.as_ref().map(|j| j.full));
        if self.rescan && !playing {
            self.rescan = !app.rescan();
        }
    }

    /// Saves the systems switched on or off; the next run uses them.
    fn flip(&mut self, systems: &[String]) {
        for system in systems {
            match self.me.systems.iter().position(|x| x == system) {
                Some(i) => {
                    self.me.systems.remove(i);
                }
                None => self.me.systems.push(system.clone()),
            }
        }
        self.me.systems = plays()
            .filter(|p| self.me.systems.iter().any(|x| x == p))
            .map(str::to_string)
            .collect();
        let saved = ini::write(
            &self.root,
            DEVICE_FILE,
            "systems",
            &self.me.systems.join(","),
        );
        if let Err(e) = saved {
            eprintln!("slot: cloud: saving {DEVICE_FILE}: {e}");
        }
        eprintln!("slot: cloud: now syncing {}", self.me.systems.join(","));
        let me = self.me.clone();
        self.panel.show(|s| s.me = me);
    }

    fn start(&mut self, full: bool) {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, done) = mpsc::channel();
        let root = self.root.clone();
        let mut top = self.account.remote();
        let mut remote = self.account.remote();
        let mut me = self.me.clone();
        me.seen = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let manifest = root.join(MANIFEST);
        let stop = cancel.clone();
        let held = self.game.clone();
        let game = held.clone();
        let folders = library(
            &self.account.folders(!full || held.is_some()),
            &self.me.systems,
        );
        let panel = self.panel.clone();
        let answers = self
            .panel
            .answers
            .lock()
            .map(|a| a.clone())
            .unwrap_or_default();
        let answered = answers.keys().cloned().collect();
        let spawned = std::thread::Builder::new()
            .name("slot-cloud".into())
            .spawn(move || {
                let holding = |rel: &str| {
                    game.as_ref()
                        .is_some_and(|(stem, dir)| slot_cloud::belongs_to(rel, dir, stem))
                };
                let seen = devices::check_in(&mut top, &me)
                    .map_err(|e| eprintln!("slot: cloud: checking in: {e}"))
                    .ok();
                let folders: Vec<&str> = folders.iter().map(String::as_str).collect();
                let result = slot_cloud::sync_asking(
                    &Card(&root),
                    &manifest,
                    &mut remote,
                    &folders,
                    &stop,
                    &holding,
                    &panel.tally,
                    &answers,
                );
                let _ = tx.send((result, seen));
            });
        match spawned {
            Ok(_) => {
                self.panel.show(|s| s.running = Some(full));
                self.job = Some(Job {
                    full,
                    cancel,
                    done,
                    held,
                    answered,
                })
            }
            Err(e) => {
                eprintln!("slot: cloud: no thread: {e}");
                self.next = Some(Instant::now() + EVERY);
            }
        }
    }

    fn poll(&mut self, app: &mut App) {
        let Some(job) = &self.job else {
            return;
        };
        let (result, seen) = match job.done.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                (Err(Error::new("the sync stopped unexpectedly")), None)
            }
        };
        if let Some(mut all) = seen {
            all.retain(|d| d.id != self.me.id);
            all.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
            self.panel.show(|s| s.devices = all);
        }
        let full = job.full;
        let games = full && job.held.is_none() && self.account.folders(false).contains(&GAMES);
        let answered = job.answered.clone();
        self.job = None;
        self.panel.show(|s| s.running = None);
        let offline = match &result {
            Err(e) if e.offline => Some(unreachable(e)),
            _ => None,
        };
        self.panel.show(|s| s.offline = offline.clone());
        if full {
            let last = match &result {
                Ok(r) if r.cancelled => "Stopped".to_string(),
                Ok(r) => ended(r),
                Err(_) if offline.is_some() => offline.unwrap_or_default(),
                Err(e) => e.message.clone(),
            };
            self.panel.show(|s| s.last = Some(last));
        }
        match result {
            Ok(mut report) => {
                self.failing = false;
                if games && !report.cancelled {
                    // Whatever was answered has now been done.
                    if let Ok(mut a) = self.panel.answers.lock() {
                        a.retain(|rel, _| !answered.contains(rel));
                    }
                    let gone = report.gone.clone();
                    self.panel.show(|s| s.gone = gone);
                }
                // A run cut short by a game goes again as soon as it ends.
                self.next = (!report.cancelled).then(|| Instant::now() + EVERY);
                if app.take_in_synced(&report.received) {
                    self.rescan = true;
                }
                if std::mem::replace(&mut self.no_room, report.no_room.len())
                    == report.no_room.len()
                {
                    report.no_room.clear();
                }
                if std::mem::replace(&mut self.failed, report.failed.len()) == report.failed.len() {
                    report.failed.clear();
                }
                if let Some((title, detail)) = summary(&report) {
                    app.show_message(title, detail);
                }
            }
            Err(e) => {
                eprintln!("slot: cloud: {e}");
                self.next = Some(Instant::now() + RETRY);
                // Offline is usual (Wi-Fi off or still connecting): only a
                // server that answers and refuses is worth a banner.
                if !self.failing && !e.offline {
                    self.failing = true;
                    app.show_message("Cloud sync failed", e.message);
                }
            }
        }
    }
}

impl Drop for CloudSync {
    fn drop(&mut self) {
        if let Some(job) = &self.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// The folders a run covers: each of `folders` for each of `systems`, so
/// another system's files (a DS's, say) stay on the server.
fn library(folders: &[&'static str], systems: &[String]) -> Vec<String> {
    folders
        .iter()
        .flat_map(|f| systems.iter().map(move |p| format!("{f}/{p}")))
        .collect()
}

/// The systems this build plays, by folder name.
fn plays() -> impl Iterator<Item = &'static str> {
    Platform::ALL.iter().map(|p| p.dir_name())
}

/// This device as [`DEVICE_FILE`] has it, given an id the first time.
fn this_device(root: &Path) -> Device {
    let saved = ini::read(root, DEVICE_FILE);
    let id = saved
        .get("id")
        .map(|s| s.trim())
        .filter(|id| devices::id_ok(id));
    let id = match id {
        Some(id) => id.to_string(),
        None => {
            let id = format!("{}-{:04x}", MODEL.0, random16());
            if let Err(e) = ini::write(root, DEVICE_FILE, "id", &id) {
                eprintln!("slot: cloud: saving {DEVICE_FILE}: {e}");
            }
            id
        }
    };
    // Only systems this build plays; every one of them until some are switched off.
    let systems = match saved.get("systems") {
        Some(v) => {
            let picked = devices::systems(v);
            plays()
                .filter(|p| picked.iter().any(|x| x == p))
                .map(str::to_string)
                .collect()
        }
        None => plays().map(str::to_string).collect(),
    };
    Device {
        id,
        name: MODEL.1.into(),
        systems,
        seen: 0,
    }
}

/// This build's device: the start of its id, and its name.
const MODEL: (&str, &str) = if cfg!(feature = "brick") {
    ("brick", "TrimUI Brick")
} else if cfg!(feature = "device") {
    ("rgsp", "Anbernic RG SP")
} else {
    ("computer", "Slot on a computer")
};

fn random16() -> u16 {
    use std::io::Read;
    let mut b = [0u8; 2];
    match std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)) {
        Ok(()) => u16::from_le_bytes(b),
        Err(_) => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos() as u16),
    }
}

/// Why the server can't be reached, in a few words.
fn unreachable(e: &Error) -> String {
    let m = e.message.to_ascii_lowercase();
    if m.contains("dns") || m.contains("name resolution") || m.contains("unreachable") {
        "No connection: check Wi-Fi".into()
    } else if m.contains("timed out") || m.contains("timeout") {
        "The server didn't answer".into()
    } else {
        "Can't reach the server".into()
    }
}

/// How a full run ended, for the panel.
fn ended(r: &Report) -> String {
    let mut out = match (r.uploaded, r.downloaded) {
        (0, 0) => "Everything was already in sync".to_string(),
        (up, down) => format!("{up} sent, {down} received"),
    };
    if !r.failed.is_empty() {
        out += &format!(", {} couldn't be copied", r.failed.len());
    }
    if !r.no_room.is_empty() {
        out += &format!(", {} didn't fit on the card", r.no_room.len());
    }
    if r.deleted > 0 {
        out += &format!(", {} deleted", r.deleted);
    }
    out
}

/// The banner for a finished run, if it did anything worth saying.
fn summary(r: &Report) -> Option<(String, String)> {
    eprintln!(
        "slot: cloud: {} up, {} down, {} deleted, {} conflicts{}",
        r.uploaded,
        r.downloaded,
        r.deleted,
        r.conflicts.len(),
        if r.cancelled { ", cut short" } else { "" }
    );
    if !r.gone.is_empty() {
        let detail = match r.gone.len() {
            1 => "A game was deleted from the cloud: see HOME > Cloud Sync".to_string(),
            n => format!("{n} games were deleted from the cloud: see HOME > Cloud Sync"),
        };
        return Some((TITLE.into(), detail));
    }
    if let Some((rel, why)) = r.failed.first() {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        let detail = match r.failed.len() {
            1 => format!("Couldn't sync {name}: {why}"),
            n => format!("Couldn't sync {n} files, {name} among them: {why}"),
        };
        return Some((TITLE.into(), detail));
    }
    if !r.no_room.is_empty() {
        let detail = match r.no_room.len() {
            1 => "The card is full: 1 file wasn't received".to_string(),
            n => format!("The card is full: {n} files weren't received"),
        };
        return Some((TITLE.into(), detail));
    }
    if let Some(first) = r.conflicts.first() {
        let name = first.rsplit('/').next().unwrap_or(first);
        let detail = match r.conflicts.len() {
            1 => format!("{name} changed on both sides; kept both"),
            n => format!("{n} files changed on both sides; kept both"),
        };
        return Some((TITLE.into(), detail));
    }
    let files = |n: usize| {
        if n == 1 {
            "1 file".to_string()
        } else {
            format!("{n} files")
        }
    };
    let detail = match (r.uploaded, r.downloaded) {
        (0, 0) => return None,
        (up, 0) => format!("Sent {}", files(up)),
        (0, down) => format!("Received {}", files(down)),
        (up, down) => format!("Sent {}, received {down}", files(up)),
    };
    Some((TITLE.into(), detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_switched_off_and_on_again_is_synced_again() {
        let panel = Panel::default();
        panel.show(|s| s.me.systems = vec!["GBA".into(), "GB".into()]);
        panel.flip("GB");
        assert!(!panel.shown().syncs("GB"));
        panel.flip("N64");
        panel.flip("GB");
        assert!(panel.shown().syncs("GB"));
        assert_eq!(*panel.flips.lock().unwrap(), ["GB", "N64", "GB"]);
    }

    #[test]
    fn a_run_covers_each_folder_for_each_system() {
        let f = library(&["Saves", "Games"], &["GBA".into(), "PS".into()]);
        assert_eq!(f, ["Saves/GBA", "Saves/PS", "Games/GBA", "Games/PS"]);
    }

    #[test]
    fn no_connection_says_so_plainly() {
        let dns = Error::offline("listing : https://x/: Dns Failed: resolve dns name 'x:443'");
        assert_eq!(unreachable(&dns), "No connection: check Wi-Fi");
        assert_eq!(
            unreachable(&Error::offline("connection refused")),
            "Can't reach the server"
        );
    }

    #[test]
    fn a_device_keeps_its_id() {
        let d = tempfile::tempdir().unwrap();
        let first = this_device(d.path());
        assert!(first.id.starts_with(&format!("{}-", MODEL.0)), "{first:?}");
        assert_eq!(this_device(d.path()).id, first.id);
        assert_eq!(
            first.systems.len(),
            Platform::ALL.len(),
            "everything at first"
        );
        ini::write(d.path(), DEVICE_FILE, "systems", "GBA,N64, PS").unwrap();
        assert_eq!(
            this_device(d.path()).systems,
            ["GBA", "PS"],
            "only what it plays"
        );
        ini::write(d.path(), DEVICE_FILE, "systems", "").unwrap();
        assert!(this_device(d.path()).systems.is_empty());
    }

    #[test]
    fn quiet_runs_say_nothing() {
        let mut r = Report {
            deleted: 3,
            ..Report::default()
        };
        assert_eq!(summary(&r), None);
        r.uploaded = 2;
        r.downloaded = 1;
        assert_eq!(
            summary(&r),
            Some((TITLE.into(), "Sent 2 files, received 1".into()))
        );
        r.conflicts = vec!["Saves/GBA/Zelda.sav".into()];
        assert_eq!(
            summary(&r).unwrap().1,
            "Zelda.sav changed on both sides; kept both"
        );
    }
}
