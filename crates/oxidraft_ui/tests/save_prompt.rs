use oxidraft_ui::{AppState, PendingSaveAction, UiState, draw_ui, egui};

/// Same driver pattern as plot_dialog.rs / command_toast.rs / text_focus.rs:
/// a real pass over the whole draw_ui pipeline, not just the dialog in
/// isolation.
fn frame(ctx: &egui::Context, app: &mut AppState, ui_state: &mut UiState) {
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1200.0, 800.0),
        )),
        ..Default::default()
    };
    let _ = ctx.run_ui(raw, |ui| {
        draw_ui(ui, app, ui_state);
    });
}

fn save_prompt_window_id() -> egui::Id {
    egui::Id::new("save_prompt_window")
}

/// Reproduces the state a dirty New/Open/Close leaves behind (see
/// `chrome::request_or_run`) and confirms the app's own prompt actually
/// renders through the real pipeline, replacing the native OS message box
/// that used to show here.
#[test]
fn a_pending_save_action_shows_the_apps_own_prompt() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state);
    assert!(
        ctx.memory(|m| m.area_rect(save_prompt_window_id()))
            .is_none(),
        "the prompt must be closed with nothing pending"
    );

    ui_state.pending_save_prompt = Some(PendingSaveAction::New);
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);

    assert!(
        ctx.memory(|m| m.area_rect(save_prompt_window_id()))
            .is_some(),
        "a pending save action must show the app's own prompt window"
    );
}
