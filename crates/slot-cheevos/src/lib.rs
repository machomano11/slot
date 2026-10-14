//! RetroAchievements through rcheevos' rc_client.
//!
//! rc_client does the bookkeeping: it hashes the ROM, fetches the game's
//! achievements, watches memory once a frame and reports unlocks. This crate
//! gives it the three things it needs from slot: an HTTP client, the running
//! core's memory, and somewhere to send what happened ([`Event`]).
//!
//! There is one client per process, created by [`Cheevos::start`].

use std::ffi::{c_char, c_int, c_uint, c_void, CStr, CString};
use std::io::Read;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

/// rcheevos console ids for the systems slot runs.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Console {
    GameBoy = 4,
    GameBoyAdvance = 5,
    GameBoyColor = 6,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    /// `token` replaces the password for later logins.
    LoggedIn {
        name: String,
        token: String,
        score: u32,
    },
    LoginFailed(String),
    GameLoaded {
        title: String,
        unlocked: u32,
        total: u32,
    },
    /// The ROM is unknown to RetroAchievements, or the lookup failed.
    GameFailed(String),
    Unlocked {
        id: u32,
        title: String,
        description: String,
        points: u32,
    },
    Mastered {
        title: String,
    },
    LeaderboardSubmitted {
        title: String,
        score: String,
    },
    ServerError(String),
    /// Unlocks are queued and will be retried.
    Disconnected,
    Reconnected,
    /// rc_client wants the game restarted (hardcore was switched on).
    Reset,
}

#[repr(C)]
struct Hooks {
    http: extern "C" fn(*const c_char, *const c_char, *const c_char, *mut c_void),
    event: extern "C" fn(c_int, u32, u32, *const c_char, *const c_char),
    log: extern "C" fn(*const c_char),
}

/// `retro_get_memory_data` and `retro_get_memory_size` of the loaded core.
pub type MemoryData = unsafe extern "C" fn(c_uint) -> *mut c_void;
pub type MemorySize = unsafe extern "C" fn(c_uint) -> usize;

extern "C" {
    fn slot_ch_create(hooks: *const Hooks, host: *const c_char) -> c_int;
    fn slot_ch_user_agent(buffer: *mut c_char, size: usize) -> usize;
    fn slot_ch_http_done(req: *mut c_void, status: c_int, body: *const c_char, len: usize);
    fn slot_ch_set_hardcore(on: c_int);
    fn slot_ch_hardcore() -> c_int;
    fn slot_ch_login_password(user: *const c_char, password: *const c_char);
    fn slot_ch_login_token(user: *const c_char, token: *const c_char);
    fn slot_ch_memory_size() -> u32;
    fn slot_ch_memory_at(address: u32) -> *mut u8;
    fn slot_ch_set_memory(
        mmap: *const c_void,
        data: MemoryData,
        size: MemorySize,
        console: u32,
    ) -> u32;
    fn slot_ch_load_game(console: u32, rom: *const u8, len: usize);
    fn slot_ch_unload_game();
    fn slot_ch_forget_memory();
    fn slot_ch_do_frame();
    fn slot_ch_idle();
    fn slot_ch_reset();
    fn slot_ch_progress_size() -> usize;
    fn slot_ch_save_progress(buffer: *mut u8, size: usize) -> c_int;
    fn slot_ch_load_progress(buffer: *const u8, size: usize) -> c_int;
}

const LOGIN_OK: c_int = 1;
const LOGIN_FAILED: c_int = 2;
const GAME_LOADED: c_int = 3;
const GAME_FAILED: c_int = 4;
const UNLOCKED: c_int = 5;
const MASTERED: c_int = 6;
const SERVER_ERROR: c_int = 7;
const DISCONNECTED: c_int = 8;
const RECONNECTED: c_int = 9;
const RESET: c_int = 10;
const LEADERBOARD_SUBMITTED: c_int = 11;

/// The HTTP status rc_client reads as "no response at all", so it retries.
const NO_RESPONSE: c_int = -1;

struct Shared {
    events: Mutex<Sender<Event>>,
    user_agent: String,
}

static SHARED: OnceLock<Shared> = OnceLock::new();

fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

