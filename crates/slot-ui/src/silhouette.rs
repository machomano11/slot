use std::sync::OnceLock;

use crate::bitmaps;
use crate::cart::{
    plain_cart_box, CART_H, CART_W, GB_CART_H, GB_CART_W, NES_CART_H, NES_CART_W, SNES_CART_H,
    SNES_CART_W,
};

const NES_CART_SVG: &str = include_str!("../assets/nes_cart.svg");
const NES_DETAIL_SVG: &str = include_str!("../assets/nes_cart_detail.svg");
const FAMICOM_CART_SVG: &str = include_str!("../assets/famicom_cart.svg");
const FAMICOM_DETAIL_SVG: &str = include_str!("../assets/famicom_cart_detail.svg");
const SNES_CART_SVG: &str = include_str!("../assets/snes_cart.svg");
const SNES_DETAIL_SVG: &str = include_str!("../assets/snes_cart_detail.svg");
const SNES_PAL_CART_SVG: &str = include_str!("../assets/snes_pal_cart.svg");
const SNES_PAL_DETAIL_SVG: &str = include_str!("../assets/snes_pal_cart_detail.svg");
const GG_CART_SVG: &str = include_str!("../assets/gg_cart.svg");
const GG_DETAIL_SVG: &str = include_str!("../assets/gg_cart_detail.svg");
const SMS_CART_SVG: &str = include_str!("../assets/sms_cart.svg");
const SMS_DETAIL_SVG: &str = include_str!("../assets/sms_cart_detail.svg");
const MD_CART_SVG: &str = include_str!("../assets/md_cart.svg");
const MD_DETAIL_SVG: &str = include_str!("../assets/md_cart_detail.svg");

/// A cart of one fixed shape, drawn from its SVG outline and moulding: the consoles whose
/// carts have no regional or colour variants to choose between.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum PlainCart {
    GameGear,
    MasterSystem,
    MegaDrive,
}

impl PlainCart {
    pub const ALL: [PlainCart; 3] = [
        PlainCart::GameGear,
        PlainCart::MasterSystem,
        PlainCart::MegaDrive,
    ];

    fn svgs(self) -> (&'static str, &'static str) {
        match self {
            PlainCart::GameGear => (GG_CART_SVG, GG_DETAIL_SVG),
            PlainCart::MasterSystem => (SMS_CART_SVG, SMS_DETAIL_SVG),
            PlainCart::MegaDrive => (MD_CART_SVG, MD_DETAIL_SVG),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GbShell {
    Notched,
    Rounded,
}

/// The NES cart, or the small landscape Famicom one.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum NesShell {
    Nes,
    Famicom,
}

/// The US SNES cart, or the rounder one Europe and Japan (Super Famicom) had.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SnesShell {
    Us,
    Pal,
}

pub fn silhouette(w: u32, h: u32) -> Vec<u8> {
    alpha(bitmaps::gba_cart(), w, h)
}

pub fn gb_silhouette(shell: GbShell, w: u32, h: u32) -> Vec<u8> {
    let src = match shell {
        GbShell::Notched => bitmaps::gb_cart(),
        GbShell::Rounded => bitmaps::gbc_cart(),
    };
    alpha(src, w, h)
}

fn alpha(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    debug_assert_eq!(rgba.len(), (w * h * 4) as usize);
    rgba.iter().skip(3).step_by(4).copied().collect()
}

pub(crate) fn cart_mask() -> &'static [u8] {
    static MASK: OnceLock<Vec<u8>> = OnceLock::new();
    MASK.get_or_init(|| silhouette(CART_W, CART_H))
}

pub(crate) fn gb_cart_mask(shell: GbShell) -> &'static [u8] {
    static NOTCHED: OnceLock<Vec<u8>> = OnceLock::new();
    static ROUNDED: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        GbShell::Notched => &NOTCHED,
        GbShell::Rounded => &ROUNDED,
    };
    lock.get_or_init(|| gb_silhouette(shell, GB_CART_W, GB_CART_H))
}

