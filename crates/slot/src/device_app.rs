use std::path::PathBuf;
use std::time::{Duration, Instant};

use slot::frontend::Frontend;
#[cfg(feature = "brick")]
use slot::input::BrickInput;
#[cfg(not(feature = "brick"))]
use slot::input::DeviceInput;
use slot::panel_turn::{panel_turn, PanelTurn};
use slot_gfx::{Compositor, FbdevSurface, Surface};
use slot_power::trace_first_frame;
#[cfg(feature = "brick")]
use slot_power::BrickPlatform;
#[cfg(not(feature = "brick"))]
use slot_power::DevicePlatform;

const CARD: &str = "/mnt/sdcard";

const MIN_FRAME: Duration = Duration::from_millis(12);

const STEP_TIMEOUT: Duration = Duration::from_millis(12);

const MARGIN: Duration = Duration::from_micros(1500);

struct Pacer {
    last_return: Option<Instant>,
    period: Duration,
    work: Duration,
    works: [Duration; WORK_WINDOW],
    next: usize,
}

const WORK_WINDOW: usize = 32;

impl Pacer {
    fn new() -> Self {
        Pacer {
            last_return: None,
            period: Duration::from_micros(16_760),
            work: Duration::from_millis(4),
            works: [Duration::from_millis(4); WORK_WINDOW],
            next: 0,
        }
    }

    fn wait(&self) {
        let Some(last) = self.last_return else {
            return;
        };
        let delay = self.period.saturating_sub(self.work + MARGIN);
        if let Some(left) = (last + delay).checked_duration_since(Instant::now()) {
            std::thread::sleep(left);
        }
    }

    fn swapped(&mut self, work: Duration, blocked: bool) -> bool {
        let now = Instant::now();
        let mut missed = false;
        if let Some(last) = self.last_return {
            let seen = now - last;
            missed = seen > self.period.mul_f64(1.5);
            if blocked && seen > Duration::from_millis(12) && seen < Duration::from_millis(22) {
                self.period = self.period.mul_f64(0.95) + seen.mul_f64(0.05);
            }
        }
        self.works[self.next] = work;
        self.next = (self.next + 1) % WORK_WINDOW;
        self.work = self.works.iter().copied().max().unwrap_or(work);
        self.last_return = Some(now);
        missed
    }
}