extern "C" fn on_event(kind: c_int, a: u32, b: u32, title: *const c_char, detail: *const c_char) {
    let (title, detail) = (text(title), text(detail));
    let event = match kind {
        LOGIN_OK => Event::LoggedIn {
            name: title,
            token: detail,
            score: a.max(b),
        },
        LOGIN_FAILED => Event::LoginFailed(detail),
        GAME_LOADED => Event::GameLoaded {
            title,
            unlocked: a,
            total: b,
        },
        GAME_FAILED => Event::GameFailed(detail),
        UNLOCKED => Event::Unlocked {
            id: a,
            title,
            description: detail,
            points: b,
        },
        MASTERED => Event::Mastered { title },
        LEADERBOARD_SUBMITTED => Event::LeaderboardSubmitted {
            title,
            score: detail,
        },
        SERVER_ERROR => Event::ServerError(format!("{title}: {detail}")),
        DISCONNECTED => Event::Disconnected,
        RECONNECTED => Event::Reconnected,
        RESET => Event::Reset,
        _ => return,
    };
    if let Some(shared) = SHARED.get() {
        if let Ok(tx) = shared.events.lock() {
            let _ = tx.send(event);
        }
    }
}

extern "C" fn on_log(message: *const c_char) {
    eprintln!("slot: cheevos: {}", text(message));
}

struct Request(*mut c_void);
// The request handle is only touched again by slot_ch_http_done, once.
unsafe impl Send for Request {}

extern "C" fn on_http(
    url: *const c_char,
    post: *const c_char,
    content_type: *const c_char,
    req: *mut c_void,
) {
    let (url, post, content_type) = (text(url), text(post), text(content_type));
    let req = Request(req);
    let agent = SHARED
        .get()
        .map(|s| s.user_agent.clone())
        .unwrap_or_default();
    std::thread::Builder::new()
        .name("slot-cheevos-http".into())
        .spawn(move || {
            let req = req;
            let (status, body) = fetch(&url, &post, &content_type, &agent);
            unsafe { slot_ch_http_done(req.0, status, body.as_ptr() as *const c_char, body.len()) };
        })
        .expect("spawn the RetroAchievements request thread");
}

