use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Platform {
    #[default]
    Gba,
    Gb,
    Gbc,
    Nes,
    Snes,
    Gg,
    Sms,
}

impl Platform {
    pub const ALL: [Platform; 7] = [
        Platform::Gba,
        Platform::Gb,
        Platform::Gbc,
        Platform::Nes,
        Platform::Snes,
        Platform::Gg,
        Platform::Sms,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Platform::Gba => "Game Boy Advance",
            Platform::Gb => "Game Boy",
            Platform::Gbc => "Game Boy Color",
            Platform::Nes => "Nintendo Entertainment System",
            Platform::Snes => "Super Nintendo Entertainment System",
            Platform::Gg => "Sega Game Gear",
            Platform::Sms => "Sega Master System",
        }
    }

    pub fn dir_name(self) -> &'static str {
        match self {
            Platform::Gba => "GBA",
            Platform::Gb => "GB",
            Platform::Gbc => "GBC",
            Platform::Nes => "NES",
            Platform::Snes => "SNES",
            Platform::Gg => "GG",
            Platform::Sms => "SMS",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Platform::Gba => &["gba"],
            Platform::Gb | Platform::Gbc => &["gb", "gbc"],
            Platform::Nes => &["nes"],
            Platform::Snes => &["sfc", "smc"],
            Platform::Gg => &["gg"],
            Platform::Sms => &["sms"],
        }
    }

    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc | Platform::Gg => (160, 144),
            Platform::Sms => (256, 192),
            // With FCEUmm's and snes9x's 8-line overscan crop top and bottom.
            Platform::Nes | Platform::Snes => (256, 224),
        }
    }

    /// The frame canvas a core draws into, its picture centred: GBA-sized for the Game
    /// Boys and the Game Gear, the full 256x240 (overscan included) for the NES and SNES,
    /// and the Master System, whose 192 lines (224 in a few games) sit in its middle. A SNES
    /// hi-res frame (512 wide, or 448 lines interlaced) is halved into it as it arrives.
    pub fn canvas(self) -> (u32, u32) {
        match self {
            Platform::Gba | Platform::Gb | Platform::Gbc | Platform::Gg => (240, 160),
            Platform::Nes | Platform::Snes | Platform::Sms => (256, 240),
        }
    }

    /// Drew on a Game Boy's LCD: has Slot's colour correction, palettes and bezels. The
    /// other consoles drew on a TV, or (the Game Gear) have no such options in their core.
    pub fn game_boy_family(self) -> bool {
        matches!(self, Platform::Gba | Platform::Gb | Platform::Gbc)
    }

    pub fn accepts(self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                self.extensions()
                    .iter()
                    .any(|k| ext.eq_ignore_ascii_case(k))
            })
    }
}
