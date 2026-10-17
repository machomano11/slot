//! Slot syncing on its own from the game list. Needs a WebDAV server: set
//! SLOT_WEBDAV_URL (an empty folder), SLOT_WEBDAV_USER and
//! SLOT_WEBDAV_PASSWORD, then run with `--ignored`.

mod common;

use std::time::{Duration, Instant, SystemTime};

use slot::app::Phase;

#[test]
#[ignore = "needs a WebDAV server"]
fn the_game_list_sends_a_save_and_says_so() {
    let url = std::env::var("SLOT_WEBDAV_URL").expect("SLOT_WEBDAV_URL");
    let user = std::env::var("SLOT_WEBDAV_USER").unwrap_or_default();
    let pw = std::env::var("SLOT_WEBDAV_PASSWORD").unwrap_or_default();
    let d = common::tmp_root_with_carts(&["Advance", "Boktai"]);
    let root = d.path();
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/cloudsync.ini"),
        format!("url={url}\nusername={user}\npassword={pw}\n"),
    )
    .unwrap();
    let save = root.join("Saves/GBA/Advance.sav");
    std::fs::create_dir_all(save.parent().unwrap()).unwrap();
    std::fs::write(&save, b"progress").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&save)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(60))
        .unwrap();

    let (mut session, _motor) = common::session_with_platform(root);
    // Two carts and nothing left running: Slot opens on the game list.
    assert!(matches!(session.app().phase(), Phase::Shelf));
    let started = Instant::now();
    let banner = loop {
        session.update(1.0 / 60.0);
        if let Some(m) = session.app_mut().take_message() {
            break m;
        }
        assert!(started.elapsed() < Duration::from_secs(20), "no banner");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(banner, ("Cloud sync".into(), "Sent 1 file".into()));
    assert!(
        std::fs::read_to_string(root.join("Config/cloudsync.manifest"))
            .unwrap()
            .contains("Saves/GBA/Advance.sav")
    );
}

#[test]
fn games_that_arrive_join_the_game_list_without_moving_it() {
    let d = common::tmp_root_with_carts(&["Advance", "Mario"]);
    let root = d.path();
    let (mut session, _motor) = common::session_with_platform(root);
    let app = session.app_mut();
    assert!(matches!(app.phase(), Phase::Shelf));
    let before = app.selected_stem().map(str::to_string);
    let stems = |app: &slot::app::App| {
        let mut s: Vec<String> = app.carts().map(|c| c.stem.clone()).collect();
        s.sort();
        s
    };
    assert_eq!(stems(app), ["Advance", "Mario"]);

    std::fs::copy(
        root.join("Games/GBA/Advance.gba"),
        root.join("Games/GBA/Boktai.gba"),
    )
    .unwrap();
    let waiting = app.take_in_synced(&["Games/GBA/Boktai.gba".to_string()]);
    assert!(!waiting, "on the game list it is read straight away");
    assert_eq!(stems(app), ["Advance", "Boktai", "Mario"]);
    assert_eq!(app.selected_stem().map(str::to_string), before);
}
