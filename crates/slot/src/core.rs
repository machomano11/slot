use std::path::{Path, PathBuf};

use slot_retro::{LibretroCore, MockCore, RetroCore};
use slot_store::{Core, GbColors, Platform};

use crate::root;

const CORE_ENV: &str = "SLOT_CORE";

pub fn dylib_name(core: Core) -> String {
    format!(
        "{}_libretro.{}",
        core.as_str(),
        std::env::consts::DLL_EXTENSION
    )
}

fn candidates(root: &Path, core: Core) -> Vec<PathBuf> {
    if let Some(named) = std::env::var_os(CORE_ENV) {
        return vec![PathBuf::from(named)];
    }
    let name = dylib_name(core);
    let mut paths = vec![root.join("System").join(&name)];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        paths.push(dir.join(&name));
        paths.push(dir.join("vendor").join(&name));
    }
    paths.push(Path::new("vendor").join(&name));
    paths
}

pub struct Opened {
    pub core: Box<dyn RetroCore>,
    pub named: bool,
}

pub fn open_core(
    root: &Path,
    core: Core,
    platform: Platform,
    serial: &str,
    colour: bool,
    palette: Option<GbColors>,
    link: Option<u8>,
) -> Opened {
    let paths = candidates(root, core);
    match open_named(root, core, platform, serial, colour, palette, link, &paths) {
        Some(core) => Opened { core, named: true },
        None => {
            report_missing(core, &paths);
            Opened {
                core: Box::new(MockCore::new()),
                named: false,
            }
        }
    }
}

pub fn open_core_for(
    root: &Path,
    core: Core,
    serial: &str,
    colour: bool,
    paths: &[PathBuf],
) -> Box<dyn RetroCore> {
    open_core_for_palette(root, core, Platform::Gba, serial, colour, None, paths)
}

pub fn open_core_for_palette(
    root: &Path,
    core: Core,
    platform: Platform,
    serial: &str,
    colour: bool,
    palette: Option<GbColors>,
    paths: &[PathBuf],
) -> Box<dyn RetroCore> {
    open_named(root, core, platform, serial, colour, palette, None, paths).unwrap_or_else(|| {
        report_missing(core, paths);
        Box::new(MockCore::new())
    })
}

#[allow(clippy::too_many_arguments)]
fn open_named(
    root: &Path,
    core: Core,
    platform: Platform,
    serial: &str,
    colour: bool,
    palette: Option<GbColors>,
    link: Option<u8>,
    paths: &[PathBuf],
) -> Option<Box<dyn RetroCore>> {
    let bios = root::bios_dir(root);
    let saves = root::saves_dir(root);
    for path in paths {
        if !path.exists() {
            continue;
        }
        match LibretroCore::open_with(path, &bios, &saves) {
            Ok(mut opened) => {
                let console = match core {
                    Core::Fceumm => Some(Platform::Nes),
                    Core::Snes9x => Some(Platform::Snes),
                    Core::Mgba | Core::Gpsp => None,
                    _ => Some(platform),
                };
                if let Some((w, h)) = console.map(Platform::canvas) {
                    opened.set_canvas(w, h);
                }
                apply_core_options(
                    &mut opened,
                    core,
                    platform,
                    serial,
                    root::has_real_bios(root),
                    colour,
                    palette,
                );
                if let Some(player) = link {
                    apply_link_options(&mut opened, core, player);
                }
                eprintln!("slot: core {}", path.display());
                return Some(Box::new(opened));
            }
            Err(e) => eprintln!("slot: {}: {e}", path.display()),
        }
    }
    None
}

pub fn palette_for(
    chosen: GbColors,
    platform: Platform,
    rom: &Path,
    link: Option<u8>,
) -> Option<GbColors> {
    let game_boy = matches!(platform, Platform::Gb | Platform::Gbc);
    (link.is_none() && game_boy && dmg_only(rom)).then_some(chosen)
}

pub fn dmg_only(rom: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 0x150];
    std::fs::File::open(rom)
        .and_then(|mut f| f.read_exact(&mut header))
        .is_ok_and(|_| header[0x143] & 0x80 == 0)
}