pub(crate) fn nes_cart_mask(shell: NesShell) -> &'static [u8] {
    static NES: OnceLock<Vec<u8>> = OnceLock::new();
    static FAMICOM: OnceLock<Vec<u8>> = OnceLock::new();
    let (lock, svg) = match shell {
        NesShell::Nes => (&NES, NES_CART_SVG),
        NesShell::Famicom => (&FAMICOM, FAMICOM_CART_SVG),
    };
    lock.get_or_init(|| {
        rasterise_svg(svg, NES_CART_W, NES_CART_H)
            .unwrap_or_else(|| vec![255; (NES_CART_W * NES_CART_H) as usize])
    })
}

pub(crate) fn nes_cart_depth(shell: NesShell) -> &'static [u8] {
    static NES: OnceLock<Vec<u8>> = OnceLock::new();
    static FAMICOM: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        NesShell::Nes => &NES,
        NesShell::Famicom => &FAMICOM,
    };
    lock.get_or_init(|| {
        depth_map(
            nes_cart_mask(shell),
            NES_CART_W as usize,
            NES_CART_H as usize,
        )
    })
}

/// NES: the full-height grip, the label recess and its arrow; nothing moulded in. Famicom: the
/// rim round the front and the step to the tab; its label recess is cut by the cart face itself.
pub(crate) fn nes_detail_mask(shell: NesShell) -> &'static Detail {
    static NES: OnceLock<Detail> = OnceLock::new();
    static FAMICOM: OnceLock<Detail> = OnceLock::new();
    let (lock, svg) = match shell {
        NesShell::Nes => (&NES, NES_DETAIL_SVG),
        NesShell::Famicom => (&FAMICOM, FAMICOM_DETAIL_SVG),
    };
    lock.get_or_init(|| {
        rasterise_detail(svg, NES_CART_W, NES_CART_H)
            .unwrap_or_else(|| Detail::blank(NES_CART_W, NES_CART_H))
    })
}

pub(crate) fn snes_cart_mask(shell: SnesShell) -> &'static [u8] {
    static US: OnceLock<Vec<u8>> = OnceLock::new();
    static PAL: OnceLock<Vec<u8>> = OnceLock::new();
    let (lock, svg) = match shell {
        SnesShell::Us => (&US, SNES_CART_SVG),
        SnesShell::Pal => (&PAL, SNES_PAL_CART_SVG),
    };
    lock.get_or_init(|| {
        rasterise_svg(svg, SNES_CART_W, SNES_CART_H)
            .unwrap_or_else(|| vec![255; (SNES_CART_W * SNES_CART_H) as usize])
    })
}

pub(crate) fn snes_cart_depth(shell: SnesShell) -> &'static [u8] {
    static US: OnceLock<Vec<u8>> = OnceLock::new();
    static PAL: OnceLock<Vec<u8>> = OnceLock::new();
    let lock = match shell {
        SnesShell::Us => &US,
        SnesShell::Pal => &PAL,
    };
    lock.get_or_init(|| {
        depth_map(
            snes_cart_mask(shell),
            SNES_CART_W as usize,
            SNES_CART_H as usize,
        )
    })
}

/// US: the wing grooves and screws, the label recess and the tab below. PAL: the side grips,
/// seams, screws and the window; its label recess is cut by the cart face itself.
pub(crate) fn snes_detail_mask(shell: SnesShell) -> &'static Detail {
    static US: OnceLock<Detail> = OnceLock::new();
    static PAL: OnceLock<Detail> = OnceLock::new();
    let (lock, svg) = match shell {
        SnesShell::Us => (&US, SNES_DETAIL_SVG),
        SnesShell::Pal => (&PAL, SNES_PAL_DETAIL_SVG),
    };
    lock.get_or_init(|| {
        rasterise_detail(svg, SNES_CART_W, SNES_CART_H)
            .unwrap_or_else(|| Detail::blank(SNES_CART_W, SNES_CART_H))
    })
}

