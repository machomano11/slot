use slot_store::gb::Class;
use slot_store::{Cart, Outline, Platform};

use crate::art;
use crate::bitmaps::{self, Mould};
use crate::board::{BOARD_H, BOARD_W};
use crate::shell::{japanese, shell_for, snes_pal, Finish, Shell};
use crate::silhouette::{
    cart_mask, gb_cart_mask, nes_cart_depth, nes_cart_mask, nes_detail_mask, plain_cart_depth,
    plain_cart_mask, plain_detail_mask, snes_cart_depth, snes_cart_mask, snes_detail_mask, Detail,
    GbShell, NesShell, PlainCart, SnesShell,
};
use crate::text;

pub const CART_W: u32 = 360;
pub const CART_H: u32 = (CART_W * SEATED_H + SEATED_W / 2) / SEATED_W;

pub const GB_CART_W: u32 = 312;

pub const GB_CART_H: u32 = (GB_CART_W * GB_SEATED_H + SEATED_W / 2) / SEATED_W;

/// About a Game Boy pak's size on the shelf, so the carousel keeps its rhythm; narrower,
/// as the NES cart is the taller shape and the row has room for ~340px.
pub const NES_CART_W: u32 = 276;

pub const NES_CART_H: u32 = (NES_CART_W * NES_SEATED_H + SEATED_W / 2) / SEATED_W;

/// A GBA cart's width: both are wide, low carts.
pub const SNES_CART_W: u32 = 360;

pub const SNES_CART_H: u32 = (SNES_CART_W * SNES_SEATED_H + SEATED_W / 2) / SEATED_W;

/// The GBA cart's label box, as drawn in `assets/gba_neutral.png`: x 48..312, y 50..186
/// of 360x212.
pub const fn label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 133 + 500) / 1000,
        (h * 236 + 500) / 1000,
        (w * 867 + 500) / 1000,
        (h * 877 + 500) / 1000,
    )
}

pub const LABEL_WELL: [u8; 3] = [0x8d, 0x8d, 0x90];

pub const LABEL_X: u32 = label_panel(CART_W, CART_H).0;
pub const LABEL_Y: u32 = label_panel(CART_W, CART_H).1;
pub const LABEL_W: u32 = label_panel(CART_W, CART_H).2 - LABEL_X;
pub const LABEL_H: u32 = label_panel(CART_W, CART_H).3 - LABEL_Y;

pub const fn gb_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 147 + 500) / 1000,
        (h * 291 + 500) / 1000,
        (w * 854 + 500) / 1000,
        (h * 868 + 500) / 1000,
    )
}

pub const GB_LABEL_X: u32 = gb_label_panel(GB_CART_W, GB_CART_H).0;
pub const GB_LABEL_Y: u32 = gb_label_panel(GB_CART_W, GB_CART_H).1;
pub const GB_LABEL_W: u32 = gb_label_panel(GB_CART_W, GB_CART_H).2 - GB_LABEL_X;
pub const GB_LABEL_H: u32 = gb_label_panel(GB_CART_W, GB_CART_H).3 - GB_LABEL_Y;

/// The Game Boy Color pak's label box: exactly the flat label well drawn in
/// `assets/gbc_neutral.png`, x 45..267, y 110..308 of 312x337. It is the same 223x199
/// well as the Game Boy's (x 44..266, y 95..293), 1px right and 15px lower.
pub const GBC_LABEL_X: u32 = 45;
pub const GBC_LABEL_Y: u32 = 110;
pub const GBC_LABEL_W: u32 = 223;
pub const GBC_LABEL_H: u32 = 199;

const PAD: u32 = 10;
const MAX_LINES: usize = 3;
const MIN_PX: f32 = 10.0;

/// The portrait recess right of the grip, open to the top edge (nes_cart_detail.svg:
/// 93.2..207.8 of 240, 1..193.8 of 288).
pub const fn nes_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 388 + 500) / 1000,
        (h * 4 + 500) / 1000,
        (w * 866 + 500) / 1000,
        (h * 673 + 500) / 1000,
    )
}

pub const NES_LABEL_X: u32 = nes_label_panel(NES_CART_W, NES_CART_H).0;
pub const NES_LABEL_Y: u32 = nes_label_panel(NES_CART_W, NES_CART_H).1;
pub const NES_LABEL_W: u32 = nes_label_panel(NES_CART_W, NES_CART_H).2 - NES_LABEL_X;
pub const NES_LABEL_H: u32 = nes_label_panel(NES_CART_W, NES_CART_H).3 - NES_LABEL_Y;

/// The landscape recess across the top of the raised middle, open to the top edge
/// (snes_cart_detail.svg: 47..191.3 of 240, 0.5..66.7 of 157).
pub const fn snes_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 196 + 500) / 1000,
        (h * 3 + 500) / 1000,
        (w * 797 + 500) / 1000,
        (h * 425 + 500) / 1000,
    )
}

pub const SNES_LABEL_X: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).0;
pub const SNES_LABEL_Y: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).1;
pub const SNES_LABEL_W: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).2 - SNES_LABEL_X;
pub const SNES_LABEL_H: u32 = snes_label_panel(SNES_CART_W, SNES_CART_H).3 - SNES_LABEL_Y;

/// The PAL / Super Famicom cart's wider, lower recess (snes_pal_cart_detail.svg's face:
/// 29.5..210.8 of 240, 10.5..72.5 of 157).
pub const fn snes_pal_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 123 + 500) / 1000,
        (h * 67 + 500) / 1000,
        (w * 878 + 500) / 1000,
        (h * 462 + 500) / 1000,
    )
}

