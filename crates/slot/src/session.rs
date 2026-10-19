use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot_input::{Action, Gestures, Millis, RawEvent};
use slot_retro::Rumble;
use slot_store::Platform;
use slot_ui::FfState;

pub const RUMBLE_MIN_MS: Millis = 100;
pub const RUMBLE_MAX_MS: Millis = 150;

use crate::app::{App, Phase};
use crate::audio::{opened_sink, AudioSink, Ring, Sfx};
use crate::cloud::CloudSync;
use crate::core::open_core;
use crate::emu::{CoreState, EmuHandle, Speed};
use crate::frames::FrameRef;
use crate::input::Pad;
use crate::persist;
use crate::prefetch::Prefetch;

pub struct Session {
    root: PathBuf,
    app: App,
    emu: Option<EmuHandle>,
    sink: Box<dyn AudioSink>,
    gestures: Gestures,
    pad: Pad,
    rewinding: bool,
    fast: bool,
    motor: u16,
    pulse: Option<(Millis, u16)>,
    prefetch: Prefetch,
    reloading: bool,
    driven: bool,
    ra: RaLogin,
    cloud: Option<CloudSync>,
}

impl Session {
    pub fn boot(root: PathBuf) -> Self {
        Self::boot_with(root, opened_sink())
    }

    pub fn boot_with(root: PathBuf, sink: Box<dyn AudioSink>) -> Self {
        let mut session = Session {
            app: App::boot(&root),
            root,
            emu: None,
            sink,
            gestures: if cfg!(feature = "brick") {
                Gestures::brick()
            } else {
                Gestures::new()
            },
            prefetch: Prefetch::default(),
            pad: Pad::default(),
            rewinding: false,
            fast: false,
            motor: 0,
            pulse: None,
            reloading: false,
            driven: false,
            ra: RaLogin::default(),
            cloud: None,
        };
        session.cloud = CloudSync::boot(&session.root);
        if crate::ra::load(&session.root).is_none() && old_ra_config(&session.root) {
            eprintln!("slot: cheevos: an old RetroAchievements config file is no longer read");
            session
                .app
                .show_message("RetroAchievements", "Log in from HOME > RetroAchievements");
        }
        // A game left running comes straight back. Its core loads on its own thread, so start it
        // now and let the card read overlap the rest of the boot. Waiting for the first update
        // left a cold boot (after a power off) idle on the slot for the whole load.
        if session.inserting() {
            session.sync_core();
        }
        session
    }

    pub fn play_sfx(&mut self, sfx: Sfx) {
        let ring = self.sink.ring();
        let rate = ring.sample_rate();
        if rate == 0 {
            return;
        }
        let mut samples = sfx.render(rate);
        crate::audio::volume::apply(&mut samples, self.app.output_volume());
        ring.mix(&samples);
    }

    pub fn audio_queued(&self) -> usize {
        self.sink.ring().queued_frames()
    }

    pub fn audio_ring(&self) -> std::sync::Arc<Ring> {
        self.sink.ring()
    }

    pub fn rumble(&mut self, strength: u16) {
        if strength == self.motor {
            return;
        }
        self.motor = strength;
        self.app.set_rumble(strength);
    }

    pub fn prefetching(&self) -> Option<&std::path::Path> {
        self.prefetch.reading()
    }

    pub fn core_rumble(&self) -> Option<&Rumble> {
        self.emu.as_ref().map(EmuHandle::rumble)
    }

    pub fn app(&self) -> &App {
        &self.app
    }

    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    pub fn emu(&self) -> Option<&EmuHandle> {
        self.emu.as_ref()
    }

    pub fn set_driven(&mut self, driven: bool) {
        self.driven = driven;
        if let Some(emu) = &self.emu {
            emu.set_driven(driven);
        }
    }

    pub fn step_emulator(&self, present: Duration, timeout: Duration) -> bool {
        self.emu.as_ref().is_some_and(|emu| {
            emu.tick(present);
            emu.wait_frame(timeout)
        })
    }

    pub fn frame(&self) -> Option<FrameRef> {
        self.emu.as_ref().and_then(|e| e.latest_frame())
    }

    pub fn core_settling(&self) -> bool {
        self.emu
            .as_ref()
            .is_some_and(|e| e.state() == CoreState::Loading)
    }

    pub fn has_core(&self) -> bool {
        self.emu.is_some()
    }

    pub fn frame_ready(&self) -> bool {
        self.emu.as_ref().is_some_and(EmuHandle::frame_ready)
    }

    pub fn frame_size(&self) -> Option<(u32, u32)> {
        self.emu.as_ref().and_then(EmuHandle::frame_size)
    }

