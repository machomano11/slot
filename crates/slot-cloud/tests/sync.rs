use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use slot_cloud::{
    belongs_to, sync, sync_counted, sync_holding, Error, Listed, Moment, Remote, Report, Tally,
    FOLDERS, PROGRESS,
};

#[derive(Default)]
struct Memory {
    files: BTreeMap<String, (Vec<u8>, u64, u64)>,
    dirs: BTreeSet<String>,
    version: u64,
    clock: u64,
    /// Files fetched, in order.
    gets: Vec<String>,
    /// Set as an upload starts, as a game starting would.
    cancel_on_put: Option<Arc<AtomicBool>>,
    /// Refuses MOVE, as a few servers do.
    no_move: bool,
    /// Uploads to names containing this are refused, as a server's size limit would.
    refuse: Option<&'static str>,
    /// The server stops answering at the first refused upload.
    goes_down: bool,
    down: bool,
    /// Files stored, in order.
    puts: Vec<String>,
    /// Read as each upload finishes, into `moments`.
    tally: Option<Arc<Tally>>,
    moments: Vec<Moment>,
}

impl Memory {
    fn store(&mut self, rel: &str, bytes: &[u8], modified: u64) {
        self.version += 1;
        let parts: Vec<&str> = rel.split('/').collect();
        for i in 1..parts.len() {
            self.dirs.insert(parts[..i].join("/"));
        }
        self.files
            .insert(rel.into(), (bytes.to_vec(), self.version, modified));
    }

    fn bytes(&self, rel: &str) -> Option<&[u8]> {
        self.files.get(rel).map(|f| f.0.as_slice())
    }

    fn listed(&self, rel: &str) -> Option<Listed> {
        self.files.get(rel).map(|(b, v, m)| Listed {
            name: rel.rsplit('/').next().unwrap().into(),
            is_dir: false,
            size: b.len() as u64,
            tag: format!("v{v}"),
            modified: Some(*m),
        })
    }
}

impl Remote for Memory {
    fn list(&mut self, dir: &str) -> Result<Option<Vec<Listed>>, Error> {
        if !self.dirs.contains(dir) {
            return Ok(None);
        }
        let prefix = format!("{dir}/");
        let mut out: Vec<Listed> = Vec::new();
        for d in &self.dirs {
            if let Some(name) = d.strip_prefix(&prefix).filter(|n| !n.contains('/')) {
                out.push(Listed {
                    name: name.into(),
                    is_dir: true,
                    size: 0,
                    tag: String::new(),
                    modified: None,
                });
            }
        }
        for rel in self.files.keys() {
            if rel.strip_prefix(&prefix).is_some_and(|n| !n.contains('/')) {
                out.push(self.listed(rel).unwrap());
            }
        }
        Ok(Some(out))
    }

    fn stat(&mut self, rel: &str) -> Result<Option<Listed>, Error> {
        if self.down {
            return Err(Error::offline("connection refused"));
        }
        Ok(self.listed(rel))
    }

    fn get(&mut self, rel: &str, out: &mut dyn Write) -> Result<(), Error> {
        self.gets.push(rel.into());
        let bytes = self
            .bytes(rel)
            .map(<[u8]>::to_vec)
            .ok_or(Error::new(format!("404 {rel}")))?;
        out.write_all(&bytes).map_err(|e| Error::new(e.to_string()))
    }

    fn put(&mut self, rel: &str, body: &mut dyn Read, len: u64) -> Result<(), Error> {
        let parent = rel.rsplit_once('/').map(|p| p.0).unwrap_or("");
        if !parent.is_empty() && !self.dirs.contains(parent) {
            return Err(Error::new(format!("409 no folder for {rel}")));
        }
        if self.refuse.is_some_and(|r| rel.contains(r)) {
            self.down = self.goes_down;
            return Err(Error::new("HTTP 413 Payload Too Large"));
        }
        if let Some(cancel) = &self.cancel_on_put {
            cancel.store(true, Ordering::Relaxed);
        }
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .map_err(|e| Error::new(e.to_string()))?;
        assert_eq!(bytes.len() as u64, len);
        self.clock += 1;
        let now = self.clock;
        self.store(rel, &bytes, now);
        self.puts.push(rel.into());
        if let Some(t) = &self.tally {
            self.moments.push(t.read());
        }
        Ok(())
    }

