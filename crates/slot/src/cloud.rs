//! Cloud sync: keeps `Saves` and `States` in step with a WebDAV folder named
//! in [`ACCOUNT_FILE`], in the background: at start-up, as soon as a game is
//! closed, and every few minutes. While a game runs, its own save and states
//! are only ever sent, never replaced; the next run after it closes brings in
//! anything newer from the server.
//!
//! The ROMs in `Games` and their art in `Labels` go only when asked, from
//! HOME > Cloud Sync, which shows the run's progress through a [`Panel`].
//! Games that arrive join the game list without a restart.
//!
//! The panel also picks which of the owner's devices this one shares with:
//! each device checks in on the server and syncs inside one space (see
//! [`slot_cloud::devices`]), kept with this device's id in [`DEVICE_FILE`].
//!
//! Nothing happens without that file, so a card without it behaves exactly
//! as before.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use slot_cloud::devices::{self, Device};
use slot_cloud::{Account, Error, Report, Tally};
use slot_store::{ini, Platform};

use crate::app::App;

/// `url` of the WebDAV folder, plus `username` and `password`; `roms=0` and
/// `art=0` leave those out.
pub const ACCOUNT_FILE: &str = "Config/cloudsync.ini";
/// This device's `id` and the `space` it syncs in.
pub const DEVICE_FILE: &str = "Config/cloudsync-device.ini";
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
    /// The space to move to, once nothing runs.
    join: Mutex<Option<String>>,
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
    /// This device.
    pub me: Device,
    /// The other devices on the server, as of the last run.
    pub devices: Vec<Device>,
}