const SNES_PAL_LABEL: (u32, u32, u32, u32) = {
    let (x0, y0, x1, y1) = snes_pal_label_panel(SNES_CART_W, SNES_CART_H);
    (x0, y0, x1 - x0, y1 - y0)
};

/// The Famicom cart's landscape label (famicom_cart.svg's face: 27.1..212.9 of 240,
/// 160.7..256.9 of 288), shaped as cart_art.rs cuts it from pictures of the cart.
pub const fn famicom_label_panel(w: u32, h: u32) -> (u32, u32, u32, u32) {
    (
        (w * 113 + 500) / 1000,
        (h * 558 + 500) / 1000,
        (w * 887 + 500) / 1000,
        (h * 892 + 500) / 1000,
    )
}

const FAMICOM_LABEL: (u32, u32, u32, u32) = {
    let (x0, y0, x1, y1) = famicom_label_panel(NES_CART_W, NES_CART_H);
    (x0, y0, x1 - x0, y1 - y0)
};

/// A plain cart's size on the shelf and its label recess (as fractions of its outline, x0 y0
/// x1 y1). The Game Gear's small landscape cart is drawn a little narrower than a GBA one,
/// the Master System's tall one about as wide as a NES cart, the Mega Drive's a little wider.
const fn plain_geometry(cart: PlainCart) -> (u32, u32, [u32; 4]) {
    match cart {
        // gg_cart.svg: the label below the grip, x 20..220, y 36..166 of 240x192.
        PlainCart::GameGear => (300, GG_SEATED_H, [83, 188, 917, 865]),
        // sms_cart.svg: the label over the top, x 18..222, y 12..198 of 240x300.
        PlainCart::MasterSystem => (264, SMS_SEATED_H, [75, 40, 925, 660]),
        // md_cart.svg: the label below the grip, x 20..220, y 52..250 of 240x288.
        PlainCart::MegaDrive => (280, MD_SEATED_H, [83, 181, 917, 868]),
        // ngp_cart.svg: the label below the grip, x 22..218, y 44..206 of 240x228.
        PlainCart::NeoGeoPocket => (240, NGP_SEATED_H, [92, 193, 908, 904]),
        // ws_cart.svg: the label above the grip, x 18..222, y 14..160 of 240x200.
        PlainCart::WonderSwan => (270, WS_SEATED_H, [75, 70, 925, 800]),
        // disc_case.svg: the cover behind the front, x 26..232, y 8..232 of 240x240.
        PlainCart::DiscCase => (290, DISC_SEATED_H, [108, 33, 967, 967]),
        // s32x_cart.svg: the Mega Drive cart's label recess, under a taller ridged crown.
        PlainCart::Sega32x => (280, S32X_SEATED_H, [83, 222, 917, 885]),
    }
}

pub const fn plain_cart_box(cart: PlainCart) -> (u32, u32) {
    let (w, seated_h, _) = plain_geometry(cart);
    (w, (w * seated_h + SEATED_W / 2) / SEATED_W)
}

pub const fn plain_label(cart: PlainCart) -> (u32, u32, u32, u32) {
    let (w, h) = plain_cart_box(cart);
    let [x0, y0, x1, y1] = plain_geometry(cart).2;
    let (x0, y0) = ((w * x0 + 500) / 1000, (h * y0 + 500) / 1000);
    let (x1, y1) = ((w * x1 + 500) / 1000, (h * y1 + 500) / 1000);
    (x0, y0, x1 - x0, y1 - y0)
}

const NES_MAX_PX: f32 = (NES_LABEL_H - 2 * PAD) as f32 / MAX_LINES as f32;
const FAMICOM_MAX_PX: f32 = FAMICOM_LABEL.3 as f32 / (MAX_LINES as f32 * 1.36);
const SNES_MAX_PX: f32 = SNES_LABEL_H as f32 / (MAX_LINES as f32 * 1.36);
const SNES_PAL_MAX_PX: f32 = SNES_PAL_LABEL.3 as f32 / (MAX_LINES as f32 * 1.36);

const NES_RIM: u32 = (7 * NES_CART_W + SEATED_W / 2) / SEATED_W;
const SNES_RIM: u32 = (4 * SNES_CART_W + SEATED_W / 2) / SEATED_W;

pub struct CartFace {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
}

struct Spec {
    w: u32,
    h: u32,
    label: (u32, u32, u32, u32),
    mask: &'static [u8],
    mould: Mould,
    paint: &'static [u8],
    /// NES and SNES carts have no bitmap mould: they are drawn from their SVG outline.
    drawn: Option<Drawn>,
}

/// How a NES or SNES cart is drawn (Slot-Brick's procedural shell).
struct Drawn {
    depth: &'static [u8],
    detail: &'static Detail,
    max_px: f32,
    max_h: f32,
    rim: u32,
    board: Board,
}

struct Board {
    x: (f32, f32),
    top: f32,
    pins: (f32, f32),
    contacts_from: f32,
    traces_from: f32,
}

enum Shape {
    Gba,
    Gb(GbShell),
    Nes(NesShell),
    Snes(SnesShell),
    Plain(PlainCart),
}

pub fn gb_shell_of(cart: &Cart) -> Option<GbShell> {
    match cart.platform {
        Platform::Gb | Platform::Gbc => Some(match cart.shell.map(|c| c.outline) {
            Some(Outline::Notched) => GbShell::Notched,
            Some(Outline::Rounded) => GbShell::Rounded,
            _ => match slot_store::gb::cached_class(&cart.rom) {
                Class::ColourOnly => GbShell::Rounded,
                Class::Original | Class::DualMode => GbShell::Notched,
            },
        }),
        _ => None,
    }
}

