use std::path::Path;

use slot_gfx::{Sizing, SRC_H, SRC_W, WHOLE_TEXTURE};
use slot_store::{ini, Platform, Scale, Stretch};

pub const VIDEO_MODE_FILE: &str = if cfg!(feature = "brick") {
    "Config/brick_video_mode_v3.ini"
} else {
    "Config/video_mode.txt"
};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum VideoMode {
    #[default]
    Actual,
    Stretch,
}

impl VideoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VideoMode::Actual => "actual",
            VideoMode::Stretch => "stretch",
        }
    }

    pub fn parse(s: &str) -> Option<VideoMode> {
        match s.trim().to_ascii_lowercase().as_str() {
            "actual" => Some(VideoMode::Actual),
            "stretch" => Some(VideoMode::Stretch),
            _ => None,
        }
    }
}

pub fn video_mode_for(root: &Path, stem: &str) -> VideoMode {
    video_mode_for_platform(root, stem, Platform::Gba)
}

pub fn default_for_platform(platform: Platform) -> VideoMode {
    match platform {
        // A PlayStation's frames are 256 to 640 wide, all shown 4:3 on a TV: filling the
        // 4:3 screen is the true shape.
        Platform::Ps => VideoMode::Stretch,
        _ => VideoMode::Actual,
    }
}

pub fn video_mode_for_platform(root: &Path, stem: &str, platform: Platform) -> VideoMode {
    ini::value(root, VIDEO_MODE_FILE, stem)
        .as_deref()
        .and_then(VideoMode::parse)
        .unwrap_or_else(|| default_for_platform(platform))
}

pub fn write_video_mode(root: &Path, stem: &str, mode: VideoMode) -> std::io::Result<()> {
    ini::write(root, VIDEO_MODE_FILE, stem, mode.as_str())
}

pub fn source_rect(platform: Platform, mode: VideoMode) -> [f32; 4] {
    picture_source_rect(platform.picture(), platform.canvas(), mode)
}

/// The source rect for a picture of `(w, h)` centred on a canvas of `(src_w, src_h)`.
pub fn picture_source_rect(
    (w, h): (u32, u32),
    (src_w, src_h): (u32, u32),
    mode: VideoMode,
) -> [f32; 4] {
    // In canvas units: a NES frame arrives on a 256x240 canvas, the Game Boys' on 240x160.
    #[cfg(feature = "brick")]
    if mode == VideoMode::Actual {
        // Expand the source rectangle to 4:3; the shader paints the region
        // outside the active picture black. Stretch mode below has no bars.
        let aspect = slot_gfx::OUT_W as f32 / slot_gfx::OUT_H as f32;
        let (vw, vh) = if w as f32 / h as f32 > aspect {
            (w as f32, w as f32 / aspect)
        } else {
            (h as f32 * aspect, h as f32)
        };
        return [
            (src_w as f32 - vw) / (2.0 * src_w as f32),
            (src_h as f32 - vh) / (2.0 * src_h as f32),
            vw / src_w as f32,
            vh / src_h as f32,
        ];
    }
    if mode == VideoMode::Actual || (w, h) == (src_w, src_h) {
        return WHOLE_TEXTURE;
    }
    let x = src_w.saturating_sub(w) / 2;
    let y = src_h.saturating_sub(h) / 2;
    [
        x as f32 / src_w as f32,
        y as f32 / src_h as f32,
        w as f32 / src_w as f32,
        h as f32 / src_h as f32,
    ]
}

