//! Two-way sync of a card's `Saves` and `States` with a remote folder.
//!
//! A manifest remembers each file as it was after the last sync, on both
//! sides, so a run can tell which side changed:
//!
//! - changed on one side only: copied to the other;
//! - changed on both, or found on both with no history: if the bytes differ,
//!   the newer copy wins and the other is kept on the card as
//!   `<name>.conflict-<when>`, which is never synced;
//! - gone on one side and unchanged on the other: deleted there too, except
//!   under `Saves`, where nothing is ever deleted and the missing copy comes
//!   back instead;
//! - files written in the last few seconds wait for the next run;
//! - files of the game being played ("held") are only ever uploaded: nothing
//!   overwrites or deletes them under a running game.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod webdav;

pub use webdav::WebDav;

/// Whether `rel` is one of the files a game keeps: its save
/// (`Saves/<platform>/<stem>.<ext>`) and its states
/// (`States/<platform>/<core>/<stem>/…`).
pub fn belongs_to(rel: &str, platform_dir: &str, stem: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    match parts.as_slice() {
        ["Saves", p, file] => {
            *p == platform_dir
                && file
                    .strip_prefix(stem)
                    .is_some_and(|rest| rest.starts_with('.'))
        }
        ["States", p, _core, s, _, ..] => *p == platform_dir && *s == stem,
        _ => false,
    }
}

/// The card folders that sync, relative to the root.
pub const FOLDERS: [&str; 2] = ["Saves", "States"];
/// Folders where a deletion is never passed on.
const KEEP: [&str; 1] = ["Saves"];
const SETTLE: Duration = Duration::from_secs(3);
const MANIFEST_HEADER: &str = "slot-cloud 1";
const CONFLICT: &str = ".conflict-";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    /// The server couldn't be reached at all (no Wi-Fi yet, say), as opposed
    /// to it answering with a refusal.
    pub offline: bool,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            offline: false,
        }
    }

    pub fn offline(message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            offline: true,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// One entry of a remote folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Changes whenever the contents do (the ETag, or size and date).
    pub tag: String,
    /// Seconds since 1970.
    pub modified: Option<u64>,
}

pub trait Remote {
    /// A folder's entries, or `None` if it doesn't exist.
    fn list(&mut self, dir: &str) -> Result<Option<Vec<Listed>>, Error>;
    fn stat(&mut self, rel: &str) -> Result<Option<Listed>, Error>;
    fn get(&mut self, rel: &str) -> Result<Vec<u8>, Error>;
    fn put(&mut self, rel: &str, bytes: &[u8]) -> Result<(), Error>;
    fn mkdir(&mut self, rel: &str) -> Result<(), Error>;
    fn delete(&mut self, rel: &str) -> Result<(), Error>;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub uploaded: usize,
    pub downloaded: usize,
    pub deleted: usize,
    /// Files where both sides changed; the loser was kept as a conflict copy.
    pub conflicts: Vec<String>,
    /// The run stopped early because it was asked to.
    pub cancelled: bool,
}