fn fetch(url: &str, post: &str, content_type: &str, agent_name: &str) -> (c_int, Vec<u8>) {
    let call = slot_net::agent()
        .request(if post.is_empty() { "GET" } else { "POST" }, url)
        .set("User-Agent", agent_name);
    let sent = if post.is_empty() {
        call.call()
    } else {
        let content_type = if content_type.is_empty() {
            "application/x-www-form-urlencoded"
        } else {
            content_type
        };
        call.set("Content-Type", content_type).send_string(post)
    };
    let response = match sent {
        Ok(r) => r,
        Err(slot_net::ureq::Error::Status(_, r)) => r,
        Err(e) => {
            eprintln!("slot: cheevos: request failed: {e}");
            return (NO_RESPONSE, Vec::new());
        }
    };
    let status = c_int::from(response.status());
    let mut body = Vec::new();
    if let Err(e) = response.into_reader().take(8 << 20).read_to_end(&mut body) {
        eprintln!("slot: cheevos: reading the response: {e}");
        return (NO_RESPONSE, Vec::new());
    }
    (status, body)
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// The process-wide RetroAchievements client.
pub struct Cheevos {
    events: Mutex<Receiver<Event>>,
}

impl Cheevos {
    /// Creates the client. `client` names this frontend in the User-Agent,
    /// such as "slot/1.6.0". Returns None if a client already exists.
    pub fn start(client: &str, host: Option<&str>) -> Option<Cheevos> {
        let (tx, rx) = channel();
        let hooks = Hooks {
            http: on_http,
            event: on_event,
            log: on_log,
        };
        let host = host.map(cstring);
        if SHARED.get().is_some() {
            return None;
        }
        let created = unsafe {
            slot_ch_create(
                &hooks,
                host.as_ref().map_or(std::ptr::null(), |h| h.as_ptr()),
            )
        };
        if created == 0 {
            return None;
        }
        let mut clause = vec![0u8; 128];
        let n = unsafe { slot_ch_user_agent(clause.as_mut_ptr() as *mut c_char, clause.len()) };
        clause.truncate(n.min(clause.len()));
        let user_agent = format!("{client} {}", String::from_utf8_lossy(&clause))
            .trim()
            .to_string();
        SHARED
            .set(Shared {
                events: Mutex::new(tx),
                user_agent,
            })
            .ok()?;
        Some(Cheevos {
            events: Mutex::new(rx),
        })
    }

    pub fn user_agent(&self) -> String {
        SHARED
            .get()
            .map(|s| s.user_agent.clone())
            .unwrap_or_default()
    }

    pub fn login_with_password(&self, user: &str, password: &str) {
        let (user, password) = (cstring(user), cstring(password));
        unsafe { slot_ch_login_password(user.as_ptr(), password.as_ptr()) };
    }

    pub fn login_with_token(&self, user: &str, token: &str) {
        let (user, token) = (cstring(user), cstring(token));
        unsafe { slot_ch_login_token(user.as_ptr(), token.as_ptr()) };
    }

    pub fn set_hardcore(&self, on: bool) {
        unsafe { slot_ch_set_hardcore(c_int::from(on)) };
    }

    pub fn hardcore(&self) -> bool {
        unsafe { slot_ch_hardcore() != 0 }
    }

    /// Hashes `rom` and fetches its achievements; answers with
    /// [`Event::GameLoaded`] or [`Event::GameFailed`].
    pub fn load_game(&self, console: Console, rom: &[u8]) {
        unsafe { slot_ch_load_game(console as u32, rom.as_ptr(), rom.len()) };
    }

    /// [`Cheevos::load_game`] for a ROM still on the card: it is read on a
    /// thread of its own so a large GBA ROM never holds up a frame.
    pub fn load_game_file(&self, console: Console, rom: std::path::PathBuf) {
        let spawned = std::thread::Builder::new()
            .name("slot-cheevos-hash".into())
            .spawn(move || match std::fs::read(&rom) {
                Ok(bytes) => unsafe {
                    slot_ch_load_game(console as u32, bytes.as_ptr(), bytes.len())
                },
                Err(e) => eprintln!("slot: cheevos: reading {}: {e}", rom.display()),
            });
        if let Err(e) = spawned {
            eprintln!("slot: cheevos: {e}");
        }
    }

    pub fn unload_game(&self) {
        unsafe { slot_ch_unload_game() };
    }

    /// Everything that happened since the last call.
    pub fn events(&self) -> Vec<Event> {
        match self.events.lock() {
            Ok(rx) => rx.try_iter().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Lets rc_client do its housekeeping while no frames run (paused, menus).
    pub fn idle(&self) {
        unsafe { slot_ch_idle() };
    }
}

/// The part of the client the emulator thread drives: it sees the core's
/// memory, so it is only ever used from that thread.
pub struct FrameWatch {
    _private: (),
}

impl FrameWatch {
    /// # Safety
    /// `mmap` is null or points at a `retro_memory_map` that stays valid, with
    /// the core loaded, until [`FrameWatch::forget`]; `data` and `size` are the
    /// loaded core's own `retro_get_memory_data` and `retro_get_memory_size`.
    pub unsafe fn attach(
        mmap: *const c_void,
        data: MemoryData,
        size: MemorySize,
        console: Console,
    ) -> (FrameWatch, u32) {
        let regions = slot_ch_set_memory(mmap, data, size, console as u32);
        (FrameWatch { _private: () }, regions)
    }

    /// Call after every emulated frame the player sees, not the speculative
    /// ones run-ahead throws away.
    /// How many bytes of the console's achievement address space are mapped.
    pub fn mapped_size(&self) -> u32 {
        unsafe { slot_ch_memory_size() }
    }

    /// Where an achievement address lives in the core's memory, if mapped.
    pub fn host_address(&self, address: u32) -> Option<*mut u8> {
        let p = unsafe { slot_ch_memory_at(address) };
        (!p.is_null()).then_some(p)
    }

    pub fn frame(&mut self) {
        unsafe { slot_ch_do_frame() };
    }

    /// The game was reset or jumped to another point (a state load, rewind):
    /// partly met conditions start over.
    pub fn reset(&mut self) {
        unsafe { slot_ch_reset() };
    }

    pub fn save_progress(&mut self) -> Option<Vec<u8>> {
        let size = unsafe { slot_ch_progress_size() };
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size];
        (unsafe { slot_ch_save_progress(buf.as_mut_ptr(), size) } == 0).then_some(buf)
    }

    pub fn load_progress(&mut self, progress: &[u8]) -> bool {
        unsafe { slot_ch_load_progress(progress.as_ptr(), progress.len()) == 0 }
    }
}

impl Drop for FrameWatch {
    fn drop(&mut self) {
        unsafe { slot_ch_forget_memory() };
    }
}