    fn rename(&mut self, from: &str, to: &str) -> Result<(), Error> {
        if self.no_move {
            return Err(Error::new("405 Method Not Allowed"));
        }
        let (bytes, _, modified) = self
            .files
            .remove(from)
            .ok_or(Error::new(format!("404 {from}")))?;
        self.store(to, &bytes, modified);
        Ok(())
    }

    fn mkdir(&mut self, rel: &str) -> Result<(), Error> {
        self.dirs.insert(rel.into());
        Ok(())
    }

    fn delete(&mut self, rel: &str) -> Result<(), Error> {
        self.files.remove(rel);
        Ok(())
    }
}

const HOUR: u64 = 3600;

fn secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Writes a file as if it was last touched `ago` seconds back, so it doesn't
/// count as still being written.
fn write_aged(root: &Path, rel: &str, bytes: &[u8], ago: u64) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    let f = std::fs::File::options().write(true).open(&path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(ago))
        .unwrap();
}

fn run(root: &Path, remote: &mut Memory) -> Report {
    sync(
        root,
        &root.join("Config/cloudsync.manifest"),
        remote,
        &AtomicBool::new(false),
    )
    .expect("sync")
}

fn read(root: &Path, rel: &str) -> Option<Vec<u8>> {
    std::fs::read(root.join(rel)).ok()
}

#[test]
fn the_first_run_copies_each_side_to_the_other() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"mine", HOUR);
    write_aged(root, "States/GBA/mgba/a/resume.state", b"state", HOUR);
    let mut remote = Memory::default();
    remote.store("Saves/GB/b.sav", b"theirs", secs_now() - HOUR);

    let r = run(root, &mut remote);
    assert_eq!((r.uploaded, r.downloaded, r.deleted), (2, 1, 0));
    assert_eq!(remote.bytes("Saves/GBA/a.sav"), Some(&b"mine"[..]));
    assert_eq!(
        remote.bytes("States/GBA/mgba/a/resume.state"),
        Some(&b"state"[..])
    );
    assert_eq!(
        read(root, "Saves/GB/b.sav").as_deref(),
        Some(&b"theirs"[..])
    );

    assert_eq!(
        run(root, &mut remote),
        Report::default(),
        "a second run has nothing to do"
    );
}

#[test]
fn a_change_on_one_side_goes_to_the_other() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"v1", 2 * HOUR);
    write_aged(root, "Saves/GBA/b.sav", b"v1", 2 * HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);

    write_aged(root, "Saves/GBA/a.sav", b"v2 here", HOUR);
    remote.store("Saves/GBA/b.sav", b"v2 there", secs_now());
    let r = run(root, &mut remote);
    assert_eq!((r.uploaded, r.downloaded), (1, 1));
    assert!(r.conflicts.is_empty());
    assert_eq!(remote.bytes("Saves/GBA/a.sav"), Some(&b"v2 here"[..]));
    assert_eq!(
        read(root, "Saves/GBA/b.sav").as_deref(),
        Some(&b"v2 there"[..])
    );
}

#[test]
fn when_both_sides_change_the_newer_wins_and_the_other_is_kept_on_the_card() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/old_here.sav", b"v1", 3 * HOUR);
    write_aged(root, "Saves/GBA/new_here.sav", b"v1", 3 * HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);

    // This card played old_here two hours ago; the other device an hour ago.
    write_aged(root, "Saves/GBA/old_here.sav", b"card", 2 * HOUR);
    remote.store("Saves/GBA/old_here.sav", b"server", secs_now() - HOUR);
    // And the other way round for new_here.
    remote.store("Saves/GBA/new_here.sav", b"server", secs_now() - 2 * HOUR);
    write_aged(root, "Saves/GBA/new_here.sav", b"card", HOUR);

    let r = run(root, &mut remote);
    assert_eq!(r.conflicts.len(), 2);
    assert_eq!(
        read(root, "Saves/GBA/old_here.sav").as_deref(),
        Some(&b"server"[..])
    );
    assert_eq!(remote.bytes("Saves/GBA/new_here.sav"), Some(&b"card"[..]));

    let kept: Vec<String> = std::fs::read_dir(root.join("Saves/GBA"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".conflict-"))
        .collect();
    assert_eq!(kept.len(), 2, "{kept:?}");
    for name in &kept {
        let bytes = read(root, &format!("Saves/GBA/{name}")).unwrap();
        match name.starts_with("old_here") {
            true => assert_eq!(bytes, b"card"),
            false => assert_eq!(bytes, b"server"),
        }
        assert!(
            remote.bytes(&format!("Saves/GBA/{name}")).is_none(),
            "conflict copies stay on the card"
        );
    }
    assert_eq!(run(root, &mut remote), Report::default());
}

