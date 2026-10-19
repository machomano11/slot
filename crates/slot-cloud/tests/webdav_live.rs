//! Against a real WebDAV server: set SLOT_WEBDAV_URL (an empty folder),
//! SLOT_WEBDAV_USER and SLOT_WEBDAV_PASSWORD, then
//! `cargo test -p slot-cloud --test webdav_live -- --ignored`.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use slot_cloud::{sync, WebDav};

/// A folder of its own for each test, under the one named.
fn dav(sub: &str) -> WebDav {
    let url = std::env::var("SLOT_WEBDAV_URL").expect("SLOT_WEBDAV_URL");
    let url = format!("{}/{sub}", url.trim_end_matches('/'));
    let user = std::env::var("SLOT_WEBDAV_USER").ok();
    let pw = std::env::var("SLOT_WEBDAV_PASSWORD").ok();
    WebDav::new(&url, user.as_deref(), pw.as_deref())
}

fn aged(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(60))
        .unwrap();
}

#[test]
#[ignore = "needs a WebDAV server"]
fn two_cards_meet_through_the_server() {
    let no = AtomicBool::new(false);
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let save = "Saves/GBA/Pokémon Emerald (U) [!].sav";
    aged(&a.path().join(save), b"from a");
    aged(
        &a.path()
            .join("States/GBA/mgba/Pokémon Emerald (U) [!]/resume.state"),
        b"state",
    );

    let r = sync(
        a.path(),
        &a.path().join("m"),
        &mut dav("two_cards_meet_through_the_server"),
        &no,
    )
    .unwrap();
    assert_eq!(r.uploaded, 2, "{r:?}");
    let r = sync(
        b.path(),
        &b.path().join("m"),
        &mut dav("two_cards_meet_through_the_server"),
        &no,
    )
    .unwrap();
    assert_eq!(r.downloaded, 2, "{r:?}");
    assert_eq!(std::fs::read(b.path().join(save)).unwrap(), b"from a");

    aged(&b.path().join(save), b"from b, longer");
    let r = sync(
        b.path(),
        &b.path().join("m"),
        &mut dav("two_cards_meet_through_the_server"),
        &no,
    )
    .unwrap();
    assert_eq!(r.uploaded, 1, "{r:?}");
    let r = sync(
        a.path(),
        &a.path().join("m"),
        &mut dav("two_cards_meet_through_the_server"),
        &no,
    )
    .unwrap();
    assert_eq!(r.downloaded, 1, "{r:?}");
    assert_eq!(
        std::fs::read(a.path().join(save)).unwrap(),
        b"from b, longer"
    );

    let r = sync(
        a.path(),
        &a.path().join("m"),
        &mut dav("two_cards_meet_through_the_server"),
        &no,
    )
    .unwrap();
    assert!(!r.changed(), "{r:?}");
}

#[test]
#[ignore = "needs a WebDAV server"]
fn a_wrong_password_says_so() {
    let url = std::env::var("SLOT_WEBDAV_URL").expect("SLOT_WEBDAV_URL");
    let mut dav = WebDav::new(&url, Some("nobody"), Some("wrong"));
    let card = tempfile::tempdir().unwrap();
    let e = sync(
        card.path(),
        &card.path().join("m"),
        &mut dav,
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(!e.offline);
    assert!(e.message.contains("username or password"), "{e}");
}

#[test]
#[ignore = "needs a WebDAV server"]
fn a_cd_sized_game_goes_up_and_comes_down_whole() {
    let no = AtomicBool::new(false);
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let game = "Games/PS/Big Game (USA).chd";
    let bytes: Vec<u8> = (0..64 << 20).map(|i: u32| ((i * 7) >> 3) as u8).collect();
    aged(&a.path().join(game), &bytes);
    aged(&a.path().join("Labels/PS/Big Game (USA).png"), b"art");

    let r = sync(
        a.path(),
        &a.path().join("m"),
        &mut dav("a_cd_sized_game_goes_up_and_comes_down_whole"),
        &no,
    )
    .unwrap();
    assert_eq!(r.uploaded, 2, "{r:?}");
    let r = sync(
        b.path(),
        &b.path().join("m"),
        &mut dav("a_cd_sized_game_goes_up_and_comes_down_whole"),
        &no,
    )
    .unwrap();
    assert_eq!(r.downloaded, 2, "{r:?}");
    assert!(std::fs::read(b.path().join(game)).unwrap() == bytes);

    let r = sync(
        a.path(),
        &a.path().join("m"),
        &mut dav("a_cd_sized_game_goes_up_and_comes_down_whole"),
        &no,
    )
    .unwrap();
    assert!(!r.changed(), "{r:?}");
}

#[test]
#[ignore = "needs a WebDAV server"]
fn devices_check_in_and_sync_in_their_own_space() {
    use slot_cloud::devices::{check_in, Device};
    use slot_cloud::Account;
    let url = std::env::var("SLOT_WEBDAV_URL").expect("SLOT_WEBDAV_URL");
    let mut ini = std::collections::HashMap::new();
    ini.insert(
        "url".to_string(),
        format!("{}/spaces", url.trim_end_matches('/')),
    );
    for (k, v) in [
        ("username", "SLOT_WEBDAV_USER"),
        ("password", "SLOT_WEBDAV_PASSWORD"),
    ] {
        if let Ok(v) = std::env::var(v) {
            ini.insert(k.to_string(), v);
        }
    }
    let account = Account::from_ini(&ini).unwrap();
    let me = Device {
        id: "brick-1a2b".into(),
        name: "TrimUI Brick".into(),
        space: "Spaces/brick-1a2b".into(),
        seen: 7,
    };
    let them = Device {
        id: "rgds-9f".into(),
        name: "Anbernic RG DS".into(),
        ..Device::default()
    };
    check_in(&mut account.remote(), &them).unwrap();
    let all = check_in(&mut account.remote(), &me).unwrap();
    assert_eq!(all, [me.clone(), them]);

    let card = tempfile::tempdir().unwrap();
    aged(&card.path().join("Saves/GBA/Zelda.sav"), b"mine");
    let r = sync(
        card.path(),
        &card.path().join("m"),
        &mut account.remote_in(&me.space),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(r.uploaded, 1);
    let mut top = account.remote();
    use slot_cloud::Remote;
    assert!(top
        .stat("Spaces/brick-1a2b/Saves/GBA/Zelda.sav")
        .unwrap()
        .is_some());
    assert!(
        top.stat("Saves/GBA/Zelda.sav").unwrap().is_none(),
        "not in the top space"
    );
}
