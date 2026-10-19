//! Brick Wi-Fi settings. Uses the firmware supplicant; no shell interpolation of credentials.
use crate::cart_art::{ArtJob, Fetched, Progress};
use slot_input::{Action, Btn};
use slot_store::Platform;
use slot_ui::{TexId, UndoFace};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, net::UnixDatagram},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
static SERIAL: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Security {
    Open,
    Personal,
    Unsupported,
}
#[derive(Clone, Debug)]
struct Network {
    ssid: Vec<u8>,
    signal: i32,
    security: Security,
    saved: Option<u32>,
}
fn decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut v = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            if b[i + 1] == b'x' && i + 3 < b.len() {
                if let Some(n) = std::str::from_utf8(&b[i + 2..i + 4])
                    .ok()
                    .and_then(|s| u8::from_str_radix(s, 16).ok())
                {
                    v.push(n);
                    i += 4;
                    continue;
                }
            }
            let c = match b[i + 1] {
                b'\\' => Some(b'\\'),
                b'"' => Some(b'"'),
                b'n' => Some(b'\n'),
                b't' => Some(b'\t'),
                b'r' => Some(b'\r'),
                _ => None,
            };
            if let Some(c) = c {
                v.push(c);
                i += 2;
                continue;
            }
        }
        v.push(b[i]);
        i += 1;
    }
    v
}
fn name(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}
fn psk(s: &str) -> Result<String> {
    if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(s.into());
    }
    if !(8..=63).contains(&s.len()) || !s.bytes().all(|b| (32..=126).contains(&b)) {
        return Err("Use 8-63 characters or 64 hex digits".into());
    }
    // wpa_config_parse_psk takes the literal bytes between the first and last quote.
    // Backslashes and interior quotes must NOT be C-escaped here.
    Ok(format!("\"{s}\""))
}
fn scan_rows(scan: &str, saved: &str) -> Vec<Network> {
    let saved: Vec<_> = saved
        .lines()
        .skip(1)
        .filter_map(|s| {
            let v: Vec<_> = s.split('\t').collect();
            Some((v.first()?.parse::<u32>().ok()?, decode(v.get(1)?)))
        })
        .collect();
    let mut out: Vec<Network> = Vec::new();
    for s in scan.lines().skip(1) {
        let v: Vec<_> = s.splitn(5, '\t').collect();
        if v.len() != 5 {
            continue;
        }
        let ssid = decode(v[4]);
        if ssid.is_empty() || ssid.len() > 32 {
            continue;
        }
        let security = if v[3].contains("PSK") {
            Security::Personal
        } else if ["WPA", "WEP", "EAP", "SAE", "OWE", "RSN"]
            .iter()
            .any(|s| v[3].contains(s))
        {
            Security::Unsupported
        } else {
            Security::Open
        };
        let n = Network {
            saved: saved.iter().find(|(_, s)| s == &ssid).map(|(id, _)| *id),
            ssid,
            security,
            signal: v[2].parse().unwrap_or(-100),
        };
        if let Some(old) = out
            .iter_mut()
            .find(|a| a.ssid == n.ssid && a.security == n.security)
        {
            if old.signal < n.signal {
                *old = n
            }
        } else {
            out.push(n)
        }
    }
    out.sort_by(|a, b| b.signal.cmp(&a.signal).then(a.ssid.cmp(&b.ssid)));
    out
}
pub(crate) trait Supplicant {
    fn command(&self, s: &str) -> Result<String>;
    fn ok(&self, s: &str) -> Result<()> {
        match self.command(s)? {
            r if r == "OK" => Ok(()),
            r => {
                let verb = s.split_whitespace().next().unwrap_or("");
                Err(format!("Wi-Fi service rejected {verb} ({r})"))
            }
        }
    }
}
pub(crate) struct Control {
    socket: UnixDatagram,
    local: PathBuf,
}
impl Drop for Control {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.local);
    }
}
impl Control {
    pub(crate) fn at(remote: &Path) -> Result<Self> {
        let local = PathBuf::from(format!(
            "/tmp/slot-wifi-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let socket = UnixDatagram::bind(&local).map_err(|_| "Cannot open Wi-Fi control socket")?;
        let c = Self { socket, local };
        fs::set_permissions(&c.local, fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot secure Wi-Fi socket")?;
        c.socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| "Cannot set Wi-Fi timeout")?;
        c.socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .map_err(|_| "Cannot set Wi-Fi timeout")?;
        c.socket
            .connect(remote)
            .map_err(|_| "Wi-Fi service unavailable")?;
        Ok(c)
    }
    fn open() -> Result<Self> {
        [
            "/etc/wifi/sockets/wlan0",
            "/var/run/wpa_supplicant/wlan0",
            "/run/wpa_supplicant/wlan0",
        ]
        .iter()
        .find_map(|s| Self::at(Path::new(s)).ok())
        .ok_or("Wi-Fi service unavailable".into())
    }
}
impl Supplicant for Control {
    fn command(&self, command: &str) -> Result<String> {
        self.socket
            .send(command.as_bytes())
            .map_err(|_| "Wi-Fi command failed")?;
        let mut b = [0; 32768];
        let n = self
            .socket
            .recv(&mut b)
            .map_err(|_| "Wi-Fi service did not respond")?;
        let s = String::from_utf8_lossy(&b[..n])
            .trim_end_matches('\n')
            .to_string();
        if s.starts_with("FAIL") || s.starts_with("UNKNOWN") {
            // Only the command word: arguments can carry a password.
            let verb = command.split_whitespace().next().unwrap_or("");
            eprintln!("slot: wifi: {verb} -> {s}");
            Err(format!("Wi-Fi service rejected {verb} ({s})"))
        } else {
            Ok(s)
        }
    }
}
/// Asks for a scan and returns what was found. Right after the radio comes
/// up the supplicant answers SCAN with FAIL until the interface is ready, and
/// with FAIL-BUSY while a scan of its own runs; both are waited out. If no
/// scan of ours starts, its latest results from its own scans are used.
fn scan(c: &impl Supplicant, cancel: &AtomicBool, settle_ms: u64) -> Result<Vec<Network>> {
    let mut refused = None;
    for _ in 0..12 {
        match c.ok("SCAN") {
            Ok(()) => {
                refused = None;
                break;
            }
            Err(e) if e.contains("FAIL-BUSY") => {
                refused = None;
                break;
            }
            Err(e) if e.starts_with("Wi-Fi service rejected") => {
                refused = Some(e);
                pause(cancel, 500)?;
            }
            Err(e) => return Err(e),
        }
    }
    if refused.is_none() {
        pause(cancel, settle_ms)?;
    }
    let networks = scan_rows(&c.command("SCAN_RESULTS")?, &c.command("LIST_NETWORKS")?);
    match refused {
        Some(e) if networks.is_empty() => {
            let state = c
                .command("STATUS")
                .ok()
                .and_then(|s| field(&s, "wpa_state").map(str::to_string))
                .unwrap_or_else(|| "unknown".into());
            eprintln!("slot: wifi: no scan: {e}, wpa_state={state}");
            Err(format!("Wi-Fi isn't ready ({state}). Try scanning again"))
        }
        _ => Ok(networks),
    }
}
fn field<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    s.lines()
        .find_map(|s| s.strip_prefix(key).and_then(|s| s.strip_prefix('=')))
}
fn pause(cancel: &AtomicBool, ms: u64) -> Result<()> {
    for _ in 0..ms.div_ceil(100) {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
/// slot carries its own helper (System/brick_wifi.sh on the Brick, System/wifi.sh on
/// the RG SP), and it wins: an /etc/wifi/wifi_init.sh on a Brick is left over from
/// another firmware and can call tools stock firmware lacks (rfkill.elf), leaving the
/// radio blocked.
fn helper(firmware: &Path, root: &Path) -> Option<PathBuf> {
    let own = ["System/brick_wifi.sh", "System/wifi.sh"]
        .iter()
        .map(|s| root.join(s))
        .find(|p| p.is_file());
    own.or_else(|| firmware.is_file().then(|| firmware.into()))
}
fn service(on: bool) -> Result<()> {
    run_helper(if on { "start" } else { "stop" }, Duration::from_secs(15))?;
    remember(&card_root(), on);
    Ok(())
}
/// Wi-Fi left on is switched on again at the next boot: BaseOS on the RG SP starts no
/// supplicant of its own (the Brick's firmware does).
fn remember(root: &Path, on: bool) {
    let mark = root.join(WIFI_ON);
    let done = if on {
        fs::create_dir_all(root.join("Config")).and_then(|_| fs::write(&mark, ""))
    } else {
        fs::remove_file(&mark).or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        })
    };
    if let Err(e) = done {
        eprintln!("slot: wifi: cannot remember the switch: {e}");
    }
}
const WIFI_ON: &str = "Config/wifi-on";
/// Joined but no address: have the helper start a DHCP client, or make the
/// running one ask again. A client that kept an old lease or backed off while the
/// supplicant was joining otherwise leaves the menu on "Waiting for IP".
/// At most once every 10 s, so the menu's 2 s checks don't keep restarting it.
fn request_address() {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if last.is_some_and(|t| t.elapsed() < Duration::from_secs(10)) {
        return;
    }
    *last = Some(Instant::now());
    drop(last);
    if let Err(e) = run_helper("dhcp", Duration::from_secs(5)) {
        eprintln!("slot: wifi: {e}");
    }
}
/// The firmware starts its own supplicant at boot, which joins a saved network
/// but runs no DHCP client, so Wi-Fi sat on "Waiting for IP" until it was
/// switched off and on. Watch it for a minute after boot and ask for an address
/// once it has joined. On the RG SP, Wi-Fi left on is first switched on again.
/// Never blocks startup.
pub fn ensure_address_at_boot() {
    thread::spawn(|| {
        // BaseOS loads the Wi-Fi driver in the background, so wlan0 can still be missing.
        if !cfg!(feature = "brick") && card_root().join(WIFI_ON).exists() {
            for _ in 0..4 {
                if Control::open().is_ok() {
                    break;
                }
                match service(true) {
                    Ok(()) => break,
                    Err(e) => eprintln!("slot: wifi: switching on at boot: {e}"),
                }
                thread::sleep(Duration::from_secs(5));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if let Ok(s) = Control::open().and_then(|c| c.command("STATUS")) {
                if field(&s, "wpa_state") == Some("COMPLETED") {
                    if field(&s, "ip_address").is_some() {
                        return;
                    }
                    request_address();
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
    });
}
fn run_helper(verb: &str, limit: Duration) -> Result<()> {
    let root =
        PathBuf::from(std::env::var_os("SLOT_ROOT").unwrap_or_else(|| "/mnt/SDCARD/Slot".into()));
    let Some(script) = helper(Path::new("/etc/wifi/wifi_init.sh"), &root) else {
        return Err("Wi-Fi helper missing: System/wifi.sh".into());
    };
    let mut child = Command::new("/bin/sh")
        .arg(&script)
        .arg(verb)
        .env("SLOT_ROOT", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Cannot start Wi-Fi helper")?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "Cannot check Wi-Fi helper")? {
            return if status.success() {
                Ok(())
            } else {
                Err("Could not switch Wi-Fi; see Logs/wifi.log".into())
            };
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Wi-Fi helper timed out; see Logs/wifi.log".into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}
fn connect(
    c: &impl Supplicant,
    n: &Network,
    password: Option<String>,
    cancel: &AtomicBool,
) -> Result<String> {
    let old = c.command("LIST_NETWORKS")?;
    let previous: Vec<(u32, bool, bool)> = old
        .lines()
        .skip(1)
        .filter_map(|s| {
            let v: Vec<_> = s.split('\t').collect();
            Some((
                v.first()?.parse().ok()?,
                !v.last()?.contains("[DISABLED]"),
                v.last()?.contains("[CURRENT]"),
            ))
        })
        .collect();
    let fresh = password.is_some() || n.saved.is_none();
    let secret = if fresh && n.security == Security::Personal {
        Some(psk(password.as_deref().unwrap_or(""))?)
    } else {
        None
    };
    let id = if fresh {
        c.command("ADD_NETWORK")?
            .parse::<u32>()
            .map_err(|_| "Cannot create Wi-Fi profile")?
    } else {
        n.saved.unwrap()
    };
    let result = (|| {
        if fresh {
            c.ok(&format!("SET_NETWORK {id} ssid {}", hex(&n.ssid)))?;
            c.ok(&format!(
                "SET_NETWORK {id} key_mgmt {}",
                if n.security == Security::Open {
                    "NONE"
                } else {
                    "WPA-PSK"
                }
            ))?;
            if let Some(secret) = secret {
                c.ok(&format!("SET_NETWORK {id} psk {secret}"))?;
            }
        }
        c.ok(&format!("SELECT_NETWORK {id}"))?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            pause(cancel, 500)?;
            let s = c.command("STATUS")?;
            if field(&s, "id").and_then(|s| s.parse::<u32>().ok()) == Some(id)
                && field(&s, "wpa_state") == Some("COMPLETED")
            {
                // Restore eligibility of previously enabled saved networks.
                for &(old, on, _) in &previous {
                    if on {
                        c.ok(&format!("ENABLE_NETWORK {old}"))?;
                    }
                }
                // Keep the previous profile until a successful replacement has been saved.
                if c.ok("SAVE_CONFIG").is_err() {
                    return Ok("Connected; firmware could not save profile".into());
                }
                if fresh {
                    if let Some(old) = n.saved {
                        if old != id && c.ok(&format!("REMOVE_NETWORK {old}")).is_ok() {
                            let _ = c.ok("SAVE_CONFIG");
                        }
                    }
                }
                return Ok(if field(&s, "ip_address").is_some() {
                    "Connected; network saved"
                } else {
                    "Connected; waiting for IP address"
                }
                .into());
            }
        }
        Err("Connection failed. Check password / signal".into())
    })();
    if result.is_err() {
        if fresh {
            let _ = c.ok(&format!("REMOVE_NETWORK {id}"));
        }
        if let Some(&(id, _, _)) = previous.iter().find(|(_, _, current)| *current) {
            let _ = c.ok(&format!("SELECT_NETWORK {id}"));
        } else {
            let _ = c.ok("DISCONNECT");
        }
        for &(id, on, _) in &previous {
            let _ = c.ok(&format!(
                "{}_NETWORK {id}",
                if on { "ENABLE" } else { "DISABLE" }
            ));
        }
    }
    result
}
enum Job {
    Status,
    Scan,
    Power(bool),
    Connect(Network, Option<String>),
    Disconnect,
}
struct Reply {
    on: bool,
    status: String,
    networks: Option<Vec<Network>>,
    /// Joined a network with no address yet: check again soon.
    awaiting_ip: bool,
}
fn run(job: Job, cancel: &AtomicBool) -> Result<Reply> {
    if let Job::Power(on) = job {
        service(on)?;
        if !on {
            return Ok(Reply {
                on: false,
                status: "Wi-Fi off".into(),
                networks: Some(Vec::new()),
                awaiting_ip: false,
            });
        }
        for _ in 0..20 {
            if Control::open().is_ok() {
                break;
            }
            pause(cancel, 250)?;
        }
        // The radio is on even if the first scan doesn't work out.
        return match run(Job::Scan, cancel) {
            Err(e) if e != "Cancelled" => Ok(Reply {
                on: true,
                status: e,
                networks: Some(Vec::new()),
                awaiting_ip: false,
            }),
            r => r,
        };
    }
    let c = match Control::open() {
        Ok(c) => c,
        Err(e) => {
            return if matches!(job, Job::Status) {
                Ok(Reply {
                    on: false,
                    status: "Wi-Fi off / service unavailable".into(),
                    networks: None,
                    awaiting_ip: false,
                })
            } else {
                Err(e)
            }
        }
    };
    let s = c.command("STATUS")?;
    let connected = field(&s, "wpa_state") == Some("COMPLETED");
    let awaiting_ip = connected && field(&s, "ip_address").is_none();
    if awaiting_ip {
        request_address();
    }
    let mut r = Reply {
        on: true,
        status: if connected {
            format!(
                "{}  {}",
                name(&decode(field(&s, "ssid").unwrap_or("Connected"))),
                field(&s, "ip_address").unwrap_or("Waiting for IP")
            )
        } else {
            "Not connected".into()
        },
        networks: None,
        awaiting_ip,
    };
    match job {
        Job::Status => {}
        Job::Power(_) => unreachable!(),
        Job::Scan => {
            let networks = scan(&c, cancel, 3500)?;
            if networks.is_empty() {
                r.status = "No networks found. Try scanning again".into();
            }
            r.networks = Some(networks);
        }
        Job::Connect(n, p) => {
            r.status = connect(&c, &n, p, cancel)?;
            r.awaiting_ip = false;
            if r.status.ends_with("waiting for IP address") {
                // Ask for an address and give it a moment, so the menu reads
                // Connected rather than waiting whenever the network is quick.
                request_address();
                r.awaiting_ip = true;
                for _ in 0..20 {
                    pause(cancel, 500)?;
                    let s = c.command("STATUS")?;
                    if let Some(ip) = field(&s, "ip_address") {
                        r.status = format!("Connected  {ip}");
                        r.awaiting_ip = false;
                        break;
                    }
                }
            }
            r.networks = Some(scan_rows(
                &c.command("SCAN_RESULTS")?,
                &c.command("LIST_NETWORKS")?,
            ));
        }
        Job::Disconnect => {
            c.ok("DISCONNECT")?;
            r.status = "Disconnected".into();
        }
    }
    Ok(r)
}
struct Worker {
    rx: mpsc::Receiver<Result<Reply>>,
    cancel: Arc<AtomicBool>,
}
/// What the on-screen keyboard is typing: a network's password or the RetroAchievements login.
enum KeyTarget {
    Wifi(Network),
    RaUser,
    RaPassword,
}
struct Keyboard {
    target: KeyTarget,
    secret: bool,
    password: String,
    key: usize,
    page: usize,
}
/// The text typed so far: dots for a password, as typed otherwise.
fn masked(k: &Keyboard) -> String {
    let n = k.password.chars().count();
    if k.secret {
        "*".repeat(n.min(30))
    } else {
        k.password.chars().skip(n.saturating_sub(30)).collect()
    }
}
fn card_root() -> PathBuf {
    PathBuf::from(std::env::var_os("SLOT_ROOT").unwrap_or_else(|| "/mnt/SDCARD/Slot".into()))
}
/// Online: wlan0 has a way out (a default route, set when DHCP hands out an address). Joined
/// to a network alone is not enough: with no address nothing can be reached.
pub fn online() -> bool {
    fs::read_to_string("/proc/net/route").is_ok_and(|s| routed(&s))
}
fn routed(table: &str) -> bool {
    table.lines().skip(1).any(|l| {
        let f: Vec<&str> = l.split_whitespace().collect();
        f.len() > 1 && f[0] == "wlan0" && f[1] == "00000000"
    })
}
/// The Cart Art screen: row 0 fetches art, the rest are games with a label, to delete.
/// `games`: platform, stem, label, and whether it is the Famicom label.
struct ArtList {
    games: Vec<(Platform, String, PathBuf, bool)>,
    armed: Option<usize>,
}
impl ArtList {
    fn load() -> Self {
        // A NES game's Famicom label is a row of its own, to delete apart from its NES one.
        let mut games: Vec<_> = slot_store::scan(&card_root())
            .unwrap_or_default()
            .into_iter()
            .flat_map(|c| {
                [
                    c.label.map(|l| (c.platform, c.stem.clone(), l, false)),
                    c.famicom_label.map(|l| (c.platform, c.stem, l, true)),
                ]
            })
            .flatten()
            .collect();
        games.sort_by(|a, b| {
            slot_store::sort_key(&a.1)
                .cmp(&slot_store::sort_key(&b.1))
                .then((a.0 as u8).cmp(&(b.0 as u8)))
        });
        Self { games, armed: None }
    }
}
/// The RetroAchievements screen: log in, test the connection, show a test unlock, log out.
/// `test`: a pretend unlock waiting for the app to toast it.
struct RaScreen {
    account: Option<crate::ra::Account>,
    user: String,
    password: String,
    job: Option<mpsc::Receiver<RaDone>>,
    test: Option<TestUnlock>,
}
/// A pretend unlock: title, description, points, and a badge already cached on the card.
pub struct TestUnlock {
    pub title: String,
    pub description: String,
    pub points: u32,
    pub badge: Option<PathBuf>,
}
/// Any badge cached on the card, so the test toast looks like the real thing.
fn cached_badge(root: &Path) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(root.join("System/Cache/Badges"))
        .ok()?
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "png"))
        .collect();
    v.sort();
    v.into_iter().next()
}
enum RaDone {
    LoggedIn(Result<(crate::ra::Account, crate::ra::Profile)>),
    Checked(Result<crate::ra::Profile>),
}
fn profile_line(p: &crate::ra::Profile) -> String {
    format!(
        "Connected in {} ms: {}, {} points ({} softcore)",
        p.ms, p.user, p.score, p.softcore
    )
}
const KEYS: [&str; 3] = [
    "abcdefghijklmnopqrstuvwxyz0123456789    ",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789    ",
    "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~        ",
];
pub struct WifiMenu {
    worker: Option<Worker>,
    pub face: Option<TexId>,
    pub dirty: bool,
    on: bool,
    status: String,
    networks: Vec<Network>,
    row: usize,
    keyboard: Option<Keyboard>,
    refreshed: Instant,
    awaiting_ip: bool,
    art: Option<ArtJob>,
    fetched: Vec<Fetched>,
    /// Set when this overlay is the Cart Art screen rather than Wi-Fi.
    list: Option<ArtList>,
    /// Set when this overlay is the RetroAchievements screen rather than Wi-Fi.
    ra: Option<RaScreen>,
}
impl WifiMenu {
    #[allow(clippy::new_without_default)] // new() starts a status query on a worker thread
    pub fn new() -> Self {
        let mut s = Self {
            worker: None,
            face: None,
            dirty: true,
            on: false,
            status: "Checking Wi-Fi...".into(),
            networks: Vec::new(),
            row: 0,
            keyboard: None,
            refreshed: Instant::now(),
            awaiting_ip: false,
            art: None,
            fetched: Vec::new(),
            list: None,
            ra: None,
        };
        s.start(Job::Status);
        s
    }
    /// The overlay behind HOME > Cart Art.
    pub fn cart_art() -> Self {
        let list = ArtList::load();
        let status = format!(
            "{} games have a label. A on one twice deletes it",
            list.games.len()
        );
        Self {
            worker: None,
            face: None,
            dirty: true,
            on: true,
            status,
            networks: Vec::new(),
            row: 0,
            keyboard: None,
            refreshed: Instant::now(),
            awaiting_ip: false,
            art: None,
            fetched: Vec::new(),
            list: Some(list),
            ra: None,
        }
    }
    /// The overlay behind HOME > RetroAchievements.
    pub fn achievements() -> Self {
        let account = crate::ra::load(&card_root());
        let status = match &account {
            Some(a) => format!("Logged in as {}", a.user),
            None => "Log in with your RetroAchievements account".into(),
        };
        Self {
            worker: None,
            face: None,
            dirty: true,
            on: true,
            status,
            networks: Vec::new(),
            row: 0,
            keyboard: None,
            refreshed: Instant::now(),
            awaiting_ip: false,
            art: None,
            fetched: Vec::new(),
            list: None,
            ra: Some(RaScreen {
                account,
                user: String::new(),
                password: String::new(),
                job: None,
                test: None,
            }),
        }
    }
    pub fn busy(&self) -> bool {
        self.worker.is_some()
            || self.art.is_some()
            || self.ra.as_ref().is_some_and(|r| r.job.is_some())
    }
    /// Labels saved or deleted since the last call, for the shelf to show straight away.
    pub fn take_fetched(&mut self) -> Vec<Fetched> {
        std::mem::take(&mut self.fetched)
    }
    /// The test unlock asked for since the last call, for the app to toast.
    pub fn take_test_unlock(&mut self) -> Option<TestUnlock> {
        self.ra.as_mut()?.test.take()
    }
    pub fn cancel(&mut self) {
        if let Some(a) = &self.art {
            a.cancel();
            self.status = "Cancelling...".into();
            self.dirty = true;
        }
        // A reply that comes back after this is dropped unread, so nothing gets saved.
        if let Some(r) = &mut self.ra {
            if r.job.take().is_some() {
                self.status = "Cancelled".into();
                self.dirty = true;
                return;
            }
        }
        if let Some(w) = &self.worker {
            w.cancel.store(true, Ordering::Relaxed);
            self.status = "Cancelling...".into();
            self.dirty = true;
        }
    }
    fn start(&mut self, job: Job) {
        if self.busy() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let token = cancel.clone();
        if !matches!(job, Job::Status) {
            self.status = match job {
                Job::Scan => "Scanning...",
                Job::Power(true) => "Turning Wi-Fi on...",
                Job::Power(false) => "Turning Wi-Fi off...",
                Job::Connect(..) => "Connecting...",
                Job::Disconnect => "Disconnecting...",
                _ => unreachable!(),
            }
            .into();
        }
        thread::spawn(move || {
            let _ = tx.send(run(job, &token));
        });
        self.worker = Some(Worker { rx, cancel });
        self.dirty = true;
    }
    fn poll_art(&mut self) {
        let Some(job) = &self.art else {
            return;
        };
        loop {
            match job.try_recv() {
                Ok(Progress::Step { done, total, name }) => {
                    self.status = format!("Cart art {}/{total}: {name}", done + 1);
                    self.dirty = true;
                }
                Ok(Progress::Got(f)) => self.fetched.push(f),
                Ok(Progress::Done(r)) => {
                    self.status = match r {
                        Ok(s) if s.found + s.missing + s.failed == 0 => {
                            "Every game already has a label".into()
                        }
                        Ok(s) => format!(
                            "Cart art: {} added, {} not found{}",
                            s.found,
                            s.missing,
                            if s.failed > 0 {
                                format!(", {} failed", s.failed)
                            } else {
                                String::new()
                            }
                        ),
                        Err(e) => e,
                    };
                    if let Some(l) = &mut self.list {
                        *l = ArtList::load();
                        self.row = self.row.min(l.games.len());
                    }
                    self.art = None;
                    self.dirty = true;
                    self.refreshed = Instant::now();
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.art = None;
                    self.status = "Cart art stopped".into();
                    self.dirty = true;
                    return;
                }
            }
        }
    }
    pub fn poll(&mut self) {
        self.poll_art();
        self.poll_ra();
        if let Some(w) = &self.worker {
            match w.rx.try_recv() {
                Ok(r) => {
                    self.worker = None;
                    match r {
                        Ok(r) => {
                            self.on = r.on;
                            self.status = r.status;
                            self.awaiting_ip = r.awaiting_ip;
                            if let Some(n) = r.networks {
                                self.networks = n;
                            }
                        }
                        Err(e) => self.status = e,
                    }
                    self.row = self.row.min(self.networks.len() + 2);
                    self.dirty = true;
                    self.refreshed = Instant::now();
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    self.status = "Wi-Fi worker stopped".into();
                    self.dirty = true;
                    self.refreshed = Instant::now();
                }
                _ => {}
            }
        }
        // Poll only while this menu is open; scanning is always explicit.
        // While an address is on its way, check every 2 s instead of every 15.
        let every = Duration::from_secs(if self.awaiting_ip { 2 } else { 15 });
        if self.ra.is_none()
            && self.list.is_none()
            && !self.busy()
            && self.keyboard.is_none()
            && self.refreshed.elapsed() > every
        {
            self.start(Job::Status);
        }
    }
    pub fn input(&mut self, a: Action) -> bool {
        if self.busy() {
            if matches!(a, Action::GbaDown(Btn::B)) {
                self.cancel();
            }
            return false;
        }
        if !matches!(
            a,
            Action::GbaDown(_) | Action::ShelfLeft | Action::ShelfRight | Action::Insert
        ) {
            return false;
        }
        self.dirty = true;
        if self.list.is_some() {
            return self.art_input(a);
        }
        if let Some(k) = &mut self.keyboard {
            match a {
                Action::GbaDown(Btn::B) => self.keyboard = None,
                Action::GbaDown(Btn::X) => {
                    k.password.pop();
                }
                Action::GbaDown(Btn::Y) => k.page = (k.page + 1) % 3,
                Action::GbaDown(Btn::Up) => k.key = (k.key + 30) % 40,
                Action::GbaDown(Btn::Down) => k.key = (k.key + 10) % 40,
                Action::GbaDown(Btn::Left) | Action::ShelfLeft => {
                    k.key = k.key / 10 * 10 + (k.key % 10 + 9) % 10
                }
                Action::GbaDown(Btn::Right) | Action::ShelfRight => {
                    k.key = k.key / 10 * 10 + (k.key % 10 + 1) % 10
                }
                Action::GbaDown(Btn::A) | Action::Insert => {
                    if k.password.len() < 64 {
                        k.password.push(KEYS[k.page].as_bytes()[k.key] as char);
                    }
                }
                Action::GbaDown(Btn::Start) => {
                    if let KeyTarget::Wifi(_) = k.target {
                        if let Err(e) = psk(&k.password) {
                            self.status = e;
                            return false;
                        }
                    }
                    let k = self.keyboard.take().unwrap();
                    match k.target {
                        KeyTarget::Wifi(n) => self.start(Job::Connect(n, Some(k.password))),
                        KeyTarget::RaUser => {
                            if let Some(ra) = &mut self.ra {
                                ra.user = k.password.trim().to_string();
                            }
                        }
                        KeyTarget::RaPassword => {
                            if let Some(ra) = &mut self.ra {
                                ra.password = k.password;
                            }
                        }
                    }
                }
                _ => {}
            }
            return false;
        }
        if self.ra.is_some() {
            return self.ra_input(a);
        }
        match a {
            Action::GbaDown(Btn::B) => return true,
            Action::GbaDown(Btn::Up) => {
                self.row = (self.row + self.networks.len() + 2) % (self.networks.len() + 3)
            }
            Action::GbaDown(Btn::Down) => self.row = (self.row + 1) % (self.networks.len() + 3),
            Action::GbaDown(Btn::A) | Action::Insert => match self.row {
                0 => self.start(Job::Power(!self.on)),
                1 => self.start(Job::Scan),
                2 => self.start(Job::Disconnect),
                _ => self.choose(false),
            },
            Action::GbaDown(Btn::X) => self.choose(true),
            _ => {}
        }
        false
    }
    fn choose(&mut self, edit: bool) {
        let Some(n) = self
            .row
            .checked_sub(3)
            .and_then(|i| self.networks.get(i))
            .cloned()
        else {
            return;
        };
        if n.security == Security::Unsupported {
            self.status = "Use an open or WPA/WPA2 personal network".into();
            return;
        }
        if n.security == Security::Personal && (edit || n.saved.is_none()) {
            self.status = "Password is case sensitive".into();
            self.keyboard = Some(Keyboard {
                target: KeyTarget::Wifi(n),
                secret: true,
                password: String::new(),
                key: 0,
                page: 0,
            });
        } else {
            self.start(Job::Connect(n, None));
        }
    }
    fn poll_ra(&mut self) {
        let Some(ra) = &mut self.ra else { return };
        let Some(job) = &ra.job else { return };
        let done = match job.try_recv() {
            Ok(d) => d,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                ra.job = None;
                self.status = "RetroAchievements check stopped".into();
                self.dirty = true;
                return;
            }
        };
        ra.job = None;
        self.dirty = true;
        self.status = match done {
            RaDone::LoggedIn(Ok((account, profile))) => {
                match crate::ra::save(&card_root(), &account) {
                    Ok(()) => {
                        ra.password.clear();
                        ra.account = Some(account);
                        self.row = 1;
                        format!("Logged in. {}", profile_line(&profile))
                    }
                    Err(e) => format!("Logged in, but cannot save the login: {e}"),
                }
            }
            RaDone::Checked(Ok(profile)) => profile_line(&profile),
            RaDone::LoggedIn(Err(e)) | RaDone::Checked(Err(e)) => e,
        };
    }
    fn ra_input(&mut self, a: Action) -> bool {
        let Some(ra) = &mut self.ra else { return true };
        let rows = if ra.account.is_some() { 4 } else { 3 };
        match a {
            Action::GbaDown(Btn::B) => return true,
            Action::GbaDown(Btn::Up) => self.row = (self.row + rows - 1) % rows,
            Action::GbaDown(Btn::Down) => self.row = (self.row + 1) % rows,
            Action::GbaDown(Btn::A) | Action::Insert => match (&ra.account, self.row) {
                (None, 0) => {
                    self.keyboard = Some(Keyboard {
                        target: KeyTarget::RaUser,
                        secret: false,
                        password: ra.user.clone(),
                        key: 0,
                        page: 0,
                    })
                }
                (None, 1) => {
                    self.keyboard = Some(Keyboard {
                        target: KeyTarget::RaPassword,
                        secret: true,
                        password: String::new(),
                        key: 0,
                        page: 0,
                    })
                }
                (None, _) => {
                    if ra.user.is_empty() || ra.password.is_empty() {
                        self.status = "Enter your username and password first".into();
                        return false;
                    }
                    if !online() {
                        self.status = "Connect to Wi-Fi first".into();
                        return false;
                    }
                    let (user, password) = (ra.user.clone(), ra.password.clone());
                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move || {
                        let _ = tx.send(RaDone::LoggedIn(crate::ra::login(&user, &password)));
                    });
                    ra.job = Some(rx);
                    self.status = "Logging in...".into();
                }
                (Some(_), 0) => {}
                (Some(account), 1) => {
                    if !online() {
                        self.status = "Connect to Wi-Fi first".into();
                        return false;
                    }
                    let account = account.clone();
                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move || {
                        let _ = tx.send(RaDone::Checked(crate::ra::check(&account)));
                    });
                    ra.job = Some(rx);
                    self.status = "Testing the connection...".into();
                }
                (Some(_), 2) => {
                    ra.test = Some(TestUnlock {
                        title: "Test Achievement".into(),
                        description: "Toasts are working".into(),
                        points: 5,
                        badge: cached_badge(&card_root()),
                    });
                    self.status = "Test achievement sent".into();
                }
                (Some(_), _) => match crate::ra::forget(&card_root()) {
                    Ok(()) => {
                        ra.account = None;
                        self.row = 0;
                        self.status = "Logged out".into();
                    }
                    Err(e) => self.status = format!("Cannot log out: {e}"),
                },
            },
            _ => {}
        }
        false
    }
    fn art_input(&mut self, a: Action) -> bool {
        let Some(list) = &mut self.list else {
            return true;
        };
        let rows = list.games.len() + 1;
        match a {
            Action::GbaDown(Btn::B) => {
                if list.armed.take().is_none() {
                    return true;
                }
                self.status = "Kept it".into();
            }
            Action::GbaDown(Btn::Up) => {
                self.row = (self.row + rows - 1) % rows;
                list.armed = None;
            }
            Action::GbaDown(Btn::Down) => {
                self.row = (self.row + 1) % rows;
                list.armed = None;
            }
            Action::GbaDown(Btn::A) | Action::Insert if self.row == 0 => {
                if !online() {
                    self.status = "Connect to Wi-Fi first".into();
                    return false;
                }
                self.status = "Cart art: fetching game list...".into();
                self.art = Some(ArtJob::start(card_root()));
            }
            Action::GbaDown(Btn::A) | Action::Insert => {
                let at = self.row - 1;
                let Some((platform, stem, path, _)) = list.games.get(at).cloned() else {
                    return false;
                };
                if list.armed != Some(at) {
                    list.armed = Some(at);
                    self.status = format!("Press A again to delete the label of {stem}");
                    return false;
                }
                list.armed = None;
                match fs::remove_file(&path) {
                    Ok(()) => {
                        eprintln!("slot: cart art: deleted {}", path.display());
                        list.games.remove(at);
                        self.row = self.row.min(list.games.len());
                        self.fetched.push(Fetched {
                            platform,
                            stem: stem.clone(),
                            label: None,
                        });
                        self.status = format!("Deleted the label of {stem}");
                    }
                    Err(e) => self.status = format!("Cannot delete label: {e}"),
                }
            }
            _ => {}
        }
        false
    }
    fn render_art(&self, list: &ArtList) -> UndoFace {
        let mut p = Painter::new();
        p.text("CART ART", 32, 24, 576, 30.0);
        p.text(&self.status, 32, 66, 576, 18.0);
        let rows = list.games.len() + 1;
        let first = self.row.saturating_sub(6);
        for (line, row) in (first..rows.min(first + 7)).enumerate() {
            let y = 110 + line * 41;
            if row == self.row {
                p.band(y, 38, slot_ui::edge());
            }
            let label = match row {
                0 => "GET CART ART".into(),
                _ => {
                    let (platform, stem, _, famicom) = &list.games[row - 1];
                    format!(
                        "{}{}  {}",
                        if list.armed == Some(row - 1) {
                            "DELETE?  "
                        } else {
                            ""
                        },
                        if *famicom { "FC" } else { platform.dir_name() },
                        stem
                    )
                }
            };
            p.text(&label, 32, y + 1, 576, 23.0);
        }
        p.text(
            if self.busy() {
                "B CANCEL"
            } else if self.row == 0 {
                "B BACK    A GET ART"
            } else if list.armed.is_some() {
                "B KEEP    A DELETE LABEL"
            } else {
                "B BACK    A DELETE LABEL"
            },
            24,
            430,
            592,
            20.0,
        );
        UndoFace {
            rgba: p.rgba,
            w: W as u32,
            h: H as u32,
        }
    }
    fn render_ra(&self, ra: &RaScreen) -> UndoFace {
        let mut p = Painter::new();
        p.text("RETROACHIEVEMENTS", 32, 24, 576, 30.0);
        p.text(&self.status, 32, 66, 576, 18.0);
        let rows: Vec<String> = match &ra.account {
            None => vec![
                format!(
                    "USERNAME     {}",
                    if ra.user.is_empty() { "-" } else { &ra.user }
                ),
                format!(
                    "PASSWORD     {}",
                    if ra.password.is_empty() {
                        "-".to_string()
                    } else {
                        "*".repeat(ra.password.chars().count().min(20))
                    }
                ),
                "LOG IN".into(),
            ],
            Some(a) => vec![
                format!("LOGGED IN AS  {}", a.user),
                "TEST CONNECTION".into(),
                "TEST ACHIEVEMENT".into(),
                "LOG OUT".into(),
            ],
        };
        for (i, label) in rows.iter().enumerate() {
            let y = 110 + i * 41;
            if i == self.row {
                p.band(y, 38, slot_ui::edge());
            }
            p.text(label, 32, y + 1, 576, 23.0);
        }
        p.text(
            if self.busy() {
                "B CANCEL"
            } else {
                "B BACK    A SELECT"
            },
            24,
            430,
            592,
            20.0,
        );
        UndoFace {
            rgba: p.rgba,
            w: W as u32,
            h: H as u32,
        }
    }
    pub fn render(&self) -> UndoFace {
        if let Some(list) = &self.list {
            return self.render_art(list);
        }
        if let (None, Some(ra)) = (&self.keyboard, &self.ra) {
            return self.render_ra(ra);
        }
        let mut p = Painter::new();
        p.text(
            if self.ra.is_some() {
                "RETROACHIEVEMENTS"
            } else {
                "WI-FI"
            },
            32,
            24,
            576,
            30.0,
        );
        p.text(&self.status, 32, 66, 576, 18.0);
        if let Some(k) = &self.keyboard {
            let title = match &k.target {
                KeyTarget::Wifi(n) => name(&n.ssid),
                KeyTarget::RaUser => "USERNAME".into(),
                KeyTarget::RaPassword => "PASSWORD".into(),
            };
            p.text(&title, 32, 102, 576, 24.0);
            p.text(
                &format!("{}  ({}/64)", masked(k), k.password.len()),
                32,
                142,
                576,
                21.0,
            );
            for i in 0..40 {
                let x = 32 + i % 10 * 58;
                let y = 196 + i / 10 * 45;
                if i == k.key {
                    p.rect(x, y, 54, 41, slot_ui::edge());
                }
                let c = &KEYS[k.page][i..i + 1];
                p.text(if c == " " { "_" } else { c }, x, y + 3, 54, 26.0);
            }
            p.text(
                "A TYPE    X DELETE    Y abc / ABC / #!?",
                24,
                392,
                592,
                18.0,
            );
            p.text(
                if matches!(k.target, KeyTarget::Wifi(_)) {
                    "B BACK                    START CONNECT"
                } else {
                    "B BACK                    START DONE"
                },
                24,
                430,
                592,
                20.0,
            );
        } else {
            let first = if self.row < 7 { 0 } else { self.row - 6 };
            for (line, row) in (first..(self.networks.len() + 3).min(first + 7)).enumerate() {
                let y = 110 + line * 41;
                if row == self.row {
                    p.band(y, 38, slot_ui::edge());
                }
                let label = match row {
                    0 => format!(
                        "WI-FI                          {}",
                        if self.on { "ON" } else { "OFF" }
                    ),
                    1 => "SCAN NETWORKS".into(),
                    2 => "DISCONNECT".into(),
                    _ => {
                        let n = &self.networks[row - 3];
                        format!(
                            "{}  {}  {} dBm",
                            name(&n.ssid),
                            if n.saved.is_some() {
                                "[SAVED]"
                            } else if n.security == Security::Open {
                                "[OPEN]"
                            } else {
                                "[LOCK]"
                            },
                            n.signal
                        )
                    }
                };
                p.text(&label, 32, y + 1, 576, 23.0);
            }
            p.text(
                if self.busy() {
                    "B CANCEL"
                } else {
                    "B BACK    A SELECT    X EDIT PASSWORD"
                },
                24,
                430,
                592,
                20.0,
            );
        }
        UndoFace {
            rgba: p.rgba,
            w: W as u32,
            h: H as u32,
        }
    }
}
/// The screens are laid out for 640 x 480 and centred on the frame, which is wider on the
/// RG SP (720 x 480) than on the Brick; backgrounds and highlight bands span the whole width.
const W: usize = slot_gfx::OUT_W as usize;
const H: usize = slot_gfx::OUT_H as usize;
const X0: usize = (W - 640) / 2;
struct Painter {
    rgba: Vec<u8>,
}
impl Painter {
    fn new() -> Self {
        let mut s = Self {
            rgba: vec![0; W * H * 4],
        };
        s.fill(0, 0, W, H, slot_ui::opening());
        s
    }
    /// A row highlight across the whole frame.
    fn band(&mut self, y: usize, h: usize, c: [f32; 4]) {
        self.fill(0, y, W, h, c);
    }
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: [f32; 4]) {
        self.fill(X0 + x, y, w, h, c);
    }
    fn fill(&mut self, x: usize, y: usize, w: usize, h: usize, c: [f32; 4]) {
        let c = c.map(|v| (v * 255.0) as u8);
        for yy in y..(y + h).min(H) {
            for xx in x..(x + w).min(W) {
                self.rgba[(yy * W + xx) * 4..(yy * W + xx) * 4 + 4].copy_from_slice(&c)
            }
        }
    }
    fn text(&mut self, s: &str, x: usize, y: usize, w: usize, px: f32) {
        let Some(font) = slot_ui::text::label_font() else {
            return;
        };
        let mut s = s.to_string();
        while slot_ui::text::line_width(font, &s, px, 0.0) > w as f32 && !s.is_empty() {
            s.pop();
        }
        let h = 40;
        let mut face = vec![0; w * h * 4];
        let layout = slot_ui::text::Layout {
            lines: vec![s],
            px,
            tracking: 0.0,
        };
        slot_ui::text::draw_centred(&mut face, w as u32, h as u32, &layout, [238, 238, 242]);
        let x = X0 + x;
        for yy in 0..h.min(H - y) {
            for xx in 0..w.min(W - x) {
                let a = (yy * w + xx) * 4;
                let b = ((y + yy) * W + x + xx) * 4;
                let alpha = face[a + 3] as u32;
                for ch in 0..3 {
                    self.rgba[b + ch] = ((face[a + ch] as u32 * alpha
                        + self.rgba[b + ch] as u32 * (255 - alpha))
                        / 255) as u8;
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    struct Mock {
        steps: RefCell<std::collections::VecDeque<(String, Result<String>)>>,
    }
    impl Mock {
        fn new(steps: Vec<(&str, Result<&str>)>) -> Self {
            Self {
                steps: RefCell::new(
                    steps
                        .into_iter()
                        .map(|(a, b)| (a.into(), b.map(String::from)))
                        .collect(),
                ),
            }
        }
        fn done(&self) {
            assert!(self.steps.borrow().is_empty());
        }
    }
    impl Supplicant for Mock {
        fn command(&self, s: &str) -> Result<String> {
            let (a, b) = self
                .steps
                .borrow_mut()
                .pop_front()
                .expect("unexpected command");
            assert_eq!(a, s);
            b
        }
    }
    fn network() -> Network {
        Network {
            ssid: b"Test".to_vec(),
            signal: -40,
            security: Security::Personal,
            saved: None,
        }
    }
    const RESULTS: &str = "bssid / frequency / signal level / flags / ssid\n00:11:22:33:44:55\t2412\t-40\t[WPA2-PSK-CCMP][ESS]\tHome";
    const SAVED: &str = "network id / ssid / bssid / flags";
    #[test]
    fn a_busy_supplicant_still_gives_scan_results() {
        let c = Mock::new(vec![
            ("SCAN", Ok("FAIL-BUSY")),
            ("SCAN_RESULTS", Ok(RESULTS)),
            ("LIST_NETWORKS", Ok(SAVED)),
        ]);
        let n = scan(&c, &AtomicBool::new(false), 0).unwrap();
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].ssid, b"Home");
        c.done();
    }
    #[test]
    fn a_scan_waits_for_the_interface_to_come_up() {
        let c = Mock::new(vec![
            ("SCAN", Ok("FAIL")),
            ("SCAN", Ok("FAIL")),
            ("SCAN", Ok("OK")),
            ("SCAN_RESULTS", Ok(RESULTS)),
            ("LIST_NETWORKS", Ok(SAVED)),
        ]);
        assert_eq!(scan(&c, &AtomicBool::new(false), 0).unwrap().len(), 1);
        c.done();
    }
    #[test]
    fn a_scan_that_never_starts_says_why() {
        let mut steps = vec![("SCAN", Ok("FAIL")); 12];
        steps.push((
            "SCAN_RESULTS",
            Ok("bssid / frequency / signal level / flags / ssid"),
        ));
        steps.push(("LIST_NETWORKS", Ok(SAVED)));
        steps.push(("STATUS", Ok("wpa_state=INTERFACE_DISABLED")));
        let c = Mock::new(steps);
        let e = scan(&c, &AtomicBool::new(false), 0).unwrap_err();
        assert_eq!(
            e,
            "Wi-Fi isn't ready (INTERFACE_DISABLED). Try scanning again"
        );
        c.done();
    }
    #[test]
    fn parse_scans() {
        let n=scan_rows("header\na\t2412\t-80\t[WPA2-PSK-CCMP]\tMy\\x20WiFi\nb\t2412\t-40\t[WPA2-PSK-CCMP]\tMy WiFi\nc\t2412\t-30\t[ESS]\tCafe\nd\t2412\t-20\t[WPA2-EAP-CCMP]\tWork\n","header\n7\tMy WiFi\tany\t[CURRENT]\n");
        assert_eq!(n.len(), 3);
        assert_eq!(n[0].security, Security::Unsupported);
        assert_eq!(n[2].signal, -40);
        assert_eq!(n[2].saved, Some(7));
    }
    #[test]
    fn password_bytes() {
        assert!(psk("short").is_err());
        assert!(psk("1234567\n").is_err());
        assert_eq!(psk("Abc\"\\123").unwrap(), "\"Abc\"\\123\"");
        assert_eq!(psk(&"a".repeat(64)).unwrap(), "a".repeat(64));
        assert!(psk(&"z".repeat(64)).is_err());
    }
    #[test]
    fn helper_prefers_card_then_firmware() {
        let d = tempfile::tempdir().unwrap();
        let fw = d.path().join("wifi_init.sh");
        let root = d.path().join("Slot");
        assert_eq!(helper(&fw, &root), None);
        fs::write(&fw, "").unwrap();
        assert_eq!(helper(&fw, &root), Some(fw.clone()));
        fs::create_dir_all(root.join("System")).unwrap();
        fs::write(root.join("System/brick_wifi.sh"), "").unwrap();
        assert_eq!(helper(&fw, &root), Some(root.join("System/brick_wifi.sh")));
    }
    #[test]
    fn the_switch_is_remembered_for_the_next_boot() {
        let d = tempfile::tempdir().unwrap();
        remember(d.path(), true);
        assert!(d.path().join(WIFI_ON).exists());
        remember(d.path(), false);
        assert!(!d.path().join(WIFI_ON).exists());
        remember(d.path(), false);
    }
    #[test]
    fn helper_finds_the_rg_sp_script() {
        let d = tempfile::tempdir().unwrap();
        let fw = d.path().join("wifi_init.sh");
        fs::write(&fw, "").unwrap();
        fs::create_dir_all(d.path().join("System")).unwrap();
        fs::write(d.path().join("System/wifi.sh"), "").unwrap();
        assert_eq!(helper(&fw, d.path()), Some(d.path().join("System/wifi.sh")));
    }
    #[test]
    fn all_keys() {
        for s in KEYS {
            assert_eq!(s.len(), 40)
        }
        for b in 32..=126 {
            assert!(KEYS.iter().any(|s| s.as_bytes().contains(&b)), "{b}");
        }
    }
    #[test]
    fn failed_setup_restores_previous_connection() {
        let m = Mock::new(vec![
            (
                "LIST_NETWORKS",
                Ok("header\n3\tOld\tany\t[CURRENT]\n4\tOther\tany\t[DISABLED]"),
            ),
            ("ADD_NETWORK", Ok("8")),
            ("SET_NETWORK 8 ssid 54657374", Err("rejected".into())),
            ("REMOVE_NETWORK 8", Ok("OK")),
            ("SELECT_NETWORK 3", Ok("OK")),
            ("ENABLE_NETWORK 3", Ok("OK")),
            ("DISABLE_NETWORK 4", Ok("OK")),
        ]);
        assert!(connect(
            &m,
            &network(),
            Some("password".into()),
            &AtomicBool::new(false)
        )
        .is_err());
        m.done();
    }
    #[test]
    fn cancelled_connect_removes_unsaved_profile() {
        let m = Mock::new(vec![
            ("LIST_NETWORKS", Ok("header")),
            ("ADD_NETWORK", Ok("8")),
            ("SET_NETWORK 8 ssid 54657374", Ok("OK")),
            ("SET_NETWORK 8 key_mgmt WPA-PSK", Ok("OK")),
            ("SET_NETWORK 8 psk \"password\"", Ok("OK")),
            ("SELECT_NETWORK 8", Ok("OK")),
            ("REMOVE_NETWORK 8", Ok("OK")),
            ("DISCONNECT", Ok("OK")),
        ]);
        assert_eq!(
            connect(
                &m,
                &network(),
                Some("password".into()),
                &AtomicBool::new(true)
            )
            .unwrap_err(),
            "Cancelled"
        );
        m.done();
    }
    #[test]
    fn successful_connect_saves_only_after_association() {
        let m = Mock::new(vec![
            ("LIST_NETWORKS", Ok("header")),
            ("ADD_NETWORK", Ok("8")),
            ("SET_NETWORK 8 ssid 54657374", Ok("OK")),
            ("SET_NETWORK 8 key_mgmt WPA-PSK", Ok("OK")),
            ("SET_NETWORK 8 psk \"password\"", Ok("OK")),
            ("SELECT_NETWORK 8", Ok("OK")),
            (
                "STATUS",
                Ok("id=8\nwpa_state=COMPLETED\nip_address=192.168.1.2"),
            ),
            ("SAVE_CONFIG", Ok("OK")),
        ]);
        assert_eq!(
            connect(
                &m,
                &network(),
                Some("password".into()),
                &AtomicBool::new(false)
            )
            .unwrap(),
            "Connected; network saved"
        );
        m.done();
    }
    #[test]
    #[ignore = "Requires a Linux runtime allowing AF_UNIX datagram sockets"]
    fn socket_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let remote = dir.path().join("server");
        let server = UnixDatagram::bind(&remote).unwrap();
        let t = thread::spawn(move || {
            let mut b = [0; 64];
            let (n, peer) = server.recv_from(&mut b).unwrap();
            assert_eq!(&b[..n], b"PING");
            server
                .send_to(b"PONG\n", peer.as_pathname().unwrap())
                .unwrap();
        });
        let c = Control::at(&remote).unwrap();
        let local = c.local.clone();
        assert_eq!(
            fs::metadata(&local).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(c.command("PING").unwrap(), "PONG");
        drop(c);
        assert!(!local.exists());
        t.join().unwrap();
    }
    #[test]
    fn joined_without_an_address_is_not_online() {
        let head = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\n";
        assert!(!routed(head));
        assert!(!routed(&format!(
            "{head}wlan0\t0001A8C0\t00000000\t0001\t0\t0\t0\t00FFFFFF\n"
        )));
        assert!(routed(&format!(
            "{head}wlan0\t00000000\t0101A8C0\t0003\t0\t0\t0\t00000000\n"
        )));
        assert!(!routed(&format!(
            "{head}usb0\t00000000\t0101A8C0\t0003\t0\t0\t0\t00000000\n"
        )));
    }
    fn ra_menu(account: Option<crate::ra::Account>) -> WifiMenu {
        WifiMenu {
            worker: None,
            face: None,
            dirty: true,
            on: true,
            status: String::new(),
            networks: Vec::new(),
            row: 0,
            keyboard: None,
            refreshed: Instant::now(),
            awaiting_ip: false,
            art: None,
            fetched: Vec::new(),
            list: None,
            ra: Some(RaScreen {
                account,
                user: String::new(),
                password: String::new(),
                job: None,
                test: None,
            }),
        }
    }
    #[test]
    fn ra_screen_types_the_login_and_needs_both_fields() {
        let mut m = ra_menu(None);
        m.input(Action::GbaDown(Btn::A));
        assert!(matches!(
            m.keyboard.as_ref().map(|k| &k.target),
            Some(KeyTarget::RaUser)
        ));
        m.input(Action::GbaDown(Btn::A));
        m.input(Action::GbaDown(Btn::Start));
        assert_eq!(m.ra.as_ref().unwrap().user, "a");
        m.row = 2;
        m.input(Action::GbaDown(Btn::A));
        assert_eq!(m.status, "Enter your username and password first");
        assert!(!m.busy());
        m.render();
    }
    #[test]
    fn ra_screen_sends_one_test_unlock() {
        let account = crate::ra::Account {
            user: "someone".into(),
            token: "t".into(),
        };
        let mut m = ra_menu(Some(account));
        m.row = 2;
        assert!(m.take_test_unlock().is_none());
        m.input(Action::GbaDown(Btn::A));
        let t = m.take_test_unlock().unwrap();
        assert_eq!((t.title.as_str(), t.points), ("Test Achievement", 5));
        assert!(m.take_test_unlock().is_none(), "taken once");
        assert!(!m.busy());
    }
    #[test]
    fn render_fixture() {
        let s = WifiMenu {
            worker: None,
            face: None,
            dirty: true,
            on: true,
            status: "Password is case sensitive".into(),
            networks: Vec::new(),
            row: 0,
            awaiting_ip: false,
            art: None,
            fetched: Vec::new(),
            list: None,
            keyboard: Some(Keyboard {
                target: KeyTarget::Wifi(network()),
                secret: true,
                password: "Test".into(),
                key: 0,
                page: 0,
            }),
            refreshed: Instant::now(),
            ra: None,
        };
        let f = s.render();
        assert_eq!(f.rgba.len(), W * H * 4);
        if let Ok(path) = std::env::var("SLOT_WIFI_PREVIEW") {
            let mut e = png::Encoder::new(fs::File::create(path).unwrap(), W as u32, H as u32);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            e.write_header().unwrap().write_image_data(&f.rgba).unwrap();
        }
    }
}
