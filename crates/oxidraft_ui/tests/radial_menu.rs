use oxidraft_ui::{AppState, UiState, draw_ui, egui};

/// Runs one full frame of the real UI (top bar, ribbon, canvas, every HUD —
/// not just the radial menu in isolation) with the given input events
/// injected. Mirrors `tests/text_focus.rs`'s driver.
#[allow(deprecated)] // Context::run / CentralPanel::show: fine for a synchronous test driver
fn frame(
    ctx: &egui::Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    events: Vec<egui::Event>,
) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        events,
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            draw_ui(ui, app, ui_state);
        });
    });
}

fn pointer_move(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![egui::Event::PointerMoved(pos)]
}

fn pointer_click(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn key(key: egui::Key, pressed: bool) -> Vec<egui::Event> {
    vec![egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]
}

/// Regression test for a real bug: `radial_menu()` must run before every
/// other chrome panel in `draw_ui` so it can consume the Tab press before
/// egui's own focus-cycling does. When it ran later, holding Tab moved
/// keyboard focus to the top bar's first button instead of opening the pie.
/// If a future refactor reorders `draw_ui`'s calls, this catches it.
#[test]
fn holding_tab_over_the_canvas_opens_the_radial_menu() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    let canvas_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(canvas_pos));
    assert!(!ui_state.radial_open);

    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Tab, true));
    assert!(
        ui_state.radial_open,
        "holding Tab over the canvas should open the radial menu"
    );
}

/// The open-gate must defer to any already-open modal dialog rather than
/// summon the pie on top of it.
#[test]
fn tab_does_not_open_radial_menu_while_settings_dialog_is_open() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();
    ui_state.settings_open = true;

    let canvas_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(canvas_pos));
    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Tab, true));
    assert!(
        !ui_state.radial_open,
        "Tab should not open the radial menu while Settings is open"
    );
}

/// Escape must only dismiss the pie, not cancel whatever the user was doing
/// underneath it — it used to double-fire because the radial menu read
/// Escape without consuming it, so canvas()'s own Escape handler saw the
/// same keypress and cancelled the in-progress tool.
#[test]
fn escape_closes_radial_menu_without_cancelling_the_active_tool() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state, vec![]);
    app.run_command("POLYLINE");

    let p1 = egui::pos2(500.0, 400.0);
    let p2 = egui::pos2(600.0, 340.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(p1));
    frame(&ctx, &mut app, &mut ui_state, pointer_click(p1));
    frame(&ctx, &mut app, &mut ui_state, pointer_move(p2));
    frame(&ctx, &mut app, &mut ui_state, pointer_click(p2));
    assert!(
        matches!(&app.tool, oxidraft_ui::tools::Tool::Polyline { pts } if pts.len() == 2),
        "expected 2 placed points before opening the radial menu, tool={:?}",
        app.tool
    );

    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Tab, true));
    assert!(ui_state.radial_open);

    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Escape, true));
    assert!(!ui_state.radial_open, "Escape should close the radial menu");
    assert!(
        matches!(&app.tool, oxidraft_ui::tools::Tool::Polyline { pts } if pts.len() == 2),
        "Escape should only dismiss the radial menu, not cancel the in-progress polyline; tool={:?}",
        app.tool
    );
}