impl Report {
    pub fn changed(&self) -> bool {
        self.uploaded + self.downloaded + self.deleted > 0 || !self.conflicts.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Seen {
    local: u64,
    size: u64,
    tag: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Local {
    mtime_ns: u64,
    size: u64,
}

/// Syncs `root`'s [`FOLDERS`] with `remote`, keeping what it learns in
/// `manifest`. Stops between files once `cancel` is set.
pub fn sync(
    root: &Path,
    manifest: &Path,
    remote: &mut dyn Remote,
    cancel: &AtomicBool,
) -> Result<Report, Error> {
    sync_holding(root, manifest, remote, cancel, &|_| false)
}

/// [`sync`], with the files for which `held` is true (a running game's) only
/// uploaded when they changed here and not on the server.
pub fn sync_holding(
    root: &Path,
    manifest: &Path,
    remote: &mut dyn Remote,
    cancel: &AtomicBool,
    held: &dyn Fn(&str) -> bool,
) -> Result<Report, Error> {
    let mut seen = read_manifest(manifest);
    let mut run = Run {
        root,
        remote,
        dirs: BTreeSet::new(),
        report: Report::default(),
        now: SystemTime::now(),
    };
    let result = run.all(&mut seen, cancel, held);
    let saved = write_manifest(manifest, &seen);
    result?;
    saved.map_err(|e| Error::new(format!("saving {}: {e}", manifest.display())))?;
    Ok(run.report)
}

struct Run<'a> {
    root: &'a Path,
    remote: &'a mut dyn Remote,
    /// Remote folders known to exist.
    dirs: BTreeSet<String>,
    report: Report,
    now: SystemTime,
}

impl Run<'_> {
    fn all(
        &mut self,
        seen: &mut BTreeMap<String, Seen>,
        cancel: &AtomicBool,
        held: &dyn Fn(&str) -> bool,
    ) -> Result<(), Error> {
        let mut local = BTreeMap::new();
        let mut settling = BTreeSet::new();
        for folder in FOLDERS {
            walk_local(self.root, folder, self.now, &mut local, &mut settling);
        }
        // The folder named in the settings may not exist yet.
        if self.remote.list("")?.is_none() {
            self.remote.mkdir("")?;
        }
        let mut remote = BTreeMap::new();
        for folder in FOLDERS {
            self.walk_remote(folder, &mut remote)?;
        }
        let names: BTreeSet<String> = local
            .keys()
            .chain(remote.keys())
            .chain(seen.keys())
            .cloned()
            .collect();
        for rel in names {
            if settling.contains(&rel) {
                continue;
            }
            if cancel.load(Ordering::Relaxed) {
                self.report.cancelled = true;
                return Ok(());
            }
            let l = local.get(&rel).copied();
            let r = remote.get(&rel);
            let s = seen.get(&rel);
            if held(&rel) {
                if let Some(l) = l.filter(|&l| only_sent(rel.as_str(), l, r, s)) {
                    let s = self.upload(&rel, l)?;
                    seen.insert(rel, s);
                }
                continue;
            }
            match self.one(&rel, l, r, s)? {
                Some(s) => seen.insert(rel, s),
                None => seen.remove(&rel),
            };
        }
        Ok(())
    }

    /// Brings one file in line; returns what to remember about it.
    fn one(
        &mut self,
        rel: &str,
        l: Option<Local>,
        r: Option<&Listed>,
        s: Option<&Seen>,
    ) -> Result<Option<Seen>, Error> {
        let keep = KEEP.iter().any(|k| rel.starts_with(&format!("{k}/")));
        let local_changed = |l: Local, s: &Seen| l.mtime_ns != s.local || l.size != s.size;
        Ok(match (l, r, s) {
            (Some(l), Some(r), Some(s)) => match (local_changed(l, s), r.tag != s.tag) {
                (false, false) => Some(s.clone()),
                (true, false) => Some(self.upload(rel, l)?),
                (false, true) => Some(self.download(rel, r)?),
                (true, true) => Some(self.both(rel, l, r)?),
            },
            (Some(l), Some(r), None) => Some(self.both(rel, l, r)?),
            (Some(l), None, Some(s)) if !keep && !local_changed(l, s) => {
                self.remove_local(rel)?;
                None
            }
            (Some(l), None, _) => Some(self.upload(rel, l)?),
            (None, Some(r), Some(s)) if !keep && r.tag == s.tag => {
                self.remote.delete(rel)?;
                self.report.deleted += 1;
                None
            }
            (None, Some(r), _) => Some(self.download(rel, r)?),
            (None, None, _) => None,
        })
    }

    fn upload(&mut self, rel: &str, l: Local) -> Result<Seen, Error> {
        let bytes = std::fs::read(self.root.join(rel))
            .map_err(|e| Error::new(format!("reading {rel}: {e}")))?;
        self.make_parents(rel)?;
        self.remote.put(rel, &bytes)?;
        let tag = self
            .remote
            .stat(rel)?
            .map(|r| r.tag)
            .ok_or_else(|| Error::new(format!("{rel} vanished right after uploading")))?;
        self.report.uploaded += 1;
        eprintln!("slot: cloud: uploaded {rel}");
        Ok(Seen {
            local: l.mtime_ns,
            size: l.size,
            tag,
        })
    }

    fn download(&mut self, rel: &str, r: &Listed) -> Result<Seen, Error> {
        let bytes = self.remote.get(rel)?;
        let seen = self.write_local(rel, &bytes, &r.tag)?;
        self.report.downloaded += 1;
        eprintln!("slot: cloud: downloaded {rel}");
        Ok(seen)
    }

    /// Both sides have the file and at least one changed it.
    fn both(&mut self, rel: &str, l: Local, r: &Listed) -> Result<Seen, Error> {
        let path = self.root.join(rel);
        let mine = std::fs::read(&path).map_err(|e| Error::new(format!("reading {rel}: {e}")))?;
        let theirs = self.remote.get(rel)?;
        if mine == theirs {
            return Ok(Seen {
                local: l.mtime_ns,
                size: l.size,
                tag: r.tag.clone(),
            });
        }
        let conflict = format!("{rel}{CONFLICT}{}", stamp(self.now));
        let local_secs = l.mtime_ns / 1_000_000_000;
        let seen = if r.modified.unwrap_or(0) > local_secs {
            eprintln!("slot: cloud: {rel} changed on both sides, keeping the server's newer copy");
            std::fs::rename(&path, self.root.join(&conflict))
                .map_err(|e| Error::new(format!("keeping {conflict}: {e}")))?;
            let seen = self.write_local(rel, &theirs, &r.tag)?;
            self.report.downloaded += 1;
            seen
        } else {
            eprintln!("slot: cloud: {rel} changed on both sides, keeping this card's newer copy");
            atomic_write(&self.root.join(&conflict), &theirs)
                .map_err(|e| Error::new(format!("keeping {conflict}: {e}")))?;
            self.upload(rel, l)?
        };
        self.report.conflicts.push(rel.to_string());
        Ok(seen)
    }

    fn write_local(&mut self, rel: &str, bytes: &[u8], tag: &str) -> Result<Seen, Error> {
        let path = self.root.join(rel);
        atomic_write(&path, bytes).map_err(|e| Error::new(format!("writing {rel}: {e}")))?;
        let l = local_of(&path)
            .ok_or_else(|| Error::new(format!("{rel} vanished right after writing")))?;
        Ok(Seen {
            local: l.mtime_ns,
            size: l.size,
            tag: tag.to_string(),
        })
    }

    fn remove_local(&mut self, rel: &str) -> Result<(), Error> {
        match std::fs::remove_file(self.root.join(rel)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(Error::new(format!("deleting {rel}: {e}"))),
        }
        eprintln!("slot: cloud: deleted {rel}, as the server did");
        self.report.deleted += 1;
        Ok(())
    }

    fn make_parents(&mut self, rel: &str) -> Result<(), Error> {
        let parts: Vec<&str> = rel.split('/').collect();
        for i in 1..parts.len() {
            let dir = parts[..i].join("/");
            if self.dirs.insert(dir.clone()) {
                self.remote.mkdir(&dir)?;
            }
        }
        Ok(())
    }

    fn walk_remote(
        &mut self,
        folder: &str,
        out: &mut BTreeMap<String, Listed>,
    ) -> Result<(), Error> {
        let mut todo = vec![folder.to_string()];
        while let Some(dir) = todo.pop() {
            let Some(entries) = self.remote.list(&dir)? else {
                continue;
            };
            self.dirs.insert(dir.clone());
            for e in entries {
                if skipped(&e.name) {
                    continue;
                }
                let rel = format!("{dir}/{}", e.name);
                match e.is_dir {
                    true => todo.push(rel),
                    false => {
                        out.insert(rel, e);
                    }
                }
            }
        }
        Ok(())
    }
}

/// Whether a held file is one that may simply be sent: it changed here, and
/// the server's copy is the one last seen (or there is none and never was).
fn only_sent(rel: &str, l: Local, r: Option<&Listed>, s: Option<&Seen>) -> bool {
    let keep = KEEP.iter().any(|k| rel.starts_with(&format!("{k}/")));
    match (r, s) {
        (None, None) => true,
        (None, Some(s)) => keep || l.mtime_ns != s.local || l.size != s.size,
        (Some(r), Some(s)) => r.tag == s.tag && (l.mtime_ns != s.local || l.size != s.size),
        (Some(_), None) => false,
    }
}

/// Hidden files (half-written temporaries among them) and conflict copies.
fn skipped(name: &str) -> bool {
    name.is_empty() || name.starts_with('.') || name.contains(CONFLICT)
}

fn walk_local(
    root: &Path,
    folder: &str,
    now: SystemTime,
    out: &mut BTreeMap<String, Local>,
    settling: &mut BTreeSet<String>,
) {
    let mut todo = vec![folder.to_string()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&dir)) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if skipped(&name) {
                continue;
            }
            let rel = format!("{dir}/{name}");
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                todo.push(rel);
            } else if meta.is_file() {
                let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                if now.duration_since(modified).is_ok_and(|age| age < SETTLE) {
                    settling.insert(rel.clone());
                }
                out.insert(
                    rel,
                    Local {
                        mtime_ns: ns(modified),
                        size: meta.len(),
                    },
                );
            }
        }
    }
}