    pub fn frames_published(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::published_count)
    }

    pub fn observed_speed(&self) -> Option<Speed> {
        self.emu.as_ref().map(EmuHandle::observed_speed)
    }

    pub fn frames_taken(&self) -> u64 {
        self.emu.as_ref().map_or(0, EmuHandle::frames_taken)
    }

    pub fn game_visible(&self) -> bool {
        self.app.game_visible()
    }

    pub fn feed(&mut self, events: impl IntoIterator<Item = RawEvent>, now: Millis) {
        let mut actions = Vec::new();
        let latched = self.gestures.ff_latched();
        self.gestures.set_repair_chord(self.app.repair_offered());
        for ev in events {
            actions.extend(self.gestures.feed(ev, now));
        }
        actions.extend(self.gestures.tick(now));
        if !latched && self.app.in_game_menu().is_some() {
            actions.extend(self.gestures.drop_ff_latch());
        }
        for action in actions {
            self.act(action);
        }
        self.sync_pad();
    }

    #[cfg(feature = "brick")]
    pub fn reset_sleep_input(&mut self) {
        let triggers = self.gestures.triggers_to_game();
        self.gestures = Gestures::brick();
        self.gestures.set_triggers_to_game(triggers);
        self.pad.clear();
        self.rewinding = false;
        self.fast = false;
        self.sync_pad();
    }

    pub fn set_menu_dpad_display(&mut self, enabled: bool) {
        self.gestures.set_menu_dpad_display(enabled);
    }

    fn sync_pad(&mut self) {
        for btn in self.app.taken_buttons() {
            self.pad.apply(Action::GbaUp(*btn));
        }
        if let Some(emu) = &self.emu {
            emu.set_input(self.pad.mask());
        }
    }

    fn bridge_link(&mut self, f: impl FnOnce(&mut App)) {
        let had_link = self.app.link_active();
        f(&mut self.app);
        if had_link && !self.app.link_active() {
            if let Some(emu) = &self.emu {
                emu.end_link();
            }
        }
    }

    fn act(&mut self, action: Action) {
        // Keep the existing Select+B shortcut as an alias on Brick.
        #[cfg(feature = "brick")]
        let action = if action == Action::ShaderPrev {
            Action::LedToggle
        } else {
            action
        };
        if trace() {
            eprintln!("slot: {action:?} in {:?}", self.app.phase());
        }
        let menu_open = self.app.in_game_menu().is_some();
        match action {
            Action::RewindStart if !menu_open => self.rewinding = true,
            Action::RewindStop => self.rewinding = false,
            Action::FfStart if !menu_open => self.fast = true,
            Action::FfStop => self.fast = false,
            _ => {}
        }
        let menu = self.overlaid();
        self.bridge_link(|app| app.apply(action));
        if matches!(
            action,
            Action::VolumeUp | Action::VolumeDown | Action::MuteToggle
        ) {
            if let Some(emu) = &self.emu {
                emu.set_volume(self.app.output_volume());
            }
        }
        if menu || self.overlaid() {
            self.pad.clear();
        } else if self.app.takes_from_the_game(action) {
            if let Action::GbaDown(btn) | Action::GbaUp(btn) = action {
                self.pad.apply(Action::GbaUp(btn));
            }
        } else {
            self.pad.apply(action);
        }
        self.sync_rumble();
    }

    pub fn update(&mut self, dt: f32) {
        self.bridge_link(|app| app.update(dt));
        if let Some(emu) = &self.emu {
            emu.set_volume(self.app.output_volume());
        }
        if let Some((client_id, transport)) = self.app.take_link_transport() {
            match &self.emu {
                Some(emu) => match self.app.link_player() {
                    Some(player) => emu.begin_cable(player, transport),
                    None => emu.begin_link(client_id, transport),
                },
                None => eprintln!("slot: link: a transport arrived with no core to run it"),
            }
        }
        if let Some(emu) = &self.emu {
            self.app.set_discs(emu.discs());
            if let Some(index) = self.app.take_disc_change() {
                emu.insert_disc(index);
            }
        }
        let mut recoloured = false;
        if let Some(p) = self.app.take_gb_palette() {
            if let Some(emu) = &self.emu {
                emu.set_option("mgba_gb_colors", p.core_name());
                recoloured = true;
            }
        }
        if let Some(on) = self.app.take_colour_correction() {
            if let Some((key, value)) =
                crate::core::colour_option(self.app.core(), self.app.playing_platform(), on)
            {
                if let Some(emu) = &self.emu {
                    emu.set_option(key, value);
                    recoloured = true;
                }
            }
        }
        if recoloured && self.held() && !self.app.link_active() {
            if let Some(emu) = &self.emu {
                emu.refresh();
            }
        }
        if let Some((stem, serial)) = self.app.take_link_reload() {
            self.reload_for_link(&stem, serial);
        }
        if self.app.link_active() {
            if self.emu.as_ref().is_some_and(EmuHandle::bios_mismatch) {
                self.bridge_link(|app| app.bios_mismatch());
            } else if self.emu.as_ref().is_some_and(EmuHandle::peer_ended) {
                self.bridge_link(|app| app.peer_ended());
            } else if self.emu.as_ref().is_some_and(EmuHandle::link_lost) {
                self.app.peer_lost();
            }
        }
        if let Some(sfx) = self.app.take_sfx() {
            self.play_sfx(sfx);
        }
        self.sync_core();
        self.sync_reload();
        self.sync_achievements();
        if let Some(cloud) = self.cloud.as_mut() {
            cloud.update(self.emu.is_some(), &mut self.app);
        }
        if !self.reloading {
            if let Some((stem, serial)) = self.app.take_solo_reload() {
                eprintln!("slot: link: restoring single-player mode");
                self.reload_for_link(&stem, serial);
            }
        }
        if !self.has_core() {
            for action in self.gestures.drop_ff_latch() {
                self.act(action);
            }
        }
        let ready = self.emu.as_ref().is_some_and(EmuHandle::has_published);
        if ready && !self.app.game_ready() {
            launch("first frame shown");
        }
        self.app.set_game_ready(ready);
        self.sync_speed();
        self.sync_rewind_hud();
        self.sync_ff_hud();
        self.sync_rumble();
        self.sync_pad();
        let now = self.app.now();
        self.prefetch.browse(self.app.browsed_rom(), now);
    }

    fn sync_rumble(&mut self) {
        let asked = match &self.emu {
            Some(emu) if self.playing() && self.app.rumble_enabled() => emu.rumble().strength(),
            _ => {
                self.pulse = None;
                return self.rumble(0);
            }
        };
        let now = self.app.now();
        let want = match (asked, self.pulse) {
            (0, Some((at, strength))) if now.saturating_sub(at) < RUMBLE_MIN_MS => strength,
            (0, _) => {
                self.pulse = None;
                0
            }
            (asked, Some((at, _))) => {
                self.pulse = Some((at, asked));
                if now.saturating_sub(at) < RUMBLE_MAX_MS {
                    asked
                } else {
                    0
                }
            }
            (asked, None) => {
                self.pulse = Some((now, asked));
                asked
            }
        };
        self.rumble(want);
    }

    fn sync_ff_hud(&mut self) {
        let ff = match (self.actually_fast_forwarding(), self.gestures.ff_latched()) {
            (false, _) => FfState::Off,
            (true, false) => FfState::Held,
            (true, true) => FfState::Latched,
        };
        self.app.set_ff(ff);
    }

    fn sync_rewind_hud(&mut self) {
        let fill = self
            .actually_rewinding()
            .then(|| self.emu.as_ref().map(EmuHandle::rewind_fill))
            .flatten();
        match fill {
            Some(fill) => self.app.show_rewind(fill),
            None => self.app.hide_rewind(),
        }
    }

    pub(crate) fn actually_rewinding(&self) -> bool {
        self.rewinding && self.playing() && self.app.may_rewind()
    }

    pub(crate) fn actually_fast_forwarding(&self) -> bool {
        self.fast && self.playing() && self.app.may_fast_forward()
    }

    fn inserting(&self) -> bool {
        matches!(self.app.phase(), Phase::Inserting { .. })
    }

    fn showing_polaroids(&self) -> bool {
        matches!(self.app.phase(), Phase::Polaroids { .. })
    }

    fn overlaid(&self) -> bool {
        self.showing_polaroids() || self.held()
    }

    fn playing(&self) -> bool {
        matches!(self.app.phase(), Phase::Playing { .. }) && !self.held()
    }

    fn held(&self) -> bool {
        self.app.game_menu_open()
            || self.app.in_game_menu().is_some()
            || self.app.repair_prompt()
            || self.app.shutting_down()
    }

    fn dozing(&self) -> bool {
        matches!(self.app.phase(), Phase::Doze { .. })
    }

    fn ejecting(&self) -> bool {
        matches!(self.app.phase(), Phase::Ejecting { .. })
    }

    fn sync_speed(&self) {
        if let Some(emu) = &self.emu {
            emu.set_fast_steps(u32::from(self.app.ff_speed()));
            emu.set_ff_sound(self.app.ff_sound());
            emu.set_speed(
                if self.inserting()
                    || self.ejecting()
                    || self.showing_polaroids()
                    || self.dozing()
                    || (self.held() && !self.app.link_active())
                {
                    Speed::Paused
                } else if self.actually_fast_forwarding() {
                    Speed::Fast
                } else {
                    Speed::Normal
                },
            );
            emu.set_rewinding(self.actually_rewinding());
            emu.set_turbo(self.app.turbo());
            emu.set_rewind_recording(self.app.records_rewind());
        }
    }

    fn no_core() -> bool {
        std::env::var_os("SLOT_NO_CORE").is_some_and(|v| v != "0")
    }

    fn sync_core(&mut self) {
        if Self::no_core() {
            return;
        }
        let stem = match self.app.phase() {
            Phase::Shelf => {
                self.emu = None;
                return;
            }
            Phase::Inserting { cart, .. } => cart.clone(),
            _ => return,
        };
        if self.emu.is_none() {
            let (_, serial) = self.app.link_mode(&stem);
            self.spawn_core(&stem, serial);
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => {
                if let (true, Some(p), Some(emu)) =
                    (self.app.palette_live(), self.app.gb_palette(), &self.emu)
                {
                    emu.set_option("mgba_gb_colors", p.core_name());
                }
                self.app.on_core_ready()
            }
            Some(CoreState::Failed) | None => {
                self.emu = None;
                self.app.on_core_failed();
            }
        }
    }

    fn spawn_core(&mut self, stem: &str, serial: &'static str) {
        let Some((rom, platform)) = self
            .app
            .seated_cart()
            .filter(|c| c.stem == stem)
            .map(|c| (c.rom.clone(), c.platform))
        else {
            return;
        };
        self.gestures.set_triggers_to_game(platform.uses_triggers());
        if let Some(names) = platform.missing_bios(&crate::root::bios_dir(&self.root)) {
            // No core is started, so the cart comes back out with this said.
            self.app
                .show_message("BIOS missing", format!("Put {names} in Slot/BIOS"));
            return;
        }
        if let Some(cloud) = self.cloud.as_mut() {
            cloud.game_started(stem, platform);
        }
        let core = slot_store::core_for_platform(&self.root, stem, platform);
        launch(&format!("opening {stem} on {}", core.as_str()));
        self.app.set_core(core);
        self.app.set_platform(platform);
        self.app
            .set_video_mode(crate::video_mode::video_mode_for_platform(
                &self.root, stem, platform,
            ));
        self.app.set_link_loaded(serial);
        let resume = (!self.app.starting_clean())
            .then(|| persist::read_resume(&self.root, platform, core, stem))
            .flatten();
        launch("resume state read");
        let player = self.app.link_player();
        let palette = crate::core::palette_for(self.app.gb_palette(), platform, &rom, player);
        self.app.set_palette_live(palette.is_some());
        let mut opened = open_core(
            &self.root,
            core,
            platform,
            serial,
            self.app.colour_correction_for(platform),
            palette,
            player,
        );
        if matches!(platform, Platform::Ws | Platform::Wsc) {
            // Held the way the game's header says: upright games are shown upright.
            let held = crate::core::wonderswan_held(&rom);
            opened.core.set_option("wswan_rotate_display", held);
        }
        self.app.set_named_core(opened.named);
        launch("core opened");
        let sav = persist::read_sav(&self.root, platform, stem);
        launch("save read");
        let ring = self.sink.ring();
        let emu = match player.filter(|_| platform == Platform::Gba) {
            Some(p) => EmuHandle::spawn_linked(opened.core, rom, ring, sav, resume, p),
            None => EmuHandle::spawn(opened.core, rom, ring, sav, resume),
        };
        emu.set_volume(self.app.output_volume());
        emu.set_native_xy(platform == Platform::Snes);
        emu.set_driven(self.driven);
        // Logged in to RetroAchievements: this game earns achievements (softcore).
        self.ra = RaLogin::default();
        if let Some(account) = crate::ra::load(&self.root) {
            let console = match platform {
                Platform::Gba => slot_cheevos::Console::Gba,
                Platform::Gb => slot_cheevos::Console::Gb,
                Platform::Gbc => slot_cheevos::Console::Gbc,
                Platform::Nes => slot_cheevos::Console::Nes,
                Platform::Snes => slot_cheevos::Console::Snes,
                Platform::Gg => slot_cheevos::Console::Gg,
                Platform::Sms => slot_cheevos::Console::Sms,
                Platform::Md => slot_cheevos::Console::Md,
                Platform::Ngp | Platform::Ngpc => slot_cheevos::Console::Ngp,
                Platform::Ws | Platform::Wsc => slot_cheevos::Console::Ws,
                Platform::Scd => slot_cheevos::Console::Scd,
                Platform::S32x => slot_cheevos::Console::S32x,
                Platform::Ps => slot_cheevos::Console::Ps,
            };
            emu.achievements(&account.user, &account.token, console);
            self.ra.console = Some(console);
        }
        self.app.set_snapshot(Box::new(emu.snapshot()));
        self.emu = Some(emu);
    }

    fn sync_achievements(&mut self) {
        let Some(emu) = &self.emu else {
            self.ra = RaLogin::default();
            return;
        };
        crate::badge::prefetch(&self.root, emu.take_badge_urls());
        for notice in emu.take_achievement_notices() {
            match &notice {
                // Signing in at game start often beats Wi-Fi: try again once it is up,
                // rather than going without achievements for the whole game.
                slot_cheevos::Notice::LoggedIn(Err(e)) if self.ra.tries < RA_RETRIES => {
                    eprintln!("slot: cheevos: sign-in failed ({e}); trying again when online");
                    if self.ra.tries == 0 {
                        self.app
                            .announce("Not logged in", "Trying again once online");
                    }
                    self.ra.tries += 1;
                    self.ra.retry_at = Some(Instant::now() + RA_RETRY_AFTER);
                    continue;
                }
                // Follows a failed sign-in; the retry loads the game again.
                slot_cheevos::Notice::GameLoaded(Err(_)) if self.ra.retry_at.is_some() => continue,
                _ => {}
            }
            let badge = match &notice {
                slot_cheevos::Notice::Unlocked { badge, .. }
                | slot_cheevos::Notice::GameCompleted { badge, .. }
                | slot_cheevos::Notice::GameLoaded(Ok((_, badge))) => Some(badge.clone()),
                _ => None,
            };
            let badge = badge.and_then(|url| {
                crate::badge::prefetch(&self.root, vec![url.clone()]);
                crate::badge::path(&self.root, &url)
            });
            self.app.achievement_notice(notice, badge);
        }
        let (Some(at), Some(console)) = (self.ra.retry_at, self.ra.console) else {
            return;
        };
        if Instant::now() < at || !net_online() {
            return;
        }
        self.ra.retry_at = None;
        if let Some(account) = crate::ra::load(&self.root) {
            eprintln!(
                "slot: cheevos: signing in again (try {})",
                self.ra.tries + 1
            );
            emu.achievements(&account.user, &account.token, console);
        }
    }

    fn reload_for_link(&mut self, stem: &str, serial: &'static str) {
        eprintln!("slot: link: loading {stem} again with gpsp_serial={serial}");
        if self.emu.is_some() {
            self.app.flush_resume();
        }
        self.emu = None;
        self.spawn_core(stem, serial);
        self.reloading = true;
    }

    fn sync_reload(&mut self) {
        if !self.reloading {
            return;
        }
        match self.emu.as_ref().map(EmuHandle::state) {
            Some(CoreState::Loading) => {}
            Some(CoreState::Ready) => {
                self.reloading = false;
                self.app.link_reload_done();
            }
            Some(CoreState::Failed) | None => {
                self.reloading = false;
                self.emu = None;
                self.app.link_reload_failed();
                if let Some((stem, serial)) = self.app.take_link_reload() {
                    self.reload_for_link(&stem, serial);
                }
            }
        }
    }
}

/// Signing in to RetroAchievements again after it failed: how many times, and how soon.
const RA_RETRIES: u32 = 5;
const RA_RETRY_AFTER: Duration = Duration::from_secs(10);

/// The game's RetroAchievements sign-in: its console, and the retries after a failure.
#[derive(Default)]
struct RaLogin {
    console: Option<slot_cheevos::Console>,
    tries: u32,
    retry_at: Option<Instant>,
}

/// Wi-Fi has an address, so a sign-in can reach the server.
fn net_online() -> bool {
    #[cfg(feature = "brick")]
    return crate::brick_wifi::online();
    #[cfg(not(feature = "brick"))]
    true
}

/// A login file from the earlier RetroAchievements build (plain or saved by TextEdit),
/// which the sign-in screen has replaced. Only its name is looked at.
fn old_ra_config(root: &std::path::Path) -> bool {
    std::fs::read_dir(root.join("Config")).is_ok_and(|d| {
        d.flatten().any(|e| {
            e.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("retroachievements.ini")
        })
    })
}

pub(crate) fn trace() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("SLOT_TRACE").is_some())
}

fn launch(step: &str) {
    eprintln!("slot: launch: {step} at {:.1} ms", crate::boot_time::ms());
}