#[test]
fn identical_files_found_on_both_sides_are_not_a_conflict() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"same", HOUR);
    let mut remote = Memory::default();
    remote.store("Saves/GBA/a.sav", b"same", secs_now());
    let r = run(root, &mut remote);
    assert_eq!(r, Report::default());
}

#[test]
fn deleted_states_are_deleted_on_the_other_side_but_saves_come_back() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"save", HOUR);
    write_aged(root, "States/GBA/mgba/a/1.state", b"one", HOUR);
    write_aged(root, "States/GBA/mgba/a/2.state", b"two", HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);

    std::fs::remove_file(root.join("Saves/GBA/a.sav")).unwrap();
    std::fs::remove_file(root.join("States/GBA/mgba/a/1.state")).unwrap();
    remote.delete("States/GBA/mgba/a/2.state").unwrap();

    let r = run(root, &mut remote);
    assert_eq!((r.deleted, r.downloaded), (2, 1));
    assert_eq!(read(root, "Saves/GBA/a.sav").as_deref(), Some(&b"save"[..]));
    assert!(remote.bytes("States/GBA/mgba/a/1.state").is_none());
    assert!(read(root, "States/GBA/mgba/a/2.state").is_none());

    remote.delete("Saves/GBA/a.sav").unwrap();
    // The copy that just came back is old enough to sync by the next run.
    write_aged(root, "Saves/GBA/a.sav", b"save", HOUR);
    let r = run(root, &mut remote);
    assert_eq!(r.uploaded, 1, "a save the server lost is uploaded again");
}

#[test]
fn files_still_being_written_and_hidden_files_wait() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/fresh.sav", b"x", 0);
    write_aged(root, "Saves/GBA/.fresh.sav.1.2.tmp", b"x", HOUR);
    let mut remote = Memory::default();
    let r = run(root, &mut remote);
    assert_eq!(r, Report::default());
    assert!(remote.files.is_empty());
}

#[test]
fn a_cancelled_run_stops_before_touching_anything() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"x", HOUR);
    let mut remote = Memory::default();
    let r = sync(
        root,
        &root.join("Config/m"),
        &mut remote,
        &AtomicBool::new(true),
    )
    .unwrap();
    assert!(r.cancelled);
    assert!(remote.files.is_empty());
}

#[test]
fn a_running_game_s_files_are_sent_but_never_replaced() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/Zelda.sav", b"v1", 3 * HOUR);
    write_aged(root, "Saves/GBA/Metroid.sav", b"v1", 3 * HOUR);
    write_aged(root, "States/GBA/mgba/Zelda/1.state", b"one", 3 * HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);
    let zelda = |rel: &str| belongs_to(rel, "GBA", "Zelda");
    let held = |root: &Path, remote: &mut Memory| {
        sync_holding(
            root,
            &root.join("Config/cloudsync.manifest"),
            remote,
            &FOLDERS,
            &AtomicBool::new(false),
            &zelda,
        )
        .unwrap()
    };

    // Played here: sent while still playing.
    write_aged(root, "Saves/GBA/Zelda.sav", b"v2", 2 * HOUR);
    let r = held(root, &mut remote);
    assert_eq!(r.uploaded, 1);
    assert_eq!(remote.bytes("Saves/GBA/Zelda.sav"), Some(&b"v2"[..]));

    // Changed elsewhere, or deleted elsewhere: left alone until the game ends.
    remote.store("Saves/GBA/Zelda.sav", b"other device", secs_now());
    remote.delete("States/GBA/mgba/Zelda/1.state").unwrap();
    remote.store("Saves/GBA/Metroid.sav", b"other device", secs_now());
    let r = held(root, &mut remote);
    assert_eq!(
        (r.uploaded, r.downloaded, r.deleted),
        (0, 1, 0),
        "only Metroid moves: {r:?}"
    );
    assert_eq!(
        read(root, "Saves/GBA/Zelda.sav").as_deref(),
        Some(&b"v2"[..])
    );
    assert!(read(root, "States/GBA/mgba/Zelda/1.state").is_some());

    // Once the game is closed the server's newer save comes down.
    let r = run(root, &mut remote);
    assert_eq!((r.downloaded, r.deleted), (1, 1), "{r:?}");
    assert_eq!(
        read(root, "Saves/GBA/Zelda.sav").as_deref(),
        Some(&b"other device"[..])
    );
}

