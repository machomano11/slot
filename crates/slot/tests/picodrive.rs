mod common;

use slot_store::{Core, Platform};

fn dylib() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(Core::Picodrive))
}

/// A 32 KB cart whose program turns the display on, paints every palette entry white (the
/// backdrop is one of them) and loops. `region` is the header's region nibble: 4 for a Master
/// System export cart, 6 for a Game Gear one.
fn white_screen_rom(region: u8) -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    let program = [
        0xf3, // DI
        0x3e, 0x40, 0xd3, 0xbf, 0x3e, 0x81, 0xd3, 0xbf, // VDP register 1 = 0x40: display on
        0x3e, 0x00, 0xd3, 0xbf, 0x3e, 0xc0, 0xd3, 0xbf, // VDP address: CRAM 0
        0x06, 0x40, 0x3e, 0xff, // LD B, 64; LD A, 0xFF
        0xd3, 0xbe, 0x10, 0xfc, // OUT (0xBE), A; DJNZ back: CRAM all white
        0x18, 0xfe, // JR $
    ];
    rom[..program.len()].copy_from_slice(&program);
    rom[0x7ff0..0x7ff8].copy_from_slice(b"TMR SEGA");
    rom[0x7fff] = (region << 4) | 0x0c;
    rom
}

/// A 128 KB Mega Drive cart whose 68000 program turns the display on in 320-wide mode,
/// paints palette entry 0 (the backdrop) white and loops.
fn white_screen_md_rom() -> Vec<u8> {
    let mut rom = vec![0u8; 0x20000];
    rom[0..4].copy_from_slice(&0x00ff_fe00u32.to_be_bytes()); // stack
    rom[4..8].copy_from_slice(&0x0000_0200u32.to_be_bytes()); // entry
    rom[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    rom[0x1f0] = b'U';
    let program: [u16; 21] = [
        0x33fc, 0x8144, 0x00c0, 0x0004, // VDP register 1 = 0x44: display on, MD mode
        0x33fc, 0x8c81, 0x00c0, 0x0004, // register 12 = 0x81: 320 wide
        0x23fc, 0xc000, 0x0000, 0x00c0, 0x0004, // VDP address: CRAM 0
        0x33fc, 0x0eee, 0x00c0, 0x0000, // CRAM 0 = white
        0x60fe, // BRA.S $
        0, 0, 0,
    ];
    for (i, w) in program.iter().enumerate() {
        rom[0x200 + 2 * i..0x202 + 2 * i].copy_from_slice(&w.to_be_bytes());
    }
    rom
}

fn runs_white(platform: Platform, name: &str, region: u8, inside: (u32, u32), outside: (u32, u32)) {
    let rom = match platform {
        Platform::Md => white_screen_md_rom(),
        _ => white_screen_rom(region),
    };
    runs(platform, name, &rom, inside, outside);
}

fn runs(platform: Platform, name: &str, cart: &[u8], inside: (u32, u32), outside: (u32, u32)) {
    let path = dylib();
    if !path.exists() {
        eprintln!("no picodrive dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let rom = d.path().join(name);
    std::fs::write(&rom, cart).unwrap();
    let mut core = slot::core::open_core_for_palette(
        d.path(),
        Core::Picodrive,
        platform,
        "auto",
        false,
        None,
        &[path],
    );
    core.load(&rom).expect("PicoDrive refused the cart");
    for _ in 0..10 {
        core.run_frame(slot_retro::ButtonMask(0));
    }
    let (w, h) = core.canvas();
    assert_eq!((w, h), platform.canvas());
    let video = core.video_xrgb8888();
    let px = |(x, y): (u32, u32)| {
        let i = ((y * w + x) * 4) as usize;
        [video[i], video[i + 1], video[i + 2]]
    };
    assert!(
        px(inside).iter().all(|c| *c > 0xc0),
        "{name}: the picture is not white: {:?}",
        px(inside)
    );
    assert_eq!(
        px(outside),
        [0, 0, 0],
        "{name}: the canvas round the picture is not black"
    );
    let state = core.serialize().expect("no save state");
    core.run_frame(slot_retro::ButtonMask(0));
    core.unserialize(&state)
        .expect("its own save state was refused");
}

#[test]
fn a_game_gear_game_draws_its_160x144_screen_centred() {
    // 160x144 in the middle of the 240x160 canvas: x 40..200, y 8..152.
    runs_white(Platform::Gg, "white.gg", 6, (120, 80), (20, 80));
}

#[test]
fn a_master_system_game_draws_its_192_lines_centred() {
    // 256x192 in the middle of the 256x240 canvas: y 24..216.
    runs_white(Platform::Sms, "white.sms", 4, (128, 120), (128, 10));
}

#[test]
fn a_mega_drive_game_draws_its_224_lines_centred() {
    // 320x224 in the middle of the 320x240 canvas: y 8..232.
    runs_white(Platform::Md, "white.md", 0, (160, 120), (160, 2));
}

#[test]
fn a_sega_cd_game_without_its_bios_is_refused_not_run_black() {
    let path = dylib();
    if !path.exists() {
        eprintln!("no picodrive dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let mut iso = vec![0u8; 2048 * 32];
    iso[..16].copy_from_slice(b"SEGADISCSYSTEM  ");
    iso[0x100..0x110].copy_from_slice(b"SEGA MEGA DRIVE ");
    iso[0x1f0] = b'U';
    let rom = d.path().join("game.iso");
    std::fs::write(&rom, iso).unwrap();
    let mut core = slot::core::open_core_for_palette(
        d.path(),
        Core::Picodrive,
        Platform::Scd,
        "auto",
        false,
        None,
        &[path],
    );
    assert!(
        core.load(&rom).is_err(),
        "PicoDrive started a CD with no BIOS"
    );
}