impl Shown {
    /// Whether this device shares a space with `other`.
    pub fn shares_with(&self, other: &Device) -> bool {
        other.space == self.me.space
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

    /// Starts sharing with `other`, or stops if this device already does.
    pub fn toggle(&self, other: &Device) {
        let shown = self.shown();
        let to = match shown.shares_with(other) {
            true => shown.me.own_space(&shown.me.space),
            false => other.space.clone(),
        };
        if let Ok(mut j) = self.join.lock() {
            *j = Some(to);
        }
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

    fn show(&self, f: impl FnOnce(&mut Shown)) {
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
}

impl CloudSync {
    pub fn boot(root: &Path) -> Option<CloudSync> {
        let account = Account::from_ini(&ini::read(root, ACCOUNT_FILE))?;
        let me = this_device(root);
        eprintln!(
            "slot: cloud: syncing with {} as {} in {:?}",
            account.url, me.id, me.space
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
        let join = self.panel.join.lock().ok().and_then(|j| j.clone());
        if let Some(job) = &self.job {
            if join.is_some() || job.full && self.panel.stop.swap(false, Ordering::Relaxed) {
                job.cancel.store(true, Ordering::Relaxed);
            }
        } else if let Some(space) = join {
            self.move_to(&space);
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

    /// Syncs in `space` from now on; the next run starts straight away.
    fn move_to(&mut self, space: &str) {
        if let Ok(mut j) = self.panel.join.lock() {
            *j = None;
        }
        self.me.space = devices::clean_space(space);
        let saved = ini::write(&self.root, DEVICE_FILE, "space", &self.me.space);
        if let Err(e) = saved {
            eprintln!("slot: cloud: saving {DEVICE_FILE}: {e}");
        }
        eprintln!("slot: cloud: now syncing in {:?}", self.me.space);
        let me = self.me.clone();
        self.panel.show(|s| s.me = me);
        self.next = None;
    }

    fn start(&mut self, full: bool) {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, done) = mpsc::channel();
        let root = self.root.clone();
        let mut top = self.account.remote();
        let mut remote = self.account.remote_in(&self.me.space);
        let mut me = self.me.clone();
        me.seen = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let manifest = root.join(manifest_for(&me.space));
        let stop = cancel.clone();
        let held = self.game.clone();
        let game = held.clone();
        let folders = library(&self.account.folders(!full || held.is_some()));
        let panel = self.panel.clone();
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
                let result = slot_cloud::sync_counted(
                    &root,
                    &manifest,
                    &mut remote,
                    &folders,
                    &stop,
                    &holding,
                    &panel.tally,
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
        self.job = None;
        self.panel.show(|s| s.running = None);
        if full {
            let last = match &result {
                Ok(r) if r.cancelled => "Stopped".to_string(),
                Ok(r) => ended(r),
                Err(e) if e.offline => format!("Can't reach the server: {}", e.message),
                Err(e) => e.message.clone(),
            };
            self.panel.show(|s| s.last = Some(last));
        }
        match result {
            Ok(mut report) => {
                self.failing = false;
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

/// The folders a run covers: games and art only for the consoles Slot
/// plays, so another system's games (a DS's, say) stay on the server.
fn library(folders: &[&'static str]) -> Vec<String> {
    folders
        .iter()
        .flat_map(|&f| match f {
            slot_cloud::GAMES | slot_cloud::ART => Platform::ALL
                .iter()
                .map(|p| format!("{f}/{}", p.dir_name()))
                .collect(),
            f => vec![f.to_string()],
        })
        .collect()
}

/// The manifest for a space: what the last run there saw.
fn manifest_for(space: &str) -> String {
    match space.rsplit('/').next().filter(|_| !space.is_empty()) {
        Some(name) => format!("Config/cloudsync-{name}.manifest"),
        None => "Config/cloudsync.manifest".into(),
    }
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
    Device {
        id,
        name: MODEL.1.into(),
        space: devices::clean_space(saved.get("space").map_or("", |s| s.as_str())),
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

    fn dev(id: &str, space: &str) -> Device {
        Device {
            id: id.into(),
            name: id.into(),
            space: space.into(),
            seen: 5,
        }
    }

    #[test]
    fn ticking_a_device_joins_its_space_and_unticking_leaves_for_our_own() {
        let panel = Panel::default();
        let ds = dev("rgds-1", "Spaces/rgds-1");
        let sp = dev("rgsp-2", "");
        panel.show(|s| s.me = dev("brick-3", ""));
        assert!(panel.shown().shares_with(&sp));
        assert!(!panel.shown().shares_with(&ds));

        panel.toggle(&ds);
        assert_eq!(panel.join.lock().unwrap().as_deref(), Some("Spaces/rgds-1"));
        panel.toggle(&sp);
        assert_eq!(
            panel.join.lock().unwrap().as_deref(),
            Some("Spaces/brick-3")
        );
        panel.show(|s| s.me.space = "Spaces/brick-3".into());
        panel.toggle(&dev("rgsp-2", "Spaces/brick-3"));
        assert_eq!(
            panel.join.lock().unwrap().as_deref(),
            Some("Spaces/brick-3-5"),
            "a space of our own that others joined is left for a new one"
        );
    }

    #[test]
    fn games_and_art_only_for_the_consoles_slot_plays() {
        let f = library(&["Saves", slot_cloud::GAMES, slot_cloud::ART]);
        assert_eq!(f[0], "Saves");
        assert!(f.contains(&"Games/GBA".to_string()));
        assert!(f.contains(&"Labels/PS".to_string()));
        assert!(!f.iter().any(|f| f == "Games" || f.ends_with("/NDS")));
        assert_eq!(f.len(), 1 + 2 * Platform::ALL.len());
    }

    #[test]
    fn each_space_keeps_its_own_manifest() {
        assert_eq!(manifest_for(""), "Config/cloudsync.manifest");
        assert_eq!(
            manifest_for("Spaces/rgds-1"),
            "Config/cloudsync-rgds-1.manifest"
        );
    }

    #[test]
    fn a_device_keeps_its_id() {
        let d = tempfile::tempdir().unwrap();
        let first = this_device(d.path());
        assert!(first.id.starts_with(&format!("{}-", MODEL.0)), "{first:?}");
        assert_eq!(this_device(d.path()).id, first.id);
        ini::write(d.path(), DEVICE_FILE, "space", "Spaces/x-1").unwrap();
        assert_eq!(this_device(d.path()).space, "Spaces/x-1");
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
