#![cfg(feature = "brick")]
use slot::app::App;
use slot_input::{Action, Btn};
#[test]
fn holding_power_shuts_down_and_release_changes_nothing() {
    let mut app = App::new(Vec::new());
    app.apply(Action::PowerHold);
    assert!(app.powering_off());
    assert!(!app.restarting());
    app.apply(Action::PowerOff); // emitted when the held power button is released
    assert!(app.powering_off());
    assert!(!app.restarting());
}
#[test]
fn x_on_about_exits_to_the_stock_os() {
    let mut app = App::new(Vec::new());
    app.apply(Action::QuickMenu);
    while app.quick_menu() != Some(slot_ui::QuickRow::About) {
        app.apply(Action::GbaDown(Btn::Down));
    }
    app.apply(Action::GbaDown(Btn::A));
    app.apply(Action::GbaDown(Btn::X));
    assert!(app.restarting());
    assert!(!app.powering_off());
}