pub fn label_rect(shell: Option<GbShell>) -> (f32, f32, f32, f32) {
    let s = spec(match shell {
        None => Shape::Gba,
        Some(shell) => Shape::Gb(shell),
    });
    let (x, y, w, h) = s.label;
    (
        x as f32 / s.w as f32,
        y as f32 / s.h as f32,
        w as f32 / s.w as f32,
        h as f32 / s.h as f32,
    )
}

/// A SNES cart's shape: as chosen in the cart menu (notched is the US cart, rounded the
/// PAL / Super Famicom one), else where the ROM's header says it was sold, else the region in
/// its name. Reads the ROM.
pub fn snes_shell_of(cart: &Cart) -> Option<SnesShell> {
    (cart.platform == Platform::Snes).then(|| match cart.shell.map(|c| c.outline) {
        Some(Outline::Notched) => SnesShell::Us,
        Some(Outline::Rounded) => SnesShell::Pal,
        _ => match slot_store::region::snes_pal_or_japan(&cart.rom)
            .unwrap_or_else(|| snes_pal(&cart.stem))
        {
            true => SnesShell::Pal,
            false => SnesShell::Us,
        },
    })
}

/// The cart's label picture at w x h: filling the label, but on the Famicom cart a tall (NES)
/// label is shown whole, as filling would cut most of it away.
pub fn fitted_label(cart: &Cart, w: u32, h: u32) -> Option<Vec<u8>> {
    let path = shown_label(cart)?;
    if nes_famicom_of(cart) {
        art::label_fit(path, w, h)
    } else {
        art::cover(path, w, h)
    }
}

/// The label a cart wears: a NES game keeps one for each of its carts, so swapping the
/// shape swaps the picture; while it has only one, both carts show it.
pub fn shown_label(cart: &Cart) -> Option<&std::path::Path> {
    let (own, famicom) = (cart.label.as_deref(), cart.famicom_label.as_deref());
    if nes_famicom_of(cart) {
        famicom.or(own)
    } else {
        own.or(famicom)
    }
}

/// A NES game on the Famicom cart: as chosen in the cart menu (notched is the NES cart,
/// rounded the Famicom), else a ROM named as a Japanese release or on a Famicom-only board.
pub fn nes_famicom_of(cart: &Cart) -> bool {
    cart.platform == Platform::Nes
        && match cart.shell.map(|c| c.outline) {
            Some(Outline::Notched) => false,
            Some(Outline::Rounded) => true,
            _ => japanese(&cart.stem) || slot_store::region::nes_famicom_board(&cart.rom),
        }
}

/// The one shape a console's carts all had, for a console with no choice of shapes.
pub fn plain_cart_for(platform: Platform) -> Option<PlainCart> {
    match platform {
        Platform::Gg => Some(PlainCart::GameGear),
        Platform::Sms => Some(PlainCart::MasterSystem),
        Platform::Md => Some(PlainCart::MegaDrive),
        Platform::Ngp | Platform::Ngpc => Some(PlainCart::NeoGeoPocket),
        Platform::Ws | Platform::Wsc => Some(PlainCart::WonderSwan),
        Platform::Scd => Some(PlainCart::DiscCase),
        Platform::S32x => Some(PlainCart::Sega32x),
        Platform::Gba | Platform::Gb | Platform::Gbc | Platform::Nes | Platform::Snes => None,
    }
}

fn shape_of(cart: &Cart) -> Shape {
    if let Some(plain) = plain_cart_for(cart.platform) {
        return Shape::Plain(plain);
    }
    match (cart.platform, gb_shell_of(cart)) {
        (Platform::Nes, _) if nes_famicom_of(cart) => Shape::Nes(NesShell::Famicom),
        (Platform::Nes, _) => Shape::Nes(NesShell::Nes),
        (Platform::Snes, _) => Shape::Snes(snes_shell_of(cart).unwrap_or(SnesShell::Us)),
        (_, None) => Shape::Gba,
        (_, Some(shell)) => Shape::Gb(shell),
    }
}