#[cfg(all(test, feature = "brick"))]
mod brick_tests {
    use super::*;
    #[test]
    fn default_fills_screen_without_cropping_game_pixels() {
        assert_eq!(VideoMode::default(), VideoMode::Actual);
        assert_eq!((slot_gfx::OUT_W, slot_gfx::OUT_H), (640, 480));
        assert_eq!((SRC_W, SRC_H), (240, 160));
        for platform in [Platform::Gb, Platform::Gbc, Platform::Gba] {
            let [x, y, w, h] = source_rect(platform, VideoMode::Stretch);
            let (pw, ph) = platform.picture();
            assert!((w * SRC_W as f32 - pw as f32).abs() < 0.001);
            // Upstream 1.7 stretches a Game Boy picture sideways only, so its
            // 144 lines sit in the full source height with the band above dark.
            assert!(h * SRC_H as f32 >= ph as f32 - 0.001);
            assert!((2.0 * x + w - 1.0).abs() < 0.001);
            assert!(y >= 0.0 && y + h <= 1.0 + 0.001);
        }
    }
    #[test]
    fn defaults_are_platform_specific_and_saved_choices_work() {
        let root = tempfile::tempdir().unwrap();
        for platform in [Platform::Gb, Platform::Gbc] {
            assert_eq!(
                video_mode_for_platform(root.path(), "unsaved", platform),
                VideoMode::Actual
            );
            let [_, _, w, h] = source_rect(platform, default_for_platform(platform));
            let (pw, ph) = platform.picture();
            let ratio =
                (4.0 / 3.0) * (pw as f32 / (w * SRC_W as f32)) / (ph as f32 / (h * SRC_H as f32));
            assert!((ratio - 10.0 / 9.0).abs() < 0.001);
        }
        assert_eq!(
            video_mode_for_platform(root.path(), "unsaved", Platform::Gba),
            VideoMode::Actual
        );
        assert_eq!(
            source_rect(Platform::Gba, default_for_platform(Platform::Gba)),
            [0.0, -0.0625, 1.0, 1.125]
        );
        write_video_mode(root.path(), "choice", VideoMode::Actual).unwrap();
        assert_eq!(
            video_mode_for_platform(root.path(), "choice", Platform::Gba),
            VideoMode::Actual
        );
    }

    #[test]
    fn a_nes_picture_sits_in_its_own_canvas() {
        let (cw, ch) = Platform::Nes.canvas();
        let [x, y, w, h] = source_rect(Platform::Nes, VideoMode::Stretch);
        assert_eq!(
            (w * cw as f32, h * ch as f32),
            (256.0, 224.0),
            "the overscan-cropped picture"
        );
        assert_eq!((x, y * ch as f32), (0.0, 8.0), "centred, 8 lines down");
        let [_, _, w, h] = source_rect(Platform::Nes, VideoMode::Actual);
        assert!(
            (w * cw as f32 / (h * ch as f32) - 4.0 / 3.0).abs() < 0.001,
            "padded to 4:3"
        );
        assert!((h * ch as f32 - 224.0).abs() < 0.001, "no line cropped");
    }

    #[test]
    fn actual_mode_preserves_original_aspect() {
        for platform in [Platform::Gb, Platform::Gbc, Platform::Gba] {
            let [_, _, w, h] = source_rect(platform, VideoMode::Actual);
            assert!((w * SRC_W as f32 / (h * SRC_H as f32) - 4.0 / 3.0).abs() < 0.001);
        }
    }
}

pub fn picture_for(
    platform: Platform,
    mode: VideoMode,
    scale: Scale,
    stretch: Stretch,
) -> (Sizing, [f32; 4]) {
    let (w, h) = platform.picture();
    let x = SRC_W.saturating_sub(w) as f32 / 2.0 / SRC_W as f32;
    let y = SRC_H.saturating_sub(h) as f32 / 2.0 / SRC_H as f32;
    let (fw, fh) = (w as f32 / SRC_W as f32, h as f32 / SRC_H as f32);
    let crop = [x, y, fw, fh];
    let window = [x, 0.0, fw, 1.0];
    if platform.hardware() == Platform::Gba {
        let sizing = match scale {
            Scale::Integer => Sizing::Integer,
            Scale::FullScreen => Sizing::Panel,
            Scale::SpFrame | Scale::Fit => Sizing::Band,
        };
        return (sizing, WHOLE_TEXTURE);
    }
    match (mode, scale, stretch) {
        (VideoMode::Actual, Scale::Integer, _) => (Sizing::Integer, crop),
        (VideoMode::Actual, Scale::Fit, _) => (Sizing::Fit, crop),
        (VideoMode::Actual, _, _) => (Sizing::Band, WHOLE_TEXTURE),
        (VideoMode::Stretch, _, Stretch::Classic) => (Sizing::Band, window),
        (VideoMode::Stretch, _, Stretch::FullScreen) => (Sizing::Panel, crop),
    }
}