pub(crate) fn plain_cart_mask(cart: PlainCart) -> &'static [u8] {
    static MASKS: [OnceLock<Vec<u8>>; PlainCart::ALL.len()] =
        [const { OnceLock::new() }; PlainCart::ALL.len()];
    MASKS[cart as usize].get_or_init(|| {
        let (w, h) = plain_cart_box(cart);
        rasterise_svg(cart.svgs().0, w, h).unwrap_or_else(|| vec![255; (w * h) as usize])
    })
}

pub(crate) fn plain_cart_depth(cart: PlainCart) -> &'static [u8] {
    static DEPTHS: [OnceLock<Vec<u8>>; PlainCart::ALL.len()] =
        [const { OnceLock::new() }; PlainCart::ALL.len()];
    DEPTHS[cart as usize].get_or_init(|| {
        let (w, h) = plain_cart_box(cart);
        depth_map(plain_cart_mask(cart), w as usize, h as usize)
    })
}

/// The grip ridges and the seam above the connector; the label recess is cut by the cart face.
pub(crate) fn plain_detail_mask(cart: PlainCart) -> &'static Detail {
    static DETAILS: [OnceLock<Detail>; PlainCart::ALL.len()] =
        [const { OnceLock::new() }; PlainCart::ALL.len()];
    DETAILS[cart as usize].get_or_init(|| {
        let (w, h) = plain_cart_box(cart);
        rasterise_detail(cart.svgs().1, w, h).unwrap_or_else(|| Detail::blank(w, h))
    })
}

fn depth_map(mask: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut d: Vec<u8> = mask
        .iter()
        .map(|c| if *c > 127 { 255 } else { 0 })
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let up = if y == 0 { 0 } else { d[i - w] };
            let left = if x == 0 { 0 } else { d[i - 1] };
            d[i] = d[i].min(up.saturating_add(1)).min(left.saturating_add(1));
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            if d[i] == 0 {
                continue;
            }
            let down = if y + 1 == h { 0 } else { d[i + w] };
            let right = if x + 1 == w { 0 } else { d[i + 1] };
            d[i] = d[i]
                .min(down.saturating_add(1))
                .min(right.saturating_add(1));
        }
    }
    d
}

pub(crate) struct Detail {
    pub shadow: Vec<u8>,
    pub highlight: Vec<u8>,
}

impl Detail {
    fn blank(w: u32, h: u32) -> Detail {
        Detail {
            shadow: vec![0; (w * h) as usize],
            highlight: vec![0; (w * h) as usize],
        }
    }
}

fn rasterise_detail(svg: &str, w: u32, h: u32) -> Option<Detail> {
    let px = render(svg, w, h)?;
    let mut shadow = Vec::with_capacity((w * h) as usize);
    let mut highlight = Vec::with_capacity((w * h) as usize);
    for p in px.data().chunks_exact(4) {
        let lit = ((p[0] as u32 * 54 + p[1] as u32 * 183 + p[2] as u32 * 19) / 256) as u8;
        let lit = lit.min(p[3]);
        highlight.push(lit);
        shadow.push(p[3] - lit);
    }
    Some(Detail { shadow, highlight })
}

fn rasterise_svg(svg: &str, w: u32, h: u32) -> Option<Vec<u8>> {
    let px = render(svg, w, h)?;
    Some(px.data().iter().skip(3).step_by(4).copied().collect())
}

fn render(svg: &str, w: u32, h: u32) -> Option<resvg::tiny_skia::Pixmap> {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    let size = tree.size();
    let scale =
        resvg::tiny_skia::Transform::from_scale(w as f32 / size.width(), h as f32 / size.height());
    resvg::render(&tree, scale, &mut pixmap.as_mut());
    Some(pixmap)
}
