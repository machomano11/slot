//! Cloud sync: keeps `Saves` and `States`, and the ROMs in `Games` and their
//! art in `Labels`, in step with a WebDAV folder named in [`ACCOUNT_FILE`].
//! It runs in the background: at start-up, as soon as a game is closed, and
//! every few minutes. While a game runs, its own save and states are only ever
//! sent, never replaced, and the ROMs and art wait; the next run after it
//! closes brings in anything newer from the server. Games that arrive join the
//! game list without a restart.
//!
//! Nothing happens without that file, so a card without it behaves exactly
//! as before.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slot_cloud::{Account, Error, Report};
use slot_store::{ini, Platform};

use crate::app::App;

/// `url` of the WebDAV folder, plus `username` and `password`; `roms=0` and
/// `art=0` leave those out.
pub const ACCOUNT_FILE: &str = "Config/cloudsync.ini";
const MANIFEST: &str = "Config/cloudsync.manifest";
const EVERY: Duration = Duration::from_secs(10 * 60);
const RETRY: Duration = Duration::from_secs(60);
const TITLE: &str = "Cloud sync";

pub struct CloudSync {
    root: PathBuf,
    account: Account,
    job: Option<Job>,
    next: Option<Instant>,
    /// The running game: its stem and platform folder.
    game: Option<(String, &'static str)>,
    failing: bool,
    /// Games came down and the game list hasn't been read again yet.
    rescan: bool,
    /// How many files the last run had no room for, so a full card is said once.
    no_room: usize,
}

struct Job {
    cancel: Arc<AtomicBool>,
    done: Receiver<Result<Report, Error>>,
    /// The game whose files this run leaves alone, if any.
    held: Option<(String, &'static str)>,
}

impl CloudSync {
    pub fn boot(root: &Path) -> Option<CloudSync> {
        let account = Account::from_ini(&ini::read(root, ACCOUNT_FILE))?;
        eprintln!("slot: cloud: syncing with {}", account.url);
        Some(CloudSync {
            root: root.to_path_buf(),
            account,
            job: None,
            next: None,
            game: None,
            failing: false,
            rescan: false,
            no_room: 0,
        })
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
        if self.rescan && !playing {
            self.rescan = !app.rescan();
        }
        if self.job.is_none() && self.next.is_none_or(|t| Instant::now() >= t) {
            self.start();
        }
    }

    fn start(&mut self) {
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, done) = mpsc::channel();
        let root = self.root.clone();
        let mut remote = self.account.remote();
        let stop = cancel.clone();
        let held = self.game.clone();
        let game = held.clone();
        let folders = self.account.folders(held.is_some());
        let spawned = std::thread::Builder::new()
            .name("slot-cloud".into())
            .spawn(move || {
                let holding = |rel: &str| {
                    game.as_ref()
                        .is_some_and(|(stem, dir)| slot_cloud::belongs_to(rel, dir, stem))
                };
                let manifest = root.join(MANIFEST);
                let result = slot_cloud::sync_holding(
                    &root,
                    &manifest,
                    &mut remote,
                    &folders,
                    &stop,
                    &holding,
                );
                let _ = tx.send(result);
            });
        match spawned {
            Ok(_) => self.job = Some(Job { cancel, done, held }),
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
        let result = match job.done.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(Error::new("the sync stopped unexpectedly")),
        };
        self.job = None;
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
