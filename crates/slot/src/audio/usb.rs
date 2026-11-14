use slot_power::{usb_card, SOUND_CARDS};

pub fn present_usb_card() -> Option<u32> {
    std::fs::read_to_string(SOUND_CARDS)
        .ok()
        .as_deref()
        .and_then(usb_card)
}

pub fn devices(usb: Option<u32>, fallback: &[&str]) -> Vec<String> {
    usb.map(|n| format!("plughw:{n},0"))
        .into_iter()
        .chain(fallback.iter().map(|d| d.to_string()))
        .collect()
}
