//! RetroAchievements: signs in with the account in [`ACCOUNT_FILE`], follows
//! the game being played and turns what the server says into banners.
//!
//! Nothing happens without that file, so a card without an account behaves
//! exactly as before.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slot_cheevos::{Cheevos, Console, Event};
use slot_store::{ini, Platform};

use crate::app::App;

/// `username` plus either `password` or `token`. After the first sign-in the
/// password is swapped for the token the server hands back.
pub const ACCOUNT_FILE: &str = "Config/retroachievements.ini";

const IDLE_EVERY: Duration = Duration::from_secs(1);

pub struct Achievements {
    root: PathBuf,
    cheevos: Cheevos,
    signed_in: bool,
    waiting: Option<(Console, PathBuf)>,
    playing: bool,
    offline_said: bool,
    idled: Instant,
}

pub fn console_for(platform: Platform) -> Console {
    match platform {
        Platform::Gb => Console::GameBoy,
        Platform::Gbc => Console::GameBoyColor,
        Platform::Gba => Console::GameBoyAdvance,
    }
}

impl Achievements {
    pub fn boot(root: &Path) -> Option<Achievements> {
        let account = ini::read(root, ACCOUNT_FILE);
        let get = |key: &str| {
            account
                .get(key)
                .map(String::as_str)
                .filter(|v| !v.is_empty())
        };
        let user = get("username")?;
        if matches!(get("enabled"), Some("0" | "false" | "off" | "no")) {
            return None;
        }
        let client = format!("slot/{}", crate::build_info::Build::current().version);
        let cheevos = Cheevos::start(&client, get("host"))?;
        match (get("token"), get("password")) {
            (Some(token), _) => cheevos.login_with_token(user, token),
            (None, Some(password)) => cheevos.login_with_password(user, password),
            (None, None) => {
                eprintln!("slot: cheevos: {ACCOUNT_FILE} has no password or token");
                return None;
            }
        }
        eprintln!(
            "slot: cheevos: signing in as {user} ({})",
            cheevos.user_agent()
        );
        Some(Achievements {
            root: root.to_path_buf(),
            cheevos,
            signed_in: false,
            waiting: None,
            playing: false,
            offline_said: false,
            idled: Instant::now(),
        })
    }

    /// A cart went in. The game is looked up as soon as the sign-in is done.
    pub fn game_started(&mut self, rom: &Path, platform: Platform) {
        self.game_ended();
        self.playing = true;
        let console = console_for(platform);
        match self.signed_in {
            true => self.cheevos.load_game_file(console, rom.to_path_buf()),
            false => self.waiting = Some((console, rom.to_path_buf())),
        }
    }

    pub fn game_ended(&mut self) {
        if self.playing {
            self.cheevos.unload_game();
        }
        self.playing = false;
        self.waiting = None;
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn poll(&mut self, app: &mut App) {
        if self.idled.elapsed() >= IDLE_EVERY {
            self.idled = Instant::now();
            self.cheevos.idle();
        }
        for event in self.cheevos.events() {
            self.handle(event, app);
        }
    }

    fn handle(&mut self, event: Event, app: &mut App) {
        match event {
            Event::LoggedIn { name, token, score } => {
                eprintln!("slot: cheevos: signed in as {name}, {score} points");
                self.signed_in = true;
                self.keep_token(&token);
                app.show_message("RetroAchievements", format!("Signed in as {name}"));
                if let Some((console, rom)) = self.waiting.take() {
                    self.cheevos.load_game_file(console, rom);
                }
            }
            Event::LoginFailed(why) => {
                eprintln!("slot: cheevos: sign-in failed: {why}");
                app.show_message("RetroAchievements", format!("Sign-in failed: {why}"));
            }
            Event::GameLoaded {
                title,
                unlocked,
                total,
            } => {
                eprintln!("slot: cheevos: {title}: {unlocked} of {total} unlocked");
                if total > 0 {
                    app.show_message(
                        title,
                        format!("{unlocked} of {total} achievements unlocked"),
                    );
                }
            }
            Event::GameFailed(why) => eprintln!("slot: cheevos: no achievements: {why}"),
            Event::Unlocked {
                title,
                description,
                points,
                ..
            } => {
                eprintln!("slot: cheevos: unlocked {title} ({points})");
                app.show_message(
                    format!("Unlocked: {title}"),
                    format!("{description} ({points} points)"),
                );
            }
            Event::Mastered { title } => app.show_message("Game mastered!", title),
            Event::LeaderboardSubmitted { title, score } => app.show_message(
                format!("Leaderboard: {title}"),
                format!("Submitted {score}"),
            ),
            Event::ServerError(why) => eprintln!("slot: cheevos: server: {why}"),
            Event::Disconnected => {
                eprintln!("slot: cheevos: offline, unlocks are queued");
                if !self.offline_said {
                    self.offline_said = true;
                    app.show_message("RetroAchievements", "Offline: unlocks will be sent later");
                }
            }
            Event::Reconnected => {
                eprintln!("slot: cheevos: back online");
                if self.offline_said {
                    self.offline_said = false;
                    app.show_message("RetroAchievements", "Back online: unlocks sent");
                }
            }
            Event::Reset => {}
        }
    }

    /// Swaps a saved password for the token, so the password does not stay on
    /// the card.
    fn keep_token(&self, token: &str) {
        if token.is_empty() {
            return;
        }
        let path = self.root.join(ACCOUNT_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let updated =
            ini::set(&text, "token", Some(token)).and_then(|t| ini::set(&t, "password", None));
        match updated {
            Ok(t) if t != text => {
                if let Err(e) = slot_store::atomic_write(&path, t.as_bytes()) {
                    eprintln!("slot: cheevos: saving the token: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("slot: cheevos: saving the token: {e}"),
        }
    }
}