fn spec(shape: Shape) -> Spec {
    match shape {
        Shape::Gba => Spec {
            w: CART_W,
            h: CART_H,
            label: (LABEL_X, LABEL_Y, LABEL_W, LABEL_H),
            mask: cart_mask(),
            mould: Mould::Gba,
            paint: bitmaps::gba_cart(),
            drawn: None,
        },
        Shape::Gb(shell) => Spec {
            w: GB_CART_W,
            h: GB_CART_H,
            label: match shell {
                GbShell::Notched => (GB_LABEL_X, GB_LABEL_Y, GB_LABEL_W, GB_LABEL_H),
                GbShell::Rounded => (GBC_LABEL_X, GBC_LABEL_Y, GBC_LABEL_W, GBC_LABEL_H),
            },
            mask: gb_cart_mask(shell),
            mould: match shell {
                GbShell::Notched => Mould::Notched,
                GbShell::Rounded => Mould::Rounded,
            },
            paint: match shell {
                GbShell::Notched => bitmaps::gb_cart(),
                GbShell::Rounded => bitmaps::gbc_cart(),
            },
            drawn: None,
        },
        Shape::Nes(shell) => Spec {
            w: NES_CART_W,
            h: NES_CART_H,
            label: match shell {
                NesShell::Nes => (NES_LABEL_X, NES_LABEL_Y, NES_LABEL_W, NES_LABEL_H),
                NesShell::Famicom => FAMICOM_LABEL,
            },
            mask: nes_cart_mask(shell),
            mould: Mould::Gba,
            paint: &[],
            drawn: Some(Drawn {
                depth: nes_cart_depth(shell),
                detail: nes_detail_mask(shell),
                max_px: match shell {
                    NesShell::Nes => NES_MAX_PX,
                    NesShell::Famicom => FAMICOM_MAX_PX,
                },
                max_h: match shell {
                    NesShell::Nes => (NES_LABEL_H - 2 * PAD) as f32,
                    NesShell::Famicom => f32::INFINITY,
                },
                rim: NES_RIM,
                board: Board {
                    x: (0.04, 0.96),
                    top: 0.04,
                    pins: (0.06, 0.94),
                    contacts_from: 0.93,
                    traces_from: 0.86,
                },
            }),
        },
        Shape::Snes(shell) => Spec {
            w: SNES_CART_W,
            h: SNES_CART_H,
            label: match shell {
                SnesShell::Us => (SNES_LABEL_X, SNES_LABEL_Y, SNES_LABEL_W, SNES_LABEL_H),
                SnesShell::Pal => SNES_PAL_LABEL,
            },
            mask: snes_cart_mask(shell),
            mould: Mould::Gba,
            paint: &[],
            drawn: Some(Drawn {
                depth: snes_cart_depth(shell),
                detail: snes_detail_mask(shell),
                max_px: match shell {
                    SnesShell::Us => SNES_MAX_PX,
                    SnesShell::Pal => SNES_PAL_MAX_PX,
                },
                max_h: f32::INFINITY,
                rim: SNES_RIM,
                board: Board {
                    x: (0.17, 0.83),
                    top: 0.03,
                    pins: (0.2, 0.8),
                    contacts_from: 0.9,
                    traces_from: 0.8,
                },
            }),
        },
        Shape::Plain(cart) => {
            let (w, h) = plain_cart_box(cart);
            let label = plain_label(cart);
            Spec {
                w,
                h,
                label,
                mask: plain_cart_mask(cart),
                mould: Mould::Gba,
                paint: &[],
                drawn: Some(Drawn {
                    depth: plain_cart_depth(cart),
                    detail: plain_detail_mask(cart),
                    max_px: label.3 as f32 / (MAX_LINES as f32 * 1.36),
                    max_h: f32::INFINITY,
                    rim: (5 * w + SEATED_W / 2) / SEATED_W,
                    board: Board {
                        x: (0.06, 0.94),
                        top: 0.04,
                        pins: (0.1, 0.9),
                        contacts_from: 0.93,
                        traces_from: 0.86,
                    },
                }),
            }
        }
    }
}

pub const SEATED_W: u32 = 240;
const SEATED_H: u32 = 141;
const GB_SEATED_H: u32 = 259;
/// A NES cart's face, connector foot included, is about 5:6.
const NES_SEATED_H: u32 = 288;
/// A US SNES cart's face is about 3:2, wide and low like a GBA's.
const SNES_SEATED_H: u32 = 157;
/// A Game Gear cart is about 5:4, wide and low.
const GG_SEATED_H: u32 = 192;
/// A Master System cart is about 4:5, connector foot included.
const SMS_SEATED_H: u32 = 300;
/// A Mega Drive cart is about 5:6, its connector inside the shell.
const MD_SEATED_H: u32 = 288;
/// A Neo Geo Pocket cart is a little taller than wide.
const NGP_SEATED_H: u32 = 228;
/// A WonderSwan cart is wider than tall.
const WS_SEATED_H: u32 = 200;
/// A CD jewel case is square.
const DISC_SEATED_H: u32 = 240;
/// A 32X cart is a Mega Drive one with a taller top.
const S32X_SEATED_H: u32 = 312;

pub fn seated_box(platform: Platform) -> (u32, u32) {
    if let Some(plain) = plain_cart_for(platform) {
        return (SEATED_W, plain_geometry(plain).1);
    }
    match platform {
        Platform::Gba => (SEATED_W, SEATED_H),
        Platform::Gb | Platform::Gbc => (SEATED_W, GB_SEATED_H),
        Platform::Nes => (SEATED_W, NES_SEATED_H),
        Platform::Snes => (SEATED_W, SNES_SEATED_H),
        _ => unreachable!("every other console has a plain cart"),
    }
}

pub fn cart_box(platform: Platform) -> (u32, u32) {
    let s = spec(match (platform, plain_cart_for(platform)) {
        (_, Some(plain)) => Shape::Plain(plain),
        (Platform::Gba, None) => Shape::Gba,
        (Platform::Gb | Platform::Gbc, None) => Shape::Gb(GbShell::Notched),
        (Platform::Nes, None) => Shape::Nes(NesShell::Nes),
        (Platform::Snes, None) => Shape::Snes(SnesShell::Us),
        _ => unreachable!("every other console has a plain cart"),
    });
    (s.w, s.h)
}

pub fn cart_shadow() -> CartFace {
    shadow(CART_W, CART_H, cart_mask())
}

pub fn gb_cart_shadow(shell: GbShell) -> CartFace {
    shadow(GB_CART_W, GB_CART_H, gb_cart_mask(shell))
}

pub fn nes_cart_shadow(shell: NesShell) -> CartFace {
    shadow(NES_CART_W, NES_CART_H, nes_cart_mask(shell))
}