#[test]
fn a_game_s_files_are_its_save_and_its_states() {
    let z = |rel: &str| belongs_to(rel, "GBA", "Zelda");
    assert!(z("Saves/GBA/Zelda.sav"));
    assert!(z("Saves/GBA/Zelda.rtc"));
    assert!(z("States/GBA/mgba/Zelda/resume.state"));
    assert!(z("States/GBA/gpsp/Zelda/20261014-100300.png"));
    assert!(!z("Saves/GBA/Zelda II.sav"));
    assert!(!z("Saves/GB/Zelda.sav"));
    assert!(!z("States/GBA/mgba/Zelda II/resume.state"));
    assert!(!z("Saves/GBA/Zelda"));
}

fn hidden_on_server(remote: &Memory) -> Vec<&String> {
    remote
        .files
        .keys()
        .filter(|k| k.rsplit('/').next().unwrap().starts_with('.'))
        .collect()
}

#[test]
fn games_and_their_art_sync_and_never_vanish() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(
        root,
        "Games/PS/Vagrant Story (USA).chd",
        &[7; 300_000],
        HOUR,
    );
    write_aged(root, "Labels/PS/Vagrant Story (USA).png", b"png", HOUR);
    write_aged(root, "Labels/cart_shell.ini", b"[shell]", HOUR);
    let mut remote = Memory::default();
    remote.store("Games/GBA/Advance.gba", b"rom", secs_now() - HOUR);

    let r = run(root, &mut remote);
    assert_eq!((r.uploaded, r.downloaded), (3, 1), "{r:?}");
    assert_eq!(r.received, ["Games/GBA/Advance.gba"]);
    assert_eq!(
        remote
            .bytes("Games/PS/Vagrant Story (USA).chd")
            .map(<[u8]>::len),
        Some(300_000)
    );
    assert!(
        hidden_on_server(&remote).is_empty(),
        "no upload left half-named"
    );
    assert_eq!(
        read(root, "Games/GBA/Advance.gba").as_deref(),
        Some(&b"rom"[..])
    );

    // Gone from the card, or from the server: it comes back rather than being deleted.
    std::fs::remove_file(root.join("Games/PS/Vagrant Story (USA).chd")).unwrap();
    remote.delete("Labels/PS/Vagrant Story (USA).png").unwrap();
    let r = run(root, &mut remote);
    assert_eq!((r.uploaded, r.downloaded, r.deleted), (1, 1, 0), "{r:?}");
    assert!(read(root, "Games/PS/Vagrant Story (USA).chd").is_some());
}

#[test]
fn a_game_already_on_both_sides_is_not_fetched_to_compare() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/GBA/a.gba", b"rom!", HOUR);
    write_aged(root, "Saves/GBA/a.sav", b"save", HOUR);
    let mut remote = Memory::default();
    remote.store("Games/GBA/a.gba", b"rom!", secs_now());
    remote.store("Saves/GBA/a.sav", b"save", secs_now());
    let r = run(root, &mut remote);
    assert_eq!(r, Report::default());
    assert_eq!(remote.gets, ["Saves/GBA/a.sav"], "saves are still compared");
}