/// Runs System/sleep.sh without going deaf: a Power press that lands while the
/// script is still preparing (Wi-Fi stop, sync) cancels the suspend instead of
/// being swallowed, which used to need a second press to wake. Once the script
/// has announced the kernel request (/tmp/slot-suspending) the press is the wake key.
#[cfg(feature = "brick")]
fn run_sleep_script(
    root: &std::path::Path,
    input: &mut BrickInput,
    frontend: &Frontend,
) -> (std::io::Result<std::process::ExitStatus>, bool) {
    use slot_input::{Btn, InputSource, RawEvent};
    const SUSPENDING: &str = "/tmp/slot-suspending";
    let _ = std::fs::remove_file(SUSPENDING);
    let mut child = match std::process::Command::new("/bin/sh")
        .arg(root.join("System/sleep.sh"))
        .env("SLOT_ROOT", root)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (Err(e), false),
    };
    // Any Power press seen while the script runs means "wake up", even when it
    // came too late to cancel and only made the kernel abort the suspend.
    let mut pressed = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return (Ok(status), pressed),
            Ok(None) => {}
            Err(e) => return (Err(e), pressed),
        }
        if !pressed
            && input
                .poll(frontend.now())
                .into_iter()
                .any(|e| matches!(e, RawEvent::Down(Btn::Power)))
        {
            pressed = true;
            if !std::path::Path::new(SUSPENDING).exists() {
                eprintln!("slot: sleep: Power pressed while preparing; cancelling the suspend");
                unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn run() {
    slot::boot_time::mark("start");
    let _ = std::thread::Builder::new()
        .name("slot-fonts".into())
        .spawn(slot_ui::warm_fonts);
    let audio = std::thread::Builder::new()
        .name("slot-audio-open".into())
        .spawn(slot::audio::opened_sink);
    let root = std::env::var_os("SLOT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(CARD));
    let mut surface = match FbdevSurface::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("slot: {e}");
            std::process::exit(1);
        }
    };
    slot::boot_time::mark("surface");
    let mut compositor = match Compositor::new(&surface) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("slot: {e}");
            std::process::exit(1);
        }
    };
    slot::boot_time::mark("compositor");
    let panel = surface.window_size();
    let env = |key| std::env::var(key).ok();
    let turn = panel_turn(
        panel,
        slot::baseos::var("BASEOS_PANEL_ROTATION_CCW").as_deref(),
        env("SLOT_ROTATE").as_deref(),
    );
    let turned = turn != PanelTurn::None;
    let window = if turned { (panel.1, panel.0) } else { panel };
    eprintln!(
        "slot: baseos target {}, model {}, rotation ccw {}",
        slot::baseos::var("BASEOS_TARGET").unwrap_or_default(),
        slot::baseos::var("BASEOS_MODEL").unwrap_or_default(),
        slot::baseos::var("BASEOS_PANEL_ROTATION_CCW").unwrap_or_default()
    );
    eprintln!(
        "slot: panel {}x{}, canvas {}x{}, turn {turn:?}",
        panel.0, panel.1, window.0, window.1
    );
    if turned {
        if let Err(e) = compositor.set_panel_turn(panel, turn == PanelTurn::Ccw) {
            eprintln!("slot: {e}");
            return;
        }
    }
    #[cfg(not(feature = "brick"))]
    let platform = DevicePlatform::new(root.clone());
    #[cfg(not(feature = "brick"))]
    {
        eprintln!("slot: {}", platform.report());
        platform.trace_boot();
    }
    #[cfg(feature = "brick")]
    let platform = BrickPlatform::new(root.clone());
    slot::boot_time::mark("platform");
    let sink = match audio.map(|h| h.join()) {
        Ok(Ok(sink)) => sink,
        _ => slot::audio::opened_sink(),
    };
    slot::boot_time::mark("audio");
    let mut frontend = Frontend::boot_with(Box::new(platform), sink);
    slot::boot_time::mark("frontend");
    frontend.upload_boot_faces(&mut compositor);
    slot::boot_time::mark("faces");
    frontend.upload_bezel(&mut compositor, window);
    slot::boot_time::mark("bezel");
    #[cfg(not(feature = "brick"))]
    let mut input = DeviceInput::open(&root);
    #[cfg(feature = "brick")]
    let mut input = match BrickInput::open(&root) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("slot: {e}");
            std::process::exit(1);
        }
    };
    slot::boot_time::mark("input");
    #[cfg(not(feature = "brick"))]
    let card = root.clone();
    #[cfg(not(feature = "brick"))]
    let _ = std::thread::Builder::new()
        .name("slot-bootlogo".into())
        .spawn(move || slot::bootlogo::refresh(&card));
    #[cfg(feature = "brick")]
    let mut cpu = slot::brick_power::CpuPower::open(&root);
    let mut drawn = false;
    frontend.drive_emulator();
    let mut pacer = Pacer::new();
    let (mut frames, mut missed, mut early) = (0u32, 0u32, 0u32);
    #[cfg(feature = "brick")]
    let mut sleep_since: Option<Instant> = None;
    #[cfg(feature = "brick")]
    let mut sleep_attempted = false;
    #[cfg(feature = "brick")]
    let mut sync_hold_said = false;
    loop {
        pacer.wait();
        let began = Instant::now();
        frontend.advance(&mut input);
        #[cfg(feature = "brick")]
        if input.exit_requested() {
            cpu.restore();
            frontend.exit_to_launcher();
            return;
        }
        if frontend.restarting() {
            #[cfg(feature = "brick")]
            cpu.restore();
            frontend.restart();
        }
        if frontend.powering_off() {
            #[cfg(feature = "brick")]
            cpu.restore();
            frontend.poweroff();
            return;
        }
        #[cfg(feature = "brick")]
        {
            frontend.idle_sleep();
            cpu.mode(frontend.power_mode(), frontend.audio_underruns());
            if matches!(frontend.app().phase(), slot::app::Phase::Doze { .. }) {
                let since = *sleep_since.get_or_insert_with(Instant::now);
                // Let the paused audio sink release ALSA and pending saves finish.
                let holding = frontend.app().holding_for_sync();
                if holding && !sync_hold_said {
                    sync_hold_said = true;
                    eprintln!("slot: sleep: waiting for cloud sync to finish");
                }
                if !sleep_attempted && !holding && since.elapsed() >= Duration::from_millis(3300) {
                    sleep_attempted = true;
                    if frontend.prepare_sleep() {
                        slot::brick_link::quiesce();
                        let suspended_before = slot::brick_power::suspended_seconds();
                        let (result, cancelled) = run_sleep_script(&root, &mut input, &frontend);
                        let returned = Instant::now();
                        let suspended = slot::brick_power::suspended_seconds()
                            .zip(suspended_before)
                            .map(|(a, b)| (a - b).max(0.0));
                        let confirmed = suspended.is_some_and(|v| v > 0.05);
                        cpu.mode(slot::brick_power::Mode::Boost, frontend.audio_underruns());
                        let woke = match &result {
                            Ok(status) if status.success() || confirmed || cancelled => {
                                frontend.resume_sleep(&mut input);
                                sleep_since = None;
                                sleep_attempted = false;
                                true
                            }
                            other => {
                                eprintln!("slot: suspend unavailable ({other:?}); using standby");
                                false
                            }
                        };
                        {
                            use std::io::Write;
                            if let Ok(mut log) = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open(root.join("Logs/sleep.log"))
                            {
                                let _=writeln!(log,"Slot verification: suspended_seconds={suspended:?}, confirmed={confirmed}, power_pressed_during_sleep_entry={cancelled}, helper={result:?}, screen_on={woke} {} ms after the helper returned",returned.elapsed().as_millis());
                            }
                        }
                    }
                }
                pacer = Pacer::new();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            sleep_since = None;
            sleep_attempted = false;
            sync_hold_said = false;
        }
        #[cfg(feature = "brick")]
        if !frontend.needs_render() {
            pacer = Pacer::new();
            std::thread::sleep(Duration::from_millis(25));
            continue;
        }
        let frame_ready = frontend.step_emulator(pacer.period, STEP_TIMEOUT);
        #[cfg(not(feature = "brick"))]
        let _ = frame_ready;
        frontend.render(&mut compositor, window);
        let swap = Instant::now();
        let work = swap - began;
        #[cfg(feature = "brick")]
        cpu.sample(frame_ready, work, pacer.period, frontend.audio_underruns());
        if let Err(e) = surface.swap() {
            eprintln!("slot: {e}");
            #[cfg(feature = "brick")]
            cpu.restore();
            std::process::exit(1);
        }
        let swap_took = swap.elapsed();
        slot::latency::swapped(swap_took.as_secs_f64() * 1000.0);
        let dropped = pacer.swapped(work, swap_took > Duration::from_millis(1));
        if slot::latency::tracing() {
            frames += 1;
            missed += u32::from(dropped);
            early += u32::from(swap_took > pacer.period / 2);
            if frames == 600 {
                eprintln!(
                    "slot: pace: 600 frames, {missed} missed a latch, {early} drawn early, work {:.1} ms, period {:.2} ms",
                    pacer.work.as_secs_f64() * 1e3,
                    pacer.period.as_secs_f64() * 1e3
                );
                (frames, missed, early) = (0, 0, 0);
            }
        }
        if !drawn {
            drawn = true;
            slot::boot_time::mark("first frame");
            trace_first_frame();
            #[cfg(feature = "brick")]
            if let Some(path) = std::env::var_os("SLOT_READY_FILE") {
                if let Err(e) = std::fs::write(path, "first frame displayed\n") {
                    eprintln!("slot: ready marker: {e}");
                }
            }
        }
        if let Some(left) = MIN_FRAME.checked_sub(began.elapsed()) {
            std::thread::sleep(left);
        }
    }
}
