use oxidraft_ui::{AppState, UiState, draw_ui, egui};

/// Runs one full frame of the real UI through the same driver pattern as
/// text_focus.rs (ctx.run() over the whole draw_ui pipeline, not just the
/// toast in isolation).
#[allow(deprecated)] // Context::run / CentralPanel::show: fine for a synchronous test driver
fn frame(ctx: &egui::Context, app: &mut AppState, ui_state: &mut UiState) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run(raw, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            draw_ui(ui, app, ui_state);
        });
    });
}

/// Reproduces a reported bug: a short command_log message narrows the
/// toast's persisted egui Area width, and a later, much longer message
/// then wraps inside that same narrow width instead of widening back out
/// (a Label never reports a desired width wider than the one it was
/// given, so the narrow width gets stuck forever). The toast must force a
/// fresh sizing pass whenever the message actually changes.
#[test]
fn command_toast_widens_back_out_for_a_long_message_after_a_short_one() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();
    let toast_id = egui::Id::new("command_toast");

    app.command_log.push("Hi".to_string());
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);
    let narrow_rect = ctx
        .memory(|m| m.area_rect(toast_id))
        .expect("toast should be visible after a short message");

    app.command_log.push(
        "Could not make the lines perpendicular against their existing constraints; \
         conflicts with its existing coincident constraint"
            .to_string(),
    );
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);
    let long_rect = ctx
        .memory(|m| m.area_rect(toast_id))
        .expect("toast should be visible after a long message");

    assert!(
        narrow_rect.width() < 120.0,
        "sanity check: short message should be narrow, got {}",
        narrow_rect.width()
    );
    assert!(
        long_rect.width() > 300.0,
        "toast width must grow back out for a long message instead of staying \
         stuck at the previous short message's narrow width; narrow={}, long={} (rows={})",
        narrow_rect.width(),
        long_rect.width(),
        long_rect.height() / 16.0,
    );
}

/// The toast must sit in the empty gap to the left of the status pill,
/// vertically centered on it — not floating above it.
#[test]
fn command_toast_sits_left_of_the_status_pill() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    app.command_log.push("Saved drawing".to_string());
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);

    let toast_rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("command_toast")))
        .expect("toast should be visible after a message");
    let pill_rect = ctx
        .memory(|m| m.area_rect(egui::Id::new("status_pill")))
        .expect("status pill should always be visible");

    assert!(
        toast_rect.right() <= pill_rect.left() + 1.0,
        "toast must sit to the left of the status pill, not overlap or float above it; \
         toast={toast_rect:?}, pill={pill_rect:?}"
    );
    assert!(
        (toast_rect.center().y - pill_rect.center().y).abs() < 1.0,
        "toast must be vertically centered on the status pill; toast={toast_rect:?}, pill={pill_rect:?}"
    );
}
