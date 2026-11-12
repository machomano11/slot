pub const CARDS: &str = "/proc/asound/cards";

pub fn usb_card(cards: &str) -> Option<u32> {
    cards.lines().find_map(|line| {
        let (index, rest) = line.trim_start().split_once(' ')?;
        let index = index.parse().ok()?;
        rest.contains("USB-Audio").then_some(index)
    })
}

pub fn present_usb_card() -> Option<u32> {
    std::fs::read_to_string(CARDS)
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
