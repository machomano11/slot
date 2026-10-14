//! Builds the vendored rcheevos (MIT, see rcheevos/LICENSE) and slot's shim over it.

fn main() {
    let sources = [
        "rcheevos/src/rc_client.c",
        "rcheevos/src/rc_compat.c",
        "rcheevos/src/rc_libretro.c",
        "rcheevos/src/rc_util.c",
        "rcheevos/src/rc_version.c",
        "rcheevos/src/rapi/rc_api_common.c",
        "rcheevos/src/rapi/rc_api_info.c",
        "rcheevos/src/rapi/rc_api_runtime.c",
        "rcheevos/src/rapi/rc_api_user.c",
        "rcheevos/src/rcheevos/alloc.c",
        "rcheevos/src/rcheevos/condition.c",
        "rcheevos/src/rcheevos/condset.c",
        "rcheevos/src/rcheevos/consoleinfo.c",
        "rcheevos/src/rcheevos/format.c",
        "rcheevos/src/rcheevos/lboard.c",
        "rcheevos/src/rcheevos/memref.c",
        "rcheevos/src/rcheevos/operand.c",
        "rcheevos/src/rcheevos/richpresence.c",
        "rcheevos/src/rcheevos/runtime.c",
        "rcheevos/src/rcheevos/runtime_progress.c",
        "rcheevos/src/rcheevos/trigger.c",
        "rcheevos/src/rcheevos/value.c",
        "rcheevos/src/rhash/hash.c",
        "rcheevos/src/rhash/hash_rom.c",
        "rcheevos/src/rhash/md5.c",
        "csrc/slot_cheevos.c",
    ];
    let mut build = cc::Build::new();
    build
        .include("rcheevos/include")
        .include("rcheevos/src")
        .include("libretro")
        .define("RC_STATIC", None)
        .define("RC_CLIENT_SUPPORTS_HASH", None)
        .define("RC_HASH_NO_DISC", None)
        .define("RC_HASH_NO_ENCRYPTED", None)
        .define("RC_HASH_NO_ZIP", None)
        .flag_if_supported("-std=gnu99")
        .warnings(false);
    for s in sources {
        build.file(s);
        println!("cargo:rerun-if-changed={s}");
    }
    println!("cargo:rerun-if-changed=rcheevos/include");
    build.compile("rcheevos");
}