#[test]
fn a_run_of_some_folders_leaves_the_rest_and_what_it_knows_of_them() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/GBA/a.gba", b"rom", 2 * HOUR);
    write_aged(root, "Saves/GBA/a.sav", b"v1", 2 * HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);

    write_aged(root, "Games/GBA/b.gba", b"new rom", HOUR);
    write_aged(root, "Saves/GBA/a.sav", b"v2", HOUR);
    let r = sync_holding(
        root,
        &root.join("Config/cloudsync.manifest"),
        &mut remote,
        &PROGRESS,
        &AtomicBool::new(false),
        &|_| false,
    )
    .unwrap();
    assert_eq!(r.uploaded, 1, "only the save: {r:?}");
    assert!(remote.bytes("Games/GBA/b.gba").is_none());

    let r = run(root, &mut remote);
    assert_eq!(
        (r.uploaded, r.downloaded),
        (1, 0),
        "just the new game: {r:?}"
    );
}

#[test]
fn a_run_stopped_mid_upload_leaves_no_file_behind() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/PS/big.chd", &[1; 200_000], HOUR);
    let cancel = Arc::new(AtomicBool::new(false));
    let mut remote = Memory {
        cancel_on_put: Some(cancel.clone()),
        ..Memory::default()
    };
    let manifest = root.join("Config/cloudsync.manifest");
    let r = sync(root, &manifest, &mut remote, &cancel).unwrap();
    assert!(r.cancelled);
    assert_eq!(r.uploaded, 0);
    assert!(remote.bytes("Games/PS/big.chd").is_none());

    remote.cancel_on_put = None;
    let r = run(root, &mut remote);
    assert_eq!(r.uploaded, 1, "sent whole next time: {r:?}");
    assert_eq!(
        remote.bytes("Games/PS/big.chd").map(<[u8]>::len),
        Some(200_000)
    );
}

#[test]
fn a_server_that_cannot_rename_still_gets_the_file() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/a.sav", b"save", HOUR);
    let mut remote = Memory {
        no_move: true,
        ..Memory::default()
    };
    let r = run(root, &mut remote);
    assert_eq!(r.uploaded, 1);
    assert_eq!(remote.bytes("Saves/GBA/a.sav"), Some(&b"save"[..]));
    assert!(hidden_on_server(&remote).is_empty());
    assert_eq!(run(root, &mut remote), Report::default());
}

#[test]
fn a_conflict_on_a_big_file_streams_and_keeps_both() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/PS/a.srm", &[1; 100_000], 3 * HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);
    write_aged(root, "Saves/PS/a.srm", &[2; 100_000], 2 * HOUR);
    remote.store("Saves/PS/a.srm", &[3; 100_000], secs_now() - HOUR);
    let r = run(root, &mut remote);
    assert_eq!(r.conflicts, ["Saves/PS/a.srm"]);
    assert_eq!(read(root, "Saves/PS/a.srm").unwrap(), vec![3; 100_000]);
    let names: Vec<String> = std::fs::read_dir(root.join("Saves/PS"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names.len(),
        2,
        "the card's copy is kept beside it: {names:?}"
    );
}

#[test]
fn saves_go_first_then_the_smallest_games() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/PS/Big (USA).chd", &[0; 5000], HOUR);
    write_aged(root, "Games/WS/Small.ws", &[0; 10], HOUR);
    write_aged(root, "Games/32X/Middle.32x", &[0; 500], HOUR);
    write_aged(root, "Saves/WS/Small.sav", b"save", HOUR);
    let mut remote = Memory::default();
    run(root, &mut remote);
    let order: Vec<&str> = remote
        .puts
        .iter()
        .map(|p| p.rsplit('/').next().unwrap())
        .collect();
    assert_eq!(
        order,
        [
            ".Small.sav.cloud-part",
            ".Small.ws.cloud-part",
            ".Middle.32x.cloud-part",
            ".Big (USA).chd.cloud-part"
        ]
    );
}

