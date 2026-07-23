use oxidraft_ui::{AppState, UiState, draw_ui, egui};

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
    let _ = ctx.run_ui(raw, |ui| {
        draw_ui(ui, app, ui_state);
    });
}

fn pointer_move(pos: egui::Pos2) -> Vec<egui::Event> {
    vec![egui::Event::PointerMoved(pos)]
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

#[test]
fn holding_q_over_the_canvas_opens_the_radial_menu() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();
    let canvas_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(canvas_pos));
    assert!(!ui_state.radial_open);
    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Q, true));
    assert!(
        ui_state.radial_open,
        "holding Q over the canvas should open the radial menu"
    );
}

#[test]
fn q_does_not_open_radial_menu_while_settings_dialog_is_open() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState {
        settings_open: true,
        ..Default::default()
    };
    let canvas_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(canvas_pos));
    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Q, true));
    assert!(
        !ui_state.radial_open,
        "Q should not open the radial menu while Settings is open"
    );
}

#[test]
fn escape_closes_radial_menu_without_cancelling_the_active_tool() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();
    frame(&ctx, &mut app, &mut ui_state, vec![]);
    app.run_command("POLYLINE");
    // Place the two polyline points directly. egui's hit-testing (which the
    // canvas click handler needs, via response.contains_pointer) does not
    // register under synthetic RawInput in a headless run_ui, so a real
    // pointer_click can't drive placement here; canvas_click is the exact entry
    // point that handler calls. The behaviour under test — Escape closing the
    // radial without cancelling the in-progress tool — is driven through frames.
    let (s1x, s1y) = app.view.world_to_screen(-1.0, 0.0);
    app.canvas_click(s1x, s1y);
    let (s2x, s2y) = app.view.world_to_screen(1.0, 0.5);
    app.canvas_click(s2x, s2y);
    // Put the egui pointer over the canvas so Q (a geometric latest-pos check)
    // opens the radial.
    frame(
        &ctx,
        &mut app,
        &mut ui_state,
        pointer_move(egui::pos2(600.0, 400.0)),
    );
    assert!(
        matches!(& app.tool, oxidraft_ui::tools::Tool::Polyline { pts } if pts.len() ==
        2),
        "expected 2 placed points before opening the radial menu, tool={:?}",
        app.tool
    );
    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Q, true));
    assert!(ui_state.radial_open);
    frame(&ctx, &mut app, &mut ui_state, key(egui::Key::Escape, true));
    assert!(!ui_state.radial_open, "Escape should close the radial menu");
    assert!(
        matches!(& app.tool, oxidraft_ui::tools::Tool::Polyline { pts } if pts.len() ==
        2),
        "Escape should only dismiss the radial menu, not cancel the in-progress polyline; tool={:?}",
        app.tool
    );
}
