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
    Md,
    Ngp,
    Ngpc,
}

impl Platform {
    pub const ALL: [Platform; 10] = [
        Platform::Gba,
        Platform::Gb,
        Platform::Gbc,
        Platform::Nes,
        Platform::Snes,
        Platform::Gg,
        Platform::Sms,
        Platform::Md,
        Platform::Ngp,
        Platform::Ngpc,
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
            Platform::Md => "Sega Mega Drive",
            Platform::Ngp => "Neo Geo Pocket",
            Platform::Ngpc => "Neo Geo Pocket Color",
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
            Platform::Md => "MD",
            Platform::Ngp => "NGP",
            Platform::Ngpc => "NGPC",
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
            Platform::Md => &["md", "gen", "smd", "bin"],
            Platform::Ngp | Platform::Ngpc => &["ngp", "ngc", "ngpc"],
        }
    }

    pub fn picture(self) -> (u32, u32) {
        match self {
            Platform::Gba => (240, 160),
            Platform::Gb | Platform::Gbc | Platform::Gg => (160, 144),
            Platform::Sms => (256, 192),
            // H40 mode; the H32 games' 256 columns sit in its middle.
            Platform::Md => (320, 224),
            Platform::Ngp | Platform::Ngpc => (160, 152),
            // With FCEUmm's and snes9x's 8-line overscan crop top and bottom.
            Platform::Nes | Platform::Snes => (256, 224),
        }
    }

    /// The frame canvas a core draws into, its picture centred: GBA-sized for the Game
    /// Boys, the Game Gear and the Neo Geo Pockets, the full 256x240 (overscan included) for the NES and SNES,
    /// and the Master System, whose 192 lines (224 in a few games) sit in its middle. A SNES
    /// hi-res frame (512 wide, or 448 lines interlaced) is halved into it as it arrives. The
    /// Mega Drive's is 320x240, room for a PAL game's 240 lines.
    pub fn canvas(self) -> (u32, u32) {
        match self {
            Platform::Gba
            | Platform::Gb
            | Platform::Gbc
            | Platform::Gg
            | Platform::Ngp
            | Platform::Ngpc => (240, 160),
            Platform::Nes | Platform::Snes | Platform::Sms => (256, 240),
            Platform::Md => (320, 240),
        }
    }

    /// The picture shaders a console offers: the LCD ones for a handheld's screen, at most a
    /// pixel grid for a TV's.
    pub fn shaders(self) -> &'static [crate::Shader] {
        match self {
            Platform::Gba => &crate::Shader::GBA,
            Platform::Gb | Platform::Gbc | Platform::Gg | Platform::Ngp | Platform::Ngpc => {
                &crate::Shader::GB
            }
            Platform::Nes | Platform::Snes | Platform::Sms | Platform::Md => &crate::Shader::NES,
        }
    }

    pub fn default_shader(self) -> crate::Shader {
        match self {
            Platform::Gba | Platform::Gg | Platform::Ngp | Platform::Ngpc => crate::Shader::Grid,
            Platform::Gb | Platform::Gbc => crate::Shader::Simpletex,
            Platform::Nes | Platform::Snes | Platform::Sms | Platform::Md => crate::Shader::Off,
        }
    }

    /// The consoles whose shader has its own field in SlotState (and line in the state file
    /// since before the others came).
    pub fn has_own_shader_field(self) -> bool {
        matches!(
            self,
            Platform::Gba | Platform::Gb | Platform::Gbc | Platform::Nes | Platform::Snes
        )
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