fn report_missing(core: Core, paths: &[PathBuf]) {
    eprintln!(
        "slot: no {} core found, running the mock test pattern instead. Looked in: {}",
        core.as_str(),
        paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

/// How a WonderSwan game is held, for Beetle Cygne's display option: the footer's flags byte
/// (four from the end) has bit 0 set for a game played upright.
pub fn wonderswan_held(rom: &Path) -> &'static str {
    let upright = std::fs::read(rom)
        .ok()
        .and_then(|data| data.len().checked_sub(4).map(|i| data[i] & 1 == 1))
        .unwrap_or(false);
    if upright {
        "portrait"
    } else {
        "landscape"
    }
}

pub fn colour_option(
    which: Core,
    platform: Platform,
    on: bool,
) -> Option<(&'static str, &'static str)> {
    match which {
        Core::Mgba => Some((
            "mgba_color_correction",
            if on && platform == Platform::Gb {
                "GBC"
            } else if on {
                "Auto"
            } else {
                "OFF"
            },
        )),
        Core::Gpsp => Some((
            "gpsp_color_correction",
            if on { "enabled" } else { "disabled" },
        )),
        // A TV console has no LCD to correct for, and the other handhelds' cores have no
        // correction of their own.
        _ => None,
    }
}

pub fn game_boy_player(platform: Platform, rom: &Path) -> bool {
    platform == Platform::Gba
        && !slot_store::header_code(rom).is_some_and(|code| code.starts_with('M'))
}

pub fn apply_game_boy_player(
    core: &mut dyn RetroCore,
    which: Core,
    platform: Platform,
    rom: &Path,
) {
    if which == Core::Mgba {
        let on = game_boy_player(platform, rom);
        core.set_option("mgba_force_gbp", if on { "ON" } else { "OFF" });
    }
}

pub fn apply_link_options(core: &mut LibretroCore, which: Core, player: u8) {
    if which != Core::Mgba {
        return;
    }
    core.set_option("mgba_link", "on");
    core.set_option("mgba_link_player", &player.to_string());
    eprintln!("slot: core: link mode on, player {player}");
}

pub fn apply_core_options(
    core: &mut LibretroCore,
    which: Core,
    platform: Platform,
    serial: &str,
    bios: bool,
    colour: bool,
    palette: Option<GbColors>,
) {
    // FCEUmm and snes9x have no frameskip option; slot's own skipping covers them.
    if matches!(which, Core::Mgba | Core::Gpsp) {
        core.set_option(&format!("{}_frameskip", which.as_str()), "auto");
    }
    if which == Core::Snes9x {
        // A 256x224 picture like the NES's, and no HD Mode 7: it would cost the A53 dearly
        // and send frames twice as wide for nothing on this screen.
        for (key, value) in [
            ("snes9x_overscan", "enabled"),
            ("snes9x_mode7_hires", "disabled"),
        ] {
            core.set_option(key, value);
        }
    }
    if which == Core::Picodrive {
        // Slot scales the picture itself, so the core sends it square-pixelled; the Game
        // Gear's screen is shown as it was, without the LCD's smearing. A Mega Drive gets
        // the six-button pad: its X, Y and Z are the shoulders and the top face button.
        for (key, value) in [
            ("picodrive_input1", "6 button pad"),
            // The 32X's two SH2s through the recompiler: the interpreter is far too slow.
            ("picodrive_drc", "enabled"),
            ("picodrive_aspect", "PAR"),
            ("picodrive_ggghost", "off"),
            ("picodrive_region", "Auto"),
            ("picodrive_smstype", "Auto"),
        ] {
            core.set_option(key, value);
        }
    }
    if which == Core::PcsxRearmed {
        // A real BIOS in Slot/BIOS if there is one, else the built-in one; frames skipped
        // only when the audio would run dry; the memory card saved as Slot's .sav.
        for (key, value) in [
            ("pcsx_rearmed_bios", "auto"),
            ("pcsx_rearmed_show_bios_bootlogo", "disabled"),
            ("pcsx_rearmed_drc", "enabled"),
            ("pcsx_rearmed_frameskip_type", "auto"),
            ("pcsx_rearmed_memcard1", "libretro"),
            ("pcsx_rearmed_neon_enhancement_enable", "disabled"),
        ] {
            core.set_option(key, value);
        }
    }
    if which == Core::MednafenWswan {
        // Software rotation (Slot turns no frames itself), the pad turned with the screen.
        for (key, value) in [
            ("wswan_rotate_display", "landscape"),
            ("wswan_rotate_keymap", "auto"),
            ("wswan_60hz_mode", "enabled"),
        ] {
            core.set_option(key, value);
        }
    }
    if which == Core::MednafenNgp {
        core.set_option("ngp_language", "english");
    }
    if which == Core::Fceumm {
        // 8 lines off the top and bottom, as a TV hid them: the 256x224 Platform::Nes draws.
        for (key, value) in [
            ("fceumm_overscan_v_top", "8"),
            ("fceumm_overscan_v_bottom", "8"),
            ("fceumm_overscan_h_left", "0"),
            ("fceumm_overscan_h_right", "0"),
        ] {
            core.set_option(key, value);
        }
    }
    if which == Core::Mgba {
        core.set_option("mgba_sgb_borders", "OFF");
        match palette {
            Some(p) => {
                core.set_option("mgba_gb_model", "Game Boy");
                core.set_option("mgba_gb_colors_preset", "0");
                core.set_option("mgba_gb_colors", p.core_name());
            }
            None => {
                core.set_option("mgba_gb_model", "Autodetect");
                core.set_option("mgba_gb_colors_preset", "1");
                core.set_option("mgba_gb_colors", "GBC Dark Green →A");
            }
        }
        if let Some((key, value)) = colour_option(which, platform, colour) {
            core.set_option(key, value);
        }
    }
    if which == Core::Gpsp {
        core.set_option("gpsp_serial", serial);
        core.set_option("gpsp_rtc_time_source", "system");
        if bios {
            core.set_option("gpsp_boot_mode", "bios");
        }
        if let Some((key, value)) = colour_option(which, platform, colour) {
            core.set_option(key, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn each_core_spells_colour_correction_its_own_way() {
        assert_eq!(
            colour_option(Core::Mgba, Platform::Gba, true),
            Some(("mgba_color_correction", "Auto"))
        );
        assert_eq!(
            colour_option(Core::Mgba, Platform::Gba, false),
            Some(("mgba_color_correction", "OFF"))
        );
        assert_eq!(
            colour_option(Core::Gpsp, Platform::Gba, true),
            Some(("gpsp_color_correction", "enabled"))
        );
        assert_eq!(
            colour_option(Core::Gpsp, Platform::Gba, false),
            Some(("gpsp_color_correction", "disabled"))
        );
    }

    #[test]
    fn mgba_uses_gbc_correction_for_game_boy_games() {
        assert_eq!(
            colour_option(Core::Mgba, Platform::Gb, true),
            Some(("mgba_color_correction", "GBC"))
        );
    }

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn candidates_search_the_named_cores_own_filename_only() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);

        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        assert_ne!(mgba, gpsp, "the two cores searched the same paths");
        assert!(!mgba.is_empty());
        assert!(!gpsp.is_empty());
        for path in &mgba {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("mgba_libretro"), "{name} is not mGBA's");
        }
        for path in &gpsp {
            let name = path.file_name().unwrap().to_str().unwrap();
            assert!(name.starts_with("gpsp_libretro"), "{name} is not gpSP's");
        }
    }

    #[test]
    fn candidates_search_the_roots_own_system_directory() {
        let _g = lock();
        std::env::remove_var(CORE_ENV);
        let root = Path::new("/some/content/root");
        assert_eq!(
            candidates(root, Core::Gpsp)[0],
            root.join("System").join(dylib_name(Core::Gpsp)),
        );
    }

    #[test]
    fn the_env_override_ignores_which_core_was_asked_for() {
        let _g = lock();
        std::env::set_var(CORE_ENV, "/dev/null/named-core");
        let root = Path::new("/root");
        let mgba = candidates(root, Core::Mgba);
        let gpsp = candidates(root, Core::Gpsp);
        std::env::remove_var(CORE_ENV);
        assert_eq!(mgba, vec![PathBuf::from("/dev/null/named-core")]);
        assert_eq!(
            mgba, gpsp,
            "the override stopped winning for one of the cores"
        );
    }
}
