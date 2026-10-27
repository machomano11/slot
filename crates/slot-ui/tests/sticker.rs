use slot_ui::{sticker_lines, StickerFields};

fn fields() -> StickerFields<'static> {
    StickerFields {
        battery: Some(87),
        serial: "04738857",
        version: "0.1.0",
        model: "RG SP",
    }
}

#[test]
fn the_headline_rows_name_the_brick_port() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("TRIMUI BRICK"), "{all}");
    assert!(all.contains(slot_ui::PORT_VERSION), "{all}");
    assert!(all.contains("87"), "the gauge reading is missing: {all}");
    assert!(all.contains("0473885"), "the serial went missing: {all}");
    assert!(
        !all.contains("AGS-102"),
        "the RG SP model plate is still there: {all}"
    );
}

#[test]
fn the_about_screen_says_it_is_a_port_of_the_original() {
    let all = sticker_lines(&fields()).join(" ");
    for owed in [
        "UNOFFICIAL PORT OF SLOT",
        "BRANDON T. KOWALSKI",
        "ANBERNIC RG SP",
        "TRIMUI BRICK PORT",
        "PORT BY LORDNORDYX",
    ] {
        assert!(
            all.contains(owed),
            "the About text does not mention {owed}: {all}"
        );
    }
}

#[test]
fn a_missing_gauge_is_not_drawn_as_empty() {
    let mut f = fields();
    f.battery = None;
    let all = sticker_lines(&f).join("\n");
    assert!(
        !all.contains("0%"),
        "no gauge was drawn as a flat battery: {all}"
    );
    assert!(
        all.contains("BATTERY"),
        "the row should still be there: {all}"
    );
}

#[test]
fn the_compliance_block_is_the_credits() {
    let all = sticker_lines(&fields()).join("\n").to_uppercase();
    for owed in ["MGBA", "GPSP", "OPEN SANS", "NERD", "LCD3X", "MARKUS"] {
        assert!(all.contains(owed), "the credits do not mention {owed}");
    }
}

#[test]
fn the_serial_row_matches_the_encoded_hash() {
    let all = sticker_lines(&fields()).join("\n");
    assert!(all.contains("0473885 7"), "{all}");
}

#[test]
fn every_line_is_already_upper_case() {
    for line in sticker_lines(&fields()) {
        assert_eq!(line, line.to_uppercase(), "{line}");
    }
}