pub fn snes_cart_shadow(shell: SnesShell) -> CartFace {
    shadow(SNES_CART_W, SNES_CART_H, snes_cart_mask(shell))
}

pub fn plain_cart_shadow(cart: PlainCart) -> CartFace {
    let (w, h) = plain_cart_box(cart);
    shadow(w, h, plain_cart_mask(cart))
}

fn shadow(w: u32, h: u32, mask: &[u8]) -> CartFace {
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for cover in mask {
        rgba.extend_from_slice(&[0, 0, 0, *cover]);
    }
    CartFace { rgba, w, h }
}

pub fn cart_face(cart: &Cart) -> CartFace {
    let (lw, lh) = label_size(cart);
    let art = fitted_label(cart, lw, lh);
    cart_face_with(cart, art)
}

pub fn is_clear(cart: &Cart) -> bool {
    shell_for(cart).finish != Finish::Solid
}

/// Whether the shelf draws this cart's coloured interior under its plastic: any clear cart.
pub fn shows_interior(cart: &Cart) -> bool {
    is_clear(cart) && matches!(cart.platform, Platform::Gba | Platform::Gb | Platform::Gbc)
}

/// The back plastic, with the game's colour blended onto it by the same overlay as the shells, at
/// 80% opacity. A GBA cart uses `gba_interior.png`, a Game Boy pak `gb_interior.png`, and a Game
/// Boy Color pak `gbc_interior.png`. Same size as that cart's board and face.
pub fn cart_interior(cart: &Cart) -> CartFace {
    let colour = shell_for(cart).colour;
    let (src, w, h, mask) = match gb_shell_of(cart) {
        None => (bitmaps::gba_interior(), BOARD_W, BOARD_H, None),
        Some(shell) => {
            let s = spec(Shape::Gb(shell));
            let src = match shell {
                GbShell::Notched => bitmaps::gb_interior(),
                GbShell::Rounded => bitmaps::gbc_interior(),
            };
            (src, s.w, s.h, Some(s.mask))
        }
    };
    let mut rgba = src.to_vec();
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        if let Some(mask) = mask {
            px[3] = ((px[3] as u32 * mask[i] as u32 + 127) / 255) as u8;
        }
        if px[3] == 0 {
            continue;
        }
        for c in 0..3 {
            px[c] = overlay(px[c], colour[c]);
        }
        px[3] = ((u32::from(px[3]) * u32::from(CLEAR_ALPHA) + 127) / 255) as u8;
    }
    CartFace { rgba, w, h }
}

/// The board a clear cart's plastic is drawn over, between the back and front shells. A GBA cart
/// gets `board.png`. A pak gets `gb_board.png`, cut to the notched or rounded outline.
pub fn cart_backing(shell: Option<GbShell>) -> CartFace {
    let Some(shell) = shell else {
        return CartFace {
            rgba: bitmaps::board().to_vec(),
            w: BOARD_W,
            h: BOARD_H,
        };
    };
    let s = spec(Shape::Gb(shell));
    let mut rgba = bitmaps::gb_board().to_vec();
    for (px, cover) in rgba.chunks_exact_mut(4).zip(s.mask) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
    }
}

pub fn label_size(cart: &Cart) -> (u32, u32) {
    let (_, _, lw, lh) = spec(shape_of(cart)).label;
    (lw, lh)
}

pub fn cart_face_with(cart: &Cart, art: Option<Vec<u8>>) -> CartFace {
    let s = spec(shape_of(cart));
    let shell = shell_for(cart);
    let mut face = match &s.drawn {
        Some(d) => drawn_shell_face(&s, d, &shell),
        None => shell_face(&s, &shell),
    };
    let label = match art {
        Some(rgba) => rgba,
        None => generated_label(&s, &label_text(cart)),
    };
    if let Some(d) = &s.drawn {
        mould_detail(d, &mut face);
        recess_label(&s, &mut face, &shell);
    }
    paste_label(&s, &mut face, &label);
    clip_to_silhouette(&s, &mut face);
    face
}

fn clip_to_silhouette(s: &Spec, face: &mut CartFace) {
    for (px, cover) in face.rgba.chunks_exact_mut(4).zip(s.mask) {
        px[3] = ((px[3] as u32 * *cover as u32 + 127) / 255) as u8;
    }
}

pub fn label_colour(title: &str) -> [u8; 3] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in title.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hsv_to_rgb((h % 360) as f32, 0.52, 0.74)
}

pub fn label_text(cart: &Cart) -> String {
    clean_label(&cart.stem)
}

pub fn clean_label(stem: &str) -> String {
    let mut bare = String::with_capacity(stem.len());
    let mut depth = 0u32;
    for ch in stem.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => bare.push(ch),
            _ => {}
        }
    }

    let mut out = String::with_capacity(bare.len());
    for word in bare.split_whitespace().filter(|w| *w != "-") {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        stem.to_string()
    } else {
        out
    }
}

pub fn label_tags(stem: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut cur = String::new();
    for ch in stem.chars() {
        match ch {
            '(' | '[' => {
                depth += 1;
                if depth == 1 {
                    cur.clear();
                    continue;
                }
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let t = cur.trim();
                    if !t.is_empty() {
                        out.push(t.to_string());
                    }
                    continue;
                }
            }
            _ => {}
        }
        if depth >= 1 {
            cur.push(ch);
        }
    }
    out
}

fn luma(px: &[u8]) -> u32 {
    (px[0] as u32 * 77 + px[1] as u32 * 150 + px[2] as u32 * 29) >> 8
}