#[test]
fn a_file_the_server_refuses_is_skipped_and_the_rest_still_go() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/PS/Huge.chd", &[0; 900], HOUR);
    write_aged(root, "Games/PS/Huge 2.chd", &[0; 1000], HOUR);
    write_aged(root, "Games/WSC/After.wsc", &[0; 950], HOUR);
    let mut remote = Memory {
        refuse: Some("Huge.chd"),
        ..Memory::default()
    };
    let r = run(root, &mut remote);
    assert_eq!(r.uploaded, 2, "{r:?}");
    assert_eq!(r.failed.len(), 1);
    assert_eq!(r.failed[0].0, "Games/PS/Huge.chd");
    assert!(r.failed[0].1.contains("413"), "{:?}", r.failed);
    assert!(remote.bytes("Games/WSC/After.wsc").is_some());

    remote.refuse = None;
    let r = run(root, &mut remote);
    assert_eq!((r.uploaded, r.failed.len()), (1, 0), "tried again: {r:?}");
}

#[test]
fn a_server_that_stops_answering_ends_the_run() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Games/PS/a.chd", &[0; 10], HOUR);
    write_aged(root, "Games/PS/b.chd", &[0; 20], HOUR);
    let mut remote = Memory {
        refuse: Some("a.chd"),
        goes_down: true,
        ..Memory::default()
    };
    let e = sync(
        root,
        &root.join("Config/cloudsync.manifest"),
        &mut remote,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(e.offline, "the server is gone, not the file: {e}");
    assert!(remote.bytes("Games/PS/b.chd").is_none());
}

#[test]
fn the_tally_counts_the_files_to_copy_and_the_bytes_of_each() {
    let card = tempfile::tempdir().unwrap();
    let root = card.path();
    write_aged(root, "Saves/GBA/Zelda.sav", b"save", HOUR);
    write_aged(root, "Games/GBA/Zelda.gba", &[0; 300], HOUR);
    let tally = Arc::new(Tally::default());
    let mut remote = Memory {
        tally: Some(tally.clone()),
        ..Memory::default()
    };
    remote.store("Games/GB/Tetris.gb", &[1; 50], 1);
    let manifest = root.join("Config/cloudsync.manifest");
    let no = AtomicBool::new(false);
    sync_counted(
        root,
        &manifest,
        &mut remote,
        &FOLDERS,
        &no,
        &|_| false,
        &tally,
    )
    .unwrap();
    let seen: Vec<_> = remote
        .moments
        .iter()
        .map(|m| (m.files, m.moved, m.name.as_str(), m.sending, m.at, m.of))
        .collect();
    assert_eq!(
        seen,
        [
            (3, 0, "Saves/GBA/Zelda.sav", true, 4, 4),
            (3, 2, "Games/GBA/Zelda.gba", true, 300, 300),
        ],
        "the download of Tetris came between them"
    );
    let end = tally.read();
    assert_eq!((end.files, end.moved, end.name.as_str()), (3, 3, ""));

    sync_counted(
        root,
        &manifest,
        &mut remote,
        &FOLDERS,
        &no,
        &|_| false,
        &tally,
    )
    .unwrap();
    assert_eq!(tally.read().files, 0, "nothing left to copy");
}

#[test]
fn devices_check_in_and_see_each_other() {
    use slot_cloud::devices::{check_in, clean_space, Device};
    let mut remote = Memory::default();
    let brick = Device {
        id: "brick-1a2b".into(),
        name: "TrimUI Brick".into(),
        space: String::new(),
        seen: 10,
    };
    let rgsp = Device {
        id: "rgsp-3c4d".into(),
        name: "RG SP".into(),
        space: "Spaces/rgsp-3c4d".into(),
        seen: 20,
    };
    assert_eq!(
        check_in(&mut remote, &brick).unwrap(),
        std::slice::from_ref(&brick)
    );
    let all = check_in(&mut remote, &rgsp).unwrap();
    assert_eq!(all, [rgsp.clone(), brick.clone()]);
    assert!(
        remote.dirs.contains("Spaces/rgsp-3c4d"),
        "its space is made"
    );
    assert_eq!(
        String::from_utf8_lossy(remote.bytes("Devices/brick-1a2b.ini").unwrap()),
        "name=TrimUI Brick\nspace=\nseen=10\n"
    );

    assert_eq!(clean_space("Spaces/../x"), "");
    assert_eq!(clean_space("/Spaces/rg-ds/"), "Spaces/rg-ds");
    assert_eq!(brick.own_space(""), "Spaces/brick-1a2b");
    assert_eq!(brick.own_space("Spaces/brick-1a2b"), "Spaces/brick-1a2b-10");
}
