mod common;

use slot_store::{Core, Platform};

/// Opens a Beetle core on a blank cart of the right size, runs it and checks its frame sits
/// centred on the platform's canvas, with black round it, and that a save state comes back.
fn runs_centred(
    core: Core,
    platform: Platform,
    rom: &[u8],
    name: &str,
    outside: (u32, u32),
) -> (u32, u32) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(core));
    if !path.exists() {
        eprintln!("no {} dylib on this host, skipping", core.as_str());
        return platform.picture();
    }
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join(name);
    std::fs::write(&file, rom).unwrap();
    let mut opened =
        slot::core::open_core_for_palette(d.path(), core, platform, "auto", false, None, &[path]);
    if matches!(platform, Platform::Ws | Platform::Wsc) {
        opened.set_option("wswan_rotate_display", slot::core::wonderswan_held(&file));
    }
    opened.load(&file).expect("the core refused the cart");
    for _ in 0..30 {
        opened.run_frame(slot_retro::ButtonMask(0));
    }
    let (w, h) = opened.canvas();
    assert_eq!((w, h), platform.canvas());
    let video = opened.video_xrgb8888();
    assert_eq!(video.len(), (w * h * 4) as usize);
    let (x, y) = outside;
    let i = ((y * w + x) * 4) as usize;
    assert_eq!(
        [video[i], video[i + 1], video[i + 2]],
        [0, 0, 0],
        "{name}: the canvas round the picture is not black"
    );
    let j = (((h / 2) * w + w / 2) * 4) as usize;
    assert!(
        video[j..j + 3].iter().any(|c| *c > 0x40),
        "{name}: nothing was drawn in the middle"
    );
    let state = opened.serialize().expect("no save state");
    opened.run_frame(slot_retro::ButtonMask(0));
    opened
        .unserialize(&state)
        .expect("its own save state was refused");
    opened.frame_size()
}

/// A 2 Mbit Neo Geo Pocket cart: SNK's licence string, a colour-mode byte and a title.
fn ngp_rom(colour: bool) -> Vec<u8> {
    let mut rom = vec![0u8; 0x40000];
    rom[..28].copy_from_slice(b"COPYRIGHT BY SNK CORPORATION");
    rom[0x1c..0x20].copy_from_slice(&0x0020_0040u32.to_le_bytes()); // entry
    rom[0x23] = if colour { 0x10 } else { 0x00 };
    rom[0x24..0x30].copy_from_slice(b"SLOT TEST   ");
    rom[0x40..0x42].copy_from_slice(&[0x68, 0xfe]); // JR $
    rom
}

#[test]
fn a_neo_geo_pocket_game_runs_centred_on_the_gba_canvas() {
    // 160x152 in the middle of the 240x160 canvas: x 40..200.
    let size = runs_centred(
        Core::MednafenNgp,
        Platform::Ngp,
        &ngp_rom(false),
        "test.ngp",
        (20, 80),
    );
    assert!(size == (160, 152) || size == (240, 160), "{size:?}");
    runs_centred(
        Core::MednafenNgp,
        Platform::Ngpc,
        &ngp_rom(true),
        "test.ngc",
        (220, 80),
    );
}

/// A 1 Mbit WonderSwan cart: a footer whose jump lands on a loop, and its flags byte.
fn ws_rom(colour: bool, upright: bool) -> Vec<u8> {
    let mut rom = vec![0xffu8; 0x20000];
    let n = rom.len();
    rom[n - 0x100..n - 0xfe].copy_from_slice(&[0xeb, 0xfe]); // JMP $ at f000:ff00
    rom[n - 16..n - 11].copy_from_slice(&[0xea, 0x00, 0xff, 0x00, 0xf0]); // JMP F000:FF00
    rom[n - 9] = u8::from(colour);
    rom[n - 6] = 0x01; // 1 Mbit
    rom[n - 5] = 0x00; // no save
    rom[n - 4] = u8::from(upright) | 0x04;
    rom
}

#[test]
fn a_wonderswan_game_is_shown_the_way_its_header_says_to_hold_it() {
    if !std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(Core::MednafenWswan))
        .exists()
    {
        eprintln!("no mednafen_wswan dylib on this host, skipping");
        return;
    }
    // 224x144 flat in the middle of the 300x225 canvas: x 38..262, y 40..184.
    let flat = runs_centred(
        Core::MednafenWswan,
        Platform::Ws,
        &ws_rom(false, false),
        "flat.ws",
        (20, 112),
    );
    assert_eq!(flat, (224, 144));
    // Upright, 144x224: x 78..222.
    let upright = runs_centred(
        Core::MednafenWswan,
        Platform::Wsc,
        &ws_rom(true, true),
        "upright.wsc",
        (60, 112),
    );
    assert_eq!(upright, (144, 224));
}