fn shade_of(paint: &[u8], w: u32, h: u32, label: (u32, u32, u32, u32)) -> Vec<u32> {
    let (lx, ly, lw, lh) = label;
    let mut sum = 0u64;
    let mut n = 0u64;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if paint[i + 3] < 128 {
                continue;
            }
            let on_label = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
            if on_label {
                continue;
            }
            sum += u64::from(luma(&paint[i..]));
            n += 1;
        }
    }
    let mean = (sum / n.max(1)).max(1) as u32;
    paint
        .chunks_exact(4)
        .map(|px| (luma(px) * 256 / mean).clamp(176, 256))
        .collect()
}

/// The neutral shell asset for a mould: 50% grey plastic with the console name embossed on the
/// shoulder and the grey label box. `clear` makes the see-through variant, which is its own
/// asset with a rim and a sheen. The game's colour is blended on by `overlay`.
pub fn neutral_shell(mould: Mould, clear: bool) -> Vec<u8> {
    let shape = match mould {
        Mould::Gba => Shape::Gba,
        Mould::Notched => Shape::Gb(GbShell::Notched),
        Mould::Rounded => Shape::Gb(GbShell::Rounded),
    };
    let s = spec(shape);
    assert_eq!(
        s.paint.len(),
        (s.w * s.h * 4) as usize,
        "cartridge template is not {}x{}",
        s.w,
        s.h
    );
    let shade = shade_of(s.paint, s.w, s.h, s.label);
    let mut rgba = vec![0u8; (s.w * s.h * 4) as usize];
    let inside = |x: i64, y: i64| {
        (0..i64::from(s.w)).contains(&x)
            && (0..i64::from(s.h)).contains(&y)
            && s.mask[(y * i64::from(s.w) + x) as usize] >= 128
    };
    for (i, (dst, shade)) in rgba.chunks_exact_mut(4).zip(shade).enumerate() {
        let (x, y) = ((i as u32 % s.w) as i64, (i as u32 / s.w) as i64);
        let grey = if clear {
            // Glassy plastic: flat 50% grey body, a darker rim where the walls are thick, and a
            // faint lighter diagonal sheen across the face.
            let rim = (1..=CLEAR_RIM as i64).find(|d| {
                ![(x - d, y), (x + d, y), (x, y - d), (x, y + d)]
                    .iter()
                    .all(|&(px, py)| inside(px, py))
            });
            let sheen = (x + y - i64::from(s.w) / 3).abs() < i64::from(s.w) / 14;
            match (rim, sheen) {
                (Some(_), _) => NEUTRAL - 28,
                (None, true) => NEUTRAL + 24,
                (None, false) => NEUTRAL,
            }
        } else {
            // The mould's shading (256 is unshaded) falls below 50% grey.
            NEUTRAL - ((256 - shade.min(256)) / 2) as u8
        };
        dst.copy_from_slice(&[grey, grey, grey, 255]);
    }
    let mut face = CartFace {
        rgba,
        w: s.w,
        h: s.h,
    };
    clip_to_silhouette(&s, &mut face);
    dress(&s, &mut face.rgba);
    face.rgba
}

fn dress(s: &Spec, rgba: &mut [u8]) {
    let (x0, y0, lw, lh) = s.label;
    for y in y0..y0 + lh {
        for x in x0..x0 + lw {
            let i = ((y * s.w + x) * 4) as usize;
            if rgba[i + 3] == 0 {
                continue;
            }
            rgba[i..i + 3].copy_from_slice(&LABEL_WELL);
        }
    }
    let title = match s.mould {
        Mould::Gba => "GAME BOY ADVANCE",
        Mould::Notched => "GAME BOY",
        Mould::Rounded => "GAME BOY COLOR",
    };
    stamp_console(rgba, s.w, s.h, title, y0);
}

fn stamp_console(rgba: &mut [u8], w: u32, h: u32, title: &str, label_top: u32) {
    let Some(font) = text::label_font() else {
        return;
    };
    let band = label_top.max(1);
    let layout = text::fit_box(font, title, w as f32 * 0.72, band as f32, 2, 26.0, 9.0);
    let cov = text::coverage(w, band, &layout);
    let mut ink_top = band;
    let mut ink_bottom = 0u32;
    for y in 0..band {
        let row = &cov[(y * w) as usize..(y * w + w) as usize];
        if row.iter().any(|a| *a > 0) {
            ink_top = ink_top.min(y);
            ink_bottom = y;
        }
    }
    if ink_bottom < ink_top {
        return;
    }
    let text_h = ink_bottom - ink_top + 1;
    let dest_top = label_top.saturating_sub(3 + text_h);
    for y in ink_top..=ink_bottom {
        let dy = dest_top + (y - ink_top);
        if dy >= h {
            continue;
        }
        for x in 0..w {
            let a = cov[(y * w + x) as usize] as u32;
            if a == 0 {
                continue;
            }
            let i = ((dy * w + x) * 4) as usize;
            if rgba[i + 3] == 0 {
                continue;
            }
            let base = [rgba[i], rgba[i + 1], rgba[i + 2]];
            let ink = relief(base);
            for c in 0..3 {
                rgba[i + c] = ((ink[c] as u32 * a + base[c] as u32 * (255 - a)) / 255) as u8;
            }
        }
    }
}

/// The embossed name is a darker grey than the plastic around it; the overlay then darkens
/// whatever colour the game sets by the same proportion.
fn relief(px: [u8; 3]) -> [u8; 3] {
    px.map(|c| (c as u32 * 3 / 5) as u8)
}