fn local_of(path: &Path) -> Option<Local> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Local {
        mtime_ns: ns(meta.modified().unwrap_or(UNIX_EPOCH)),
        size: meta.len(),
    })
}

fn ns(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp: PathBuf = dir.join(format!(".{name}.cloud.tmp"));
    let mut f = std::fs::File::create(&tmp)?;
    let written = f.write_all(bytes).and_then(|_| f.sync_all());
    drop(f);
    if let Err(e) = written.and_then(|_| std::fs::rename(&tmp, path)) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

fn read_manifest(path: &Path) -> BTreeMap<String, Seen> {
    let mut out = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else {
        return out;
    };
    let mut lines = text.lines();
    if lines.next() != Some(MANIFEST_HEADER) {
        return out;
    }
    for line in lines {
        let mut f = line.splitn(4, '\t');
        let (Some(local), Some(size), Some(tag), Some(rel)) =
            (f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        let (Ok(local), Ok(size)) = (local.parse(), size.parse()) else {
            continue;
        };
        out.insert(
            rel.to_string(),
            Seen {
                local,
                size,
                tag: tag.to_string(),
            },
        );
    }
    out
}

fn write_manifest(path: &Path, seen: &BTreeMap<String, Seen>) -> std::io::Result<()> {
    let mut text = String::from(MANIFEST_HEADER);
    text.push('\n');
    for (rel, s) in seen {
        let tag = s.tag.replace(['\t', '\n'], " ");
        text.push_str(&format!("{}\t{}\t{tag}\t{rel}\n", s.local, s.size));
    }
    atomic_write(path, text.as_bytes())
}

/// `20261014-100300` in UTC.
fn stamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = civil_from_days((secs / 86_400) as i64);
    let s = secs % 86_400;
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// What a card's `Config/cloudsync.ini` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Account {
    /// `url` is required; `enabled=0` turns sync off.
    pub fn from_ini(values: &HashMap<String, String>) -> Option<Account> {
        let get = |k: &str| values.get(k).map(|v| v.trim()).filter(|v| !v.is_empty());
        if matches!(get("enabled"), Some("0" | "false" | "off" | "no")) {
            return None;
        }
        let url = get("url")?;
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            eprintln!("slot: cloud: url must start with https:// or http://");
            return None;
        }
        Some(Account {
            url: url.to_string(),
            username: get("username").map(str::to_string),
            password: get("password").map(str::to_string),
        })
    }

    pub fn remote(&self) -> WebDav {
        WebDav::new(
            &self.url,
            self.username.as_deref(),
            self.password.as_deref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        for days in [-1, 0, 59, 365, 11_016, 20_740, 30_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(days_from_civil(2026, 10, 14), 20_740);
        assert_eq!(
            stamp(UNIX_EPOCH + Duration::from_secs(20_740 * 86_400 + 36_180)),
            "20261014-100300"
        );
    }

    #[test]
    fn the_manifest_survives_a_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("m");
        let mut seen = BTreeMap::new();
        seen.insert(
            "Saves/GBA/a b.sav".to_string(),
            Seen {
                local: 12,
                size: 3,
                tag: "\"x\ty\"".into(),
            },
        );
        write_manifest(&path, &seen).unwrap();
        let back = read_manifest(&path);
        assert_eq!(back["Saves/GBA/a b.sav"].tag, "\"x y\"");
        assert_eq!(back["Saves/GBA/a b.sav"].local, 12);
    }
}
