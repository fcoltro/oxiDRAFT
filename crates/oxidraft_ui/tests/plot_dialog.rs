use oxidraft_ui::{AppState, UiState, draw_ui, egui};

/// Same driver pattern as command_toast.rs / text_focus.rs: a real ctx.run()
/// over the whole draw_ui pipeline, not just the dialog in isolation.
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

/// Reproduces the File > Plot... menu action (sets the same ctx temp flag
/// `plot_dialog` reads) and confirms the dialog actually renders through the
/// real pipeline, not just that `export_pdf` works in isolation.
#[test]
fn setting_the_open_plot_flag_shows_the_plot_window() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state);
    assert!(
        ctx.memory(|m| m.area_rect(egui::Id::new("Plot"))).is_none(),
        "Plot dialog should be closed by default"
    );

    ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_plot"), true));
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);

    assert!(
        ctx.memory(|m| m.area_rect(egui::Id::new("Plot"))).is_some(),
        "Plot dialog should be visible once the open_plot flag is set"
    );
}

/// A finished plot-window pick sets `reopen_plot`; the next frames must
/// bring the dialog back in Window mode without any menu interaction.
#[test]
fn a_finished_window_pick_reopens_the_plot_dialog() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    frame(&ctx, &mut app, &mut ui_state);
    assert!(ctx.memory(|m| m.area_rect(egui::Id::new("Plot"))).is_none());

    // What apply_tool_event leaves behind after the second corner click.
    app.plot_window = Some((0.0, 0.0, 40.0, 30.0));
    app.reopen_plot = true;
    frame(&ctx, &mut app, &mut ui_state);
    frame(&ctx, &mut app, &mut ui_state);

    assert!(
        ctx.memory(|m| m.area_rect(egui::Id::new("Plot"))).is_some(),
        "the dialog must reopen after the canvas pick"
    );
    assert!(!app.reopen_plot, "the reopen flag is one-shot");
    assert_eq!(
        ctx.data(|d| d.get_temp::<usize>(egui::Id::new("plot_area_mode"))),
        Some(1),
        "the dialog reopens in Window mode"
    );
}