const CLEAR_RIM: u32 = 5;

/// How much of a clear shell is opaque: 80%.
const CLEAR_ALPHA: u8 = 204;

/// The neutral shell's flat plastic: 50% grey, the level that leaves the game's colour as it is.
const NEUTRAL: u8 = 128;

/// Photoshop's Overlay blend of one channel: the neutral shell is the base layer and the game's
/// colour the blend layer. A base of 50% grey gives the colour exactly, a darker base darkens it
/// toward black and a lighter base lightens it toward white, so the shell's highlights, shadows
/// and embossed console name all follow whatever colour the game sets.
fn overlay(base: u8, colour: u8) -> u8 {
    let (base, colour) = (i32::from(base), i32::from(colour));
    let out = if base >= i32::from(NEUTRAL) {
        colour + ((255 - colour) * (base - i32::from(NEUTRAL)) + 63) / 127
    } else {
        colour - (colour * (i32::from(NEUTRAL) - base) + 64) / i32::from(NEUTRAL)
    };
    out.clamp(0, 255) as u8
}

fn shell_face(s: &Spec, shell: &Shell) -> CartFace {
    let clear = shell.finish != Finish::Solid;
    let src = bitmaps::shell_asset(s.mould, clear);
    assert_eq!(
        src.len(),
        (s.w * s.h * 4) as usize,
        "shell bitmap is not {}x{}",
        s.w,
        s.h
    );
    let mut rgba = src.to_vec();
    // The neutral asset is blended with the game's colour everywhere but the label box, which
    // stays the grey of the asset. A clear shell is its own asset with real transparency: what the
    // compositor drew underneath (the board) shows through and stays put while the shell moves.
    // The label box stays opaque.
    let (lx, ly, lw, lh) = s.label;
    for y in 0..s.h {
        for x in 0..s.w {
            let on_label = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
            let i = ((y * s.w + x) * 4) as usize;
            if !on_label {
                for c in 0..3 {
                    rgba[i + c] = overlay(rgba[i + c], shell.colour[c]);
                }
            }
            // The asset's own alpha is kept: its transparent corners and sides hold stray RGB
            // (often white), and only the alpha stops that being drawn.
            let own = u32::from(rgba[i + 3]);
            rgba[i + 3] = if on_label {
                255
            } else if clear {
                ((own * u32::from(CLEAR_ALPHA) + 127) / 255) as u8
            } else {
                own as u8
            };
        }
    }
    if shell.finish == Finish::Glitter {
        for y in 0..s.h {
            for x in 0..s.w {
                if !fleck(x, y) {
                    continue;
                }
                let i = ((y * s.w + x) * 4) as usize;
                if rgba[i + 3] == 0 {
                    continue;
                }
                let body = [rgba[i], rgba[i + 1], rgba[i + 2]];
                rgba[i..i + 3].copy_from_slice(&lerp(body, [0xff; 3], 100, 255));
                rgba[i + 3] = 255;
            }
        }
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
    }
}

fn fleck(x: u32, y: u32) -> bool {
    let mut h = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h.is_multiple_of(100)
}

fn lerp(a: [u8; 3], b: [u8; 3], num: u32, den: u32) -> [u8; 3] {
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = ((a[c] as u32 * (den - num) + b[c] as u32 * num) / den) as u8;
    }
    out
}

/// A NES or SNES cart's plastic, drawn from its outline's depth map (Slot-Brick's shell).
fn drawn_shell_face(s: &Spec, d: &Drawn, shell: &Shell) -> CartFace {
    let mut rgba = Vec::with_capacity((s.w * s.h * 4) as usize);
    let edge = rim_colour(through(shell.colour, shell.colour));
    for (i, depth) in d.depth.iter().enumerate() {
        let (x, y) = (i as u32 % s.w, i as u32 / s.w);
        let c = match shell.finish {
            Finish::Solid => shell.colour,
            Finish::Translucent | Finish::Glitter => {
                let body = through(shell.colour, inside(s, d, x, y).unwrap_or(shell.colour));
                let c = lerp(edge, body, (*depth as u32).min(d.rim), d.rim);
                if shell.finish == Finish::Glitter && fleck(x, y) {
                    lerp(c, [0xff; 3], 100, 255)
                } else {
                    c
                }
            }
        };
        rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
    }
    CartFace {
        rgba,
        w: s.w,
        h: s.h,
    }
}

const SURFACE: u32 = 150;

const BOARD: [u8; 3] = [0x2c, 0x96, 0x52];
const TRACE: [u8; 3] = [0x5a, 0xb4, 0x74];
const GOLD: [u8; 3] = [0xe6, 0xb4, 0x46];

fn through(plastic: [u8; 3], behind: [u8; 3]) -> [u8; 3] {
    let filter =
        |a: [u8; 3], b: [u8; 3]| [0, 1, 2].map(|c| (a[c] as u32 * b[c] as u32 / 255) as u8);
    lerp(
        filter(plastic, behind),
        filter(plastic, plastic),
        SURFACE,
        255,
    )
}

