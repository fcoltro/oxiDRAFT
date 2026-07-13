use oxidraft_ui::{AppState, UiState, draw_ui, egui};

/// Same driver pattern as command_toast.rs / text_focus.rs: a real pass over
/// the whole draw_ui pipeline, not just the dialog in isolation.
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

/// The pinned `id` given to the Plot `Window` in `chrome::plot_dialog`. Since
/// egui 0.35 a `Window`'s auto-derived area id comes from `Atoms::text()`
/// (an `Option<Cow<str>>`), not the bare title, so the dialog carries an
/// explicit id and the tests look it up by that same id.
fn plot_window_id() -> egui::Id {
    egui::Id::new("plot_dialog_window")
}

/// Reproduces the File > Plot... menu action (sets the same AppState flag
/// `plot_dialog` reads) and confirms the dialog actually renders through the
/// real pipeline, not just that `export_pdf` works in isolation.
#[test]
fn setting_the_open_plot_flag_shows_the_plot_window() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state);
    assert!(
        ctx.memory(|m| m.area_rect(plot_window_id())).is_none(),
        "Plot dialog should be closed by default"
    );

    app.plot_dialog_open = true;
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);

    assert!(
        ctx.memory(|m| m.area_rect(plot_window_id())).is_some(),
        "Plot dialog should be visible once plot_dialog_open is set"
    );
}

/// A finished plot-window pick leaves `plot_dialog_open`/`plot_window_mode`
/// set on AppState; the dialog must render in Window mode with no menu
/// interaction.
#[test]
fn a_finished_window_pick_reopens_the_plot_dialog() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state);
    assert!(ctx.memory(|m| m.area_rect(plot_window_id())).is_none());

    // What apply_tool_event leaves behind after the second corner click.
    app.plot_window = Some((0.0, 0.0, 40.0, 30.0));
    app.plot_dialog_open = true;
    app.plot_window_mode = true;
    frame(&ctx, &mut app, &mut ui_state);

    assert!(
        ctx.memory(|m| m.area_rect(plot_window_id())).is_some(),
        "the dialog must reopen after the canvas pick"
    );
    assert!(
        app.plot_window_mode,
        "the dialog stays in Window mode after the pick"
    );
}
