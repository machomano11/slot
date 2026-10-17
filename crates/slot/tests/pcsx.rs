mod common;

use slot_store::{Core, Platform};

/// A PS-X EXE that spins in place at 0x80010000, run on PCSX ReARMed's built-in BIOS.
fn spinning_exe() -> Vec<u8> {
    let mut exe = vec![0u8; 0x800 + 0x800];
    exe[..8].copy_from_slice(b"PS-X EXE");
    exe[0x10..0x14].copy_from_slice(&0x8001_0000u32.to_le_bytes()); // pc
    exe[0x18..0x1c].copy_from_slice(&0x8001_0000u32.to_le_bytes()); // load address
    exe[0x1c..0x20].copy_from_slice(&0x800u32.to_le_bytes()); // size
    exe[0x30..0x34].copy_from_slice(&0x801f_fff0u32.to_le_bytes()); // stack
    let jump_to_self = 0x0800_0000u32 | ((0x8001_0000u32 >> 2) & 0x03ff_ffff);
    exe[0x800..0x804].copy_from_slice(&jump_to_self.to_le_bytes());
    exe
}

#[test]
fn a_playstation_program_runs_on_the_built_in_bios_and_saves_its_state() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../vendor")
        .join(slot::core::dylib_name(Core::PcsxRearmed));
    if !path.exists() {
        eprintln!("no pcsx_rearmed dylib on this host, skipping");
        return;
    }
    let _g = common::core_lock();
    let d = tempfile::tempdir().unwrap();
    let file = d.path().join("spin.exe");
    std::fs::write(&file, spinning_exe()).unwrap();
    let mut core = slot::core::open_core_for_palette(
        d.path(),
        Core::PcsxRearmed,
        Platform::Ps,
        "auto",
        false,
        None,
        &[path],
    );
    core.load(&file).expect("PCSX ReARMed refused the program");
    for _ in 0..60 {
        core.run_frame(slot_retro::ButtonMask(0));
    }
    assert_eq!(core.canvas(), Platform::Ps.canvas());
    let (w, h) = core.frame_size();
    assert!(
        (256..=640).contains(&w) && (224..=480).contains(&h),
        "{w}x{h}"
    );
    // The core's disc tray, as Slot reads it: the one image loaded, in it.
    assert_eq!(core.discs(), Some((0, 1)));
    let state = core.serialize().expect("no save state");
    core.run_frame(slot_retro::ButtonMask(0));
    core.unserialize(&state)
        .expect("its own save state was refused");
}
