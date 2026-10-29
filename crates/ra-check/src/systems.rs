//! The systems ra-check knows: how to spot their games and which RetroAchievements console
//! they hash as.

pub struct System {
    /// Slot's folder name, used in the report.
    pub name: &'static str,
    /// rcheevos' console id (rc_consoles.h), which is also the web API's system id.
    pub console: u32,
    /// Folder names that mean this system, normalised by [`folder_key`].
    folders: &'static [&'static str],
    /// File extensions that hold one of its games, lowercase.
    pub extensions: &'static [&'static str],
    /// Whether an extension alone is enough to tell the system ("bin" or "chd" are not).
    pub unique: &'static [&'static str],
    /// Disc games: never read out of a zip, and "bin" files beside a "cue" are its tracks.
    pub disc: bool,
}

pub const SYSTEMS: &[System] = &[
    System {
        name: "GBA",
        console: 5,
        folders: &["gba", "gameboyadvance"],
        extensions: &["gba"],
        unique: &["gba"],
        disc: false,
    },
    System {
        name: "GB",
        console: 4,
        folders: &["gb", "gameboy"],
        extensions: &["gb"],
        unique: &["gb"],
        disc: false,
    },
    System {
        name: "GBC",
        console: 6,
        folders: &["gbc", "gameboycolor"],
        extensions: &["gbc"],
        unique: &["gbc"],
        disc: false,
    },
    System {
        name: "NES",
        console: 7,
        folders: &[
            "nes",
            "fc",
            "famicom",
            "fds",
            "nintendo",
            "nintendoentertainmentsystem",
        ],
        extensions: &["nes", "fds", "unf", "unif"],
        unique: &["nes", "fds", "unf", "unif"],
        disc: false,
    },
    System {
        name: "SNES",
        console: 3,
        folders: &["snes", "sfc", "superfamicom", "supernintendo", "supernes"],
        extensions: &["sfc", "smc", "swc", "fig", "bs"],
        unique: &["sfc", "smc", "swc", "fig"],
        disc: false,
    },
    System {
        name: "MD",
        console: 1,
        folders: &[
            "md",
            "megadrive",
            "genesis",
            "gen",
            "segagenesis",
            "segamegadrive",
        ],
        extensions: &["md", "gen", "smd", "bin"],
        unique: &["md", "gen", "smd"],
        disc: false,
    },
    System {
        name: "GG",
        console: 15,
        folders: &["gg", "gamegear", "segagamegear"],
        extensions: &["gg"],
        unique: &["gg"],
        disc: false,
    },
    System {
        name: "SMS",
        console: 11,
        folders: &[
            "sms",
            "mastersystem",
            "segamastersystem",
            "mark3",
            "markiii",
        ],
        extensions: &["sms"],
        unique: &["sms"],
        disc: false,
    },
    System {
        name: "NGP",
        console: 14,
        folders: &["ngp", "ngpc", "neogeopocket", "neogeopocketcolor"],
        extensions: &["ngp", "ngc", "ngpc"],
        unique: &["ngp", "ngc", "ngpc"],
        disc: false,
    },
    System {
        name: "WS",
        console: 53,
        folders: &["ws", "wsc", "wonderswan", "wonderswancolor"],
        extensions: &["ws", "wsc", "pc2"],
        unique: &["ws", "wsc", "pc2"],
        disc: false,
    },
    System {
        name: "SEGACD",
        console: 9,
        folders: &["segacd", "scd", "megacd", "segamegacd"],
        extensions: &["chd", "cue", "iso"],
        unique: &[],
        disc: true,
    },
    System {
        name: "32X",
        console: 10,
        folders: &["32x", "sega32x"],
        extensions: &["32x", "bin"],
        unique: &["32x"],
        disc: false,
    },
    System {
        name: "PS",
        console: 12,
        folders: &["ps", "ps1", "psx", "playstation", "sonyplaystation"],
        extensions: &["chd", "cue", "iso", "pbp"],
        unique: &["pbp"],
        disc: true,
    },
    System {
        name: "N64",
        console: 2,
        folders: &["n64", "nintendo64"],
        extensions: &["n64", "z64", "v64", "ndd"],
        unique: &["n64", "z64", "v64", "ndd"],
        disc: false,
    },
    System {
        name: "DS",
        console: 18,
        folders: &["ds", "nds", "nintendods"],
        extensions: &["nds"],
        unique: &["nds"],
        disc: false,
    },
];

/// A folder name reduced to lowercase letters and digits: "Game Boy Advance" -> "gameboyadvance".
pub fn folder_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The system a folder of that name holds, if its name says.
pub fn by_folder(name: &str) -> Option<&'static System> {
    let key = folder_key(name);
    SYSTEMS.iter().find(|s| s.folders.contains(&key.as_str()))
}

/// The system an extension alone gives away, for files outside a system's folder.
pub fn by_extension(extension: &str) -> Option<&'static System> {
    SYSTEMS.iter().find(|s| s.unique.contains(&extension))
}

/// The system named on the command line, by folder name ("GBA", "megadrive").
pub fn by_name(name: &str) -> Option<&'static System> {
    by_folder(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_name_systems() {
        assert_eq!(by_folder("GBA").unwrap().console, 5);
        assert_eq!(by_folder("Game Boy Advance").unwrap().console, 5);
        assert_eq!(by_folder("NGPC").unwrap().name, "NGP");
        assert_eq!(by_folder("SEGACD").unwrap().console, 9);
        assert_eq!(by_folder("Mega Drive").unwrap().name, "MD");
        assert!(by_folder("Saves").is_none());
    }

    #[test]
    fn ambiguous_extensions_need_a_folder() {
        assert_eq!(by_extension("sfc").unwrap().name, "SNES");
        assert!(by_extension("bin").is_none());
        assert!(by_extension("chd").is_none());
    }
}
