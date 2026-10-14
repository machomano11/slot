mod common;

use std::path::PathBuf;

use slot_cheevos::{Console, FrameWatch};
use slot_retro::{ButtonMask, LibretroCore, RetroCore};

const RETRO_MEMORY_SYSTEM_RAM: u32 = 2;
const GBA_IWRAM_AND_EWRAM: u32 = 0x8000 + 0x40000;

/// The cores to check: `SLOT_ACHIEVEMENT_CORES` (colon separated) or the
/// vendored ones. A core this machine can't load is skipped.
fn cores() -> Vec<PathBuf> {
    match std::env::var("SLOT_ACHIEVEMENT_CORES") {
        Ok(list) => list.split(':').map(PathBuf::from).collect(),
        Err(_) => ["mgba", "gpsp"]
            .iter()
            .map(|c| {
                common::repo_root().join(format!(
                    "vendor/{c}_libretro.{}",
                    std::env::consts::DLL_EXTENSION
                ))
            })
            .filter(|p| p.exists())
            .collect(),
    }
}

#[test]
fn achievements_see_the_gba_work_ram_of_every_core() {
    let rom = common::repo_root().join("crates/slot/tests/rtc_probe/probe.gba");
    for dylib in cores() {
        let Ok(mut core) = LibretroCore::open(&dylib) else {
            eprintln!("skipping {}: can't load it here", dylib.display());
            continue;
        };
        core.load(&rom).expect("load the probe rom");
        for _ in 0..10 {
            core.run_frame(ButtonMask::default());
        }
        let memory = core.memory_access().expect("a loaded core exposes memory");
        let (watch, regions) = unsafe {
            FrameWatch::attach(
                memory.map,
                memory.data,
                memory.size,
                Console::GameBoyAdvance,
            )
        };
        let name = dylib.display();
        assert!(regions > 0, "{name}: no memory regions");
        assert!(
            watch.mapped_size() >= GBA_IWRAM_AND_EWRAM,
            "{name}: only {:#x} bytes mapped",
            watch.mapped_size()
        );
        let iwram = watch.host_address(0).expect("IWRAM mapped");
        let ewram = watch.host_address(0x8000).expect("EWRAM mapped");
        assert!(watch.host_address(GBA_IWRAM_AND_EWRAM - 1).is_some());
        assert_ne!(iwram, ewram);
        let system_ram = unsafe { (memory.data)(RETRO_MEMORY_SYSTEM_RAM) } as *mut u8;
        if !system_ram.is_null() && unsafe { (memory.size)(RETRO_MEMORY_SYSTEM_RAM) } == 0x40000 {
            assert_eq!(
                ewram, system_ram,
                "{name}: EWRAM is not the core's system RAM"
            );
        }
        eprintln!(
            "{name}: {regions} regions, {:#x} bytes, memory map {}",
            watch.mapped_size(),
            if memory.map.is_null() {
                "absent"
            } else {
                "present"
            }
        );
        drop(watch);
    }
}
