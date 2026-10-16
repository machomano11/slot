use slot_store::Platform;

#[test]
fn every_platform_has_a_directory() {
    let names: Vec<&str> = Platform::ALL.iter().map(|p| p.dir_name()).collect();
    assert_eq!(
        names,
        vec!["GBA", "GB", "GBC", "NES", "SNES", "GG", "SMS", "MD"]
    );
}

#[test]
fn every_platform_is_a_shelf_of_its_own() {
    assert_eq!(
        Platform::ALL,
        [
            Platform::Gba,
            Platform::Gb,
            Platform::Gbc,
            Platform::Nes,
            Platform::Snes,
            Platform::Gg,
            Platform::Sms,
            Platform::Md
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