fn inside(s: &Spec, d: &Drawn, x: u32, y: u32) -> Option<[u8; 3]> {
    let b = &d.board;
    let (fx, fy) = (x as f32 / s.w as f32, y as f32 / s.h as f32);
    if !(b.x.0..b.x.1).contains(&fx) || fy < b.top {
        return None;
    }
    let (c0, c1) = b.pins;
    let pitch = (c1 - c0) / 32.0;
    let on_contact = (c0..c1).contains(&fx) && ((fx - c0) % pitch) < pitch * 0.6;
    if on_contact && fy >= b.contacts_from {
        return Some(GOLD);
    }
    if on_contact && ((fx - c0) % pitch) < pitch * 0.2 && fy >= b.traces_from {
        Some(TRACE)
    } else {
        Some(BOARD)
    }
}

fn rim_colour(base: [u8; 3]) -> [u8; 3] {
    let mean = (base[0] as u16 + base[1] as u16 + base[2] as u16) / 3;
    base.map(|c| {
        let grey = (3 * c as u16 + mean) / 4;
        (grey + (255 - grey) * 2 / 5) as u8
    })
}

const BEVEL: u32 = 3;

fn mould_detail(d: &Drawn, face: &mut CartFace) {
    let mix = |px: &mut [u8], to: [u8; 3], a: u32| {
        for c in 0..3 {
            px[c] = ((to[c] as u32 * a + px[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    };
    let sides = d.detail.shadow.iter().zip(&d.detail.highlight);
    for (px, (shade, light)) in face.rgba.chunks_exact_mut(4).zip(sides) {
        let under = [px[0], px[1], px[2]];
        if *shade > 0 {
            mix(px, under.map(|c| (c as f32 * 0.62) as u8), *shade as u32);
        }
        if *light > 0 {
            mix(
                px,
                under.map(|c| c + ((255 - c) as f32 * 0.24) as u8),
                *light as u32,
            );
        }
    }
}

fn recess_label(s: &Spec, face: &mut CartFace, shell: &Shell) {
    let shade = |c: [u8; 3], f: f32| -> [u8; 3] {
        [
            (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
            (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
        ]
    };
    let dark = shade(shell.colour, 0.55);
    let lit = shade(shell.colour, 1.45);

    let (lx, ly, lw, lh) = s.label;
    let (w, h) = (s.w, s.h);
    let bevel = (BEVEL * w + SEATED_W / 2) / SEATED_W;
    // A recess may run to the cart's edge (the NES label is open to the top).
    let (x0, y0) = (lx.saturating_sub(bevel), ly.saturating_sub(bevel));
    let (x1, y1) = (lx + lw + bevel, ly + lh + bevel);
    let clear = shell.finish != Finish::Solid;
    let mut put = |x: u32, y: u32, upper: bool| {
        if x >= w || y >= h {
            return;
        }
        let d = ((y * w + x) * 4) as usize;
        let c = if clear {
            let under = [face.rgba[d], face.rgba[d + 1], face.rgba[d + 2]];
            shade(under, if upper { 0.7 } else { 1.18 })
        } else if upper {
            dark
        } else {
            lit
        };
        face.rgba[d] = c[0];
        face.rgba[d + 1] = c[1];
        face.rgba[d + 2] = c[2];
    };
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = (lx..lx + lw).contains(&x) && (ly..ly + lh).contains(&y);
            if inside {
                continue;
            }
            let from_top = y.saturating_sub(y0);
            let from_left = x.saturating_sub(x0);
            let from_bottom = y1.saturating_sub(y + 1);
            let from_right = x1.saturating_sub(x + 1);
            let upper = from_top.min(from_left);
            let lower = from_bottom.min(from_right);
            put(x, y, upper <= lower);
        }
    }
}

fn generated_label(s: &Spec, title: &str) -> Vec<u8> {
    let (_, _, lw, lh) = s.label;
    let bg = label_colour(title);
    let mut rgba = Vec::with_capacity((lw * lh * 4) as usize);
    for _ in 0..lw * lh {
        rgba.extend_from_slice(&[bg[0], bg[1], bg[2], 255]);
    }

    if let Some(font) = text::label_font() {
        let (max_px, max_h) = match (&s.drawn, s.mould) {
            (Some(d), _) => (d.max_px, d.max_h),
            (None, Mould::Gba) => (lh as f32 / (MAX_LINES as f32 * 1.36), f32::INFINITY),
            (None, Mould::Notched | Mould::Rounded) => {
                let room = (lh - 2 * PAD) as f32;
                (room / MAX_LINES as f32, room)
            }
        };
        let layout = text::fit_box(
            font,
            title,
            (lw - 2 * PAD) as f32,
            max_h,
            MAX_LINES,
            max_px,
            MIN_PX,
        );
        text::draw_centred(&mut rgba, lw, lh, &layout, ink(bg));
    }
    rgba
}

fn ink(bg: [u8; 3]) -> [u8; 3] {
    let luma = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    if luma > 140.0 {
        [0x1a, 0x18, 0x16]
    } else {
        [0xf4, 0xf1, 0xea]
    }
}

fn paste_label(s: &Spec, face: &mut CartFace, label: &[u8]) {
    let (lx, ly, lw, lh) = s.label;
    for y in 0..lh {
        for x in 0..lw {
            let src = ((y * lw + x) * 4) as usize;
            let a = label[src + 3] as u32;
            if a == 0 {
                continue;
            }
            let d = (((y + ly) * s.w + x + lx) * 4) as usize;
            for c in 0..3 {
                face.rgba[d + c] =
                    ((label[src + c] as u32 * a + face.rgba[d + c] as u32 * (255 - a) + 127) / 255)
                        as u8;
            }
        }
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [
        ((r + m) * 255.0).round() as u8,
        ((g + m) * 255.0).round() as u8,
        ((b + m) * 255.0).round() as u8,
    ]
}
