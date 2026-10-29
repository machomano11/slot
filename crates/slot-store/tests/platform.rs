use slot_store::Platform;

#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(
        names,
        vec![
            "GBA", "GBAV", "GBC", "GB", "NES", "SNES", "GG", "SMS", "MD", "NGP", "NGPC", "WS",
            "WSC", "SEGACD", "32X", "PS"
        ]
    );
}

#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(
        Platform::ALL,
        [
            Platform::Gba,
            Platform::Gbav,
            Platform::Gbc,
            Platform::Gb,
            Platform::Nes,
            Platform::Snes,
            Platform::Gg,
            Platform::Sms,
            Platform::Md,
            Platform::Ngp,
            Platform::Ngpc,
            Platform::Ws,
            Platform::Wsc,
            Platform::Scd,
            Platform::S32x,
            Platform::Ps
        ]
    );
    for (i, p) in Platform::ALL.iter().enumerate() {
        assert!(
            !Platform::ALL[..i].contains(p),
            "{p:?} is named twice in the ring, so two shelves would hold one platform"
        );
    }
}

#[test]
fn the_sega_8_bits_take_their_own_extensions() {
    assert!(Platform::Gg.accepts(std::path::Path::new("Sonic Chaos.gg")));
    assert!(Platform::Gg.accepts(std::path::Path::new("Shinobi.GG")));
    assert!(!Platform::Gg.accepts(std::path::Path::new("Alex Kidd.sms")));
    assert!(Platform::Sms.accepts(std::path::Path::new("Alex Kidd.sms")));
    assert!(!Platform::Sms.accepts(std::path::Path::new("Sonic Chaos.gg")));
    assert_eq!(Platform::Gg.picture(), (160, 144));
    assert_eq!(Platform::Sms.picture(), (256, 192));
}

#[test]
fn a_mega_drive_takes_every_common_rom_extension() {
    for name in [
        "Sonic.md",
        "Sonic.gen",
        "Sonic.smd",
        "Sonic.bin",
        "Sonic.MD",
    ] {
        assert!(Platform::Md.accepts(std::path::Path::new(name)), "{name}");
    }
    assert!(!Platform::Md.accepts(std::path::Path::new("Alex Kidd.sms")));
    assert_eq!(Platform::Md.picture(), (320, 224));
    assert_eq!(Platform::Md.canvas(), (320, 240));
}

#[test]
fn each_platform_takes_only_its_own_extensions() {
    assert!(Platform::Gba.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Gb.accepts(std::path::Path::new("Tetris.gbc")));
    assert!(!Platform::Gb.accepts(std::path::Path::new("Metroid Fusion.gba")));
    assert!(Platform::Gba.accepts(std::path::Path::new("Shrek.GBA")));
    assert!(Platform::Gbav.accepts(std::path::Path::new("Shrek.gba")));
    assert!(!Platform::Gbav.accepts(std::path::Path::new("Tetris.gb")));
    assert!(Platform::Nes.accepts(std::path::Path::new("Super Mario Bros. (World).nes")));
    assert!(!Platform::Nes.accepts(std::path::Path::new("Tetris.gb")));
    assert!(!Platform::Gba.accepts(std::path::Path::new("Zelda.NES")));
}

#[test]
fn a_nes_picture_fits_its_canvas_and_the_game_boys_keep_theirs() {
    assert_eq!(Platform::Nes.picture(), (256, 224));
    assert_eq!(Platform::Nes.canvas(), (256, 240));
    for p in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        assert_eq!(p.canvas(), (240, 160), "{p:?}");
    }
}

#[test]
fn both_neo_geo_pockets_take_either_extension() {
    for p in [Platform::Ngp, Platform::Ngpc] {
        for name in ["Metal Slug.ngp", "Metal Slug.ngc", "Metal Slug.NGPC"] {
            assert!(p.accepts(std::path::Path::new(name)), "{p:?} {name}");
        }
        assert_eq!(p.picture(), (160, 152));
        assert_eq!(p.canvas(), (240, 160));
    }
}

#[test]
fn a_wonderswan_canvas_holds_its_screen_either_way_up() {
    for p in [Platform::Ws, Platform::Wsc] {
        assert!(p.accepts(std::path::Path::new("Gunpey.ws")));
        assert!(p.accepts(std::path::Path::new("Judgement Silversword.wsc")));
        assert!(p.picture_follows_frames());
        let (w, h) = p.picture();
        let (cw, ch) = p.canvas();
        assert!(w <= cw && h <= ch && h <= cw && w <= ch);
    }
    assert!(!Platform::Gba.picture_follows_frames());
}

#[test]
fn a_sega_cd_game_is_a_disc_image_and_needs_a_bios() {
    for name in [
        "Sonic CD (USA).cue",
        "Snatcher.chd",
        "Lunar.m3u",
        "Popful Mail.iso",
    ] {
        assert!(Platform::Scd.accepts(std::path::Path::new(name)), "{name}");
    }
    assert!(!Platform::Scd.accepts(std::path::Path::new("Sonic CD (USA) (Track 01).bin")));
    assert!(Platform::Scd.discs() && !Platform::Md.discs());
    let d = tempfile::tempdir().unwrap();
    let missing = Platform::Scd
        .missing_bios(d.path())
        .expect("no BIOS, and none said");
    assert!(missing.contains("bios_CD_U.bin"), "{missing}");
    std::fs::write(d.path().join("bios_CD_E.bin"), b"").unwrap();
    assert_eq!(Platform::Scd.missing_bios(d.path()), None);
    assert_eq!(Platform::Md.missing_bios(&d.path().join("nowhere")), None);
}

#[test]
fn a_32x_game_is_its_own_shelf_and_shown_like_a_mega_drive_one() {
    assert!(Platform::S32x.accepts(std::path::Path::new("Knuckles' Chaotix.32x")));
    assert!(!Platform::S32x.accepts(std::path::Path::new("Sonic.md")));
    assert_eq!(Platform::S32x.picture(), Platform::Md.picture());
    assert_eq!(Platform::S32x.canvas(), Platform::Md.canvas());
}

#[test]
fn a_playstation_game_is_a_disc_with_its_own_triggers() {
    for name in [
        "Ridge Racer.cue",
        "Final Fantasy VII.m3u",
        "Crash.chd",
        "Spyro.pbp",
    ] {
        assert!(Platform::Ps.accepts(std::path::Path::new(name)), "{name}");
    }
    assert!(!Platform::Ps.accepts(std::path::Path::new("Track 01.bin")));
    assert!(Platform::Ps.discs());
    assert!(Platform::Ps.uses_triggers());
    assert!(Platform::Ps.picture_follows_frames());
    assert!(Platform::Ps.bios().is_empty());
    assert!(!Platform::Md.uses_triggers());
}

#[test]
fn gba_video_runs_on_gba_hardware() {
    assert_eq!(Platform::Gbav.hardware(), Platform::Gba);
    assert_eq!(Platform::Gbav.picture(), Platform::Gba.picture());
    for p in [Platform::Gba, Platform::Gb, Platform::Gbc] {
        assert_eq!(p.hardware(), p);
    }
}
