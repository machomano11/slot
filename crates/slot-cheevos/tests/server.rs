//! Talks to the real RetroAchievements server, so it only runs on request:
//! cargo test -p slot-cheevos -- --ignored

use std::time::{Duration, Instant};

use slot_cheevos::{Cheevos, Event};

#[test]
#[ignore = "needs the network"]
fn a_wrong_password_comes_back_as_a_failed_login() {
    let ch = Cheevos::start("slot-test/0", None).expect("one client per process");
    assert!(
        ch.user_agent().starts_with("slot-test/0 rcheevos/"),
        "{}",
        ch.user_agent()
    );
    ch.login_with_password("slot-cfw-test-nobody", "not-the-password");
    let began = Instant::now();
    loop {
        if let Some(e) = ch.events().into_iter().next() {
            assert!(matches!(e, Event::LoginFailed(_)), "{e:?}");
            eprintln!("{e:?}");
            return;
        }
        assert!(
            began.elapsed() < Duration::from_secs(40),
            "no answer from the server"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}
