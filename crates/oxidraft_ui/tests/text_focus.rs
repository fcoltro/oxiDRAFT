use oxidraft_document::EntityKind;
use oxidraft_ui::{AppState, UiState, draw_ui, egui};

/// Runs one full frame of the real UI (top bar, ribbon, canvas, every HUD —
/// not just `dyn_text_hud` in isolation) with the given input events injected
/// at the canvas's screen position.
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

#[test]
fn text_tool_focuses_field_and_enter_creates_entity_through_the_real_pipeline() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    // Frame 1: draw once so the app/ui exist in a known state, then activate TEXT.
    frame(&ctx, &mut app, &mut ui_state, vec![]);
    app.run_command("TEXT");
    assert!(matches!(
        app.tool,
        oxidraft_ui::tools::Tool::Text { anchor: None, .. }
    ));

    // Frame 2: click on the canvas to place the text anchor, exactly as a
    // real user click is routed (response.contains_pointer() +
    // primary_pressed()), through the FULL draw_ui pipeline — not by poking
    // app.tool directly.
    let click_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(click_pos));
    frame(&ctx, &mut app, &mut ui_state, pointer_click(click_pos));
    assert!(
        matches!(
            app.tool,
            oxidraft_ui::tools::Tool::Text {
                anchor: Some(_),
                ..
            }
        ),
        "click should have placed the text anchor, tool={:?}",
        app.tool
    );

    // Frame 3: the dyn_text_hud popup should now exist and have grabbed
    // keyboard focus on the text field, same as it would for a real user.
    frame(&ctx, &mut app, &mut ui_state, vec![]);
    let field_id = egui::Id::new("dyn_text_field");
    let focused = ctx.memory(|m| m.has_focus(field_id));
    assert!(
        focused,
        "text field must have keyboard focus after the anchor click"
    );

    // Frame 4: type through the real pipeline.
    frame(
        &ctx,
        &mut app,
        &mut ui_state,
        vec![egui::Event::Text("Hello".into())],
    );

    // Frame 5: Enter should commit the typed text as a Text entity.
    frame(
        &ctx,
        &mut app,
        &mut ui_state,
        vec![
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );

    let texts: Vec<&str> = app
        .document
        .editable_entities()
        .filter_map(|e| match &e.kind {
            EntityKind::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts,
        vec!["Hello"],
        "Enter should place the typed text as a Text entity through the real pipeline; tool={:?}",
        app.tool
    );
}

#[test]
fn text_tool_focuses_on_a_second_text_right_after_the_first() {
    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();
    let field_id = egui::Id::new("dyn_text_field");
    frame(&ctx, &mut app, &mut ui_state, vec![]);

    // Kept close to screen center (matches the other test's click_pos) so
    // neither point lands under the always-docked chrome (top bar, ribbon,
    // inspector, constraint bar) — this is about text-tool refocus, not
    // chrome layout, so the click just needs to land on open canvas.
    for (i, world_pt) in [(-2.0, 1.0), (2.0, -1.0)].into_iter().enumerate() {
        app.run_command("TEXT");
        let (sx, sy) = app.view.world_to_screen(world_pt.0, world_pt.1);
        let pos = egui::pos2(sx as f32, sy as f32);
        frame(&ctx, &mut app, &mut ui_state, pointer_move(pos));
        frame(&ctx, &mut app, &mut ui_state, pointer_click(pos));
        assert!(
            matches!(
                app.tool,
                oxidraft_ui::tools::Tool::Text {
                    anchor: Some(_),
                    ..
                }
            ),
            "text #{i}: click should place the anchor, tool={:?}",
            app.tool
        );

        frame(&ctx, &mut app, &mut ui_state, vec![]);
        assert!(
            ctx.memory(|m| m.has_focus(field_id)),
            "text #{i}: field must be focused"
        );

        frame(
            &ctx,
            &mut app,
            &mut ui_state,
            vec![egui::Event::Text(format!("Text{i}"))],
        );
        frame(
            &ctx,
            &mut app,
            &mut ui_state,
            vec![
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert!(
            matches!(app.tool, oxidraft_ui::tools::Tool::Select),
            "text #{i}: Enter should commit and return to Select, tool={:?}",
            app.tool
        );
    }

    let texts: Vec<&str> = app
        .document
        .editable_entities()
        .filter_map(|e| match &e.kind {
            EntityKind::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["Text0", "Text1"]);
}

#[test]
fn text_tool_focuses_after_leaving_a_blend_popup_field_focused() {
    // Reproduces: pick two entities for BLEND (leaving the confirm popup's
    // tension field auto-focused on first show), then switch straight to
    // TEXT *without* clicking Apply/Cancel first — the lingering focus on
    // the now-hidden "blend_confirm_tension" field must not block the text
    // field from grabbing focus afterward.
    use oxidraft_document::EntityKind as EK;
    use oxidraft_geometry::{Curve, LineSeg, Point2d};

    let ctx = egui::Context::default();
    let mut app = AppState::new(1200.0, 800.0);
    let mut ui_state = UiState::default();

    let a = app
        .document
        .add(EK::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(0.0, 0.0),
            Point2d::from_f64(2.0, 0.0),
        ))));
    let _ = a;
    app.document
        .add(EK::Curve(Curve::Line(LineSeg::from_endpoints(
            Point2d::from_f64(5.0, 0.0),
            Point2d::from_f64(7.0, 0.0),
        ))));

    app.run_command("BLEND");
    let (s1x, s1y) = app.view.world_to_screen(1.0, 0.0);
    app.canvas_click(s1x, s1y);
    let (s2x, s2y) = app.view.world_to_screen(6.0, 0.0);
    app.canvas_click(s2x, s2y);
    assert!(matches!(
        app.tool,
        oxidraft_ui::tools::Tool::Blend {
            second: Some(_),
            ..
        }
    ));

    // Frame: blend_confirm_hud shows and auto-focuses its tension field.
    frame(&ctx, &mut app, &mut ui_state, vec![]);
    let tension_id = egui::Id::new("blend_confirm_tension");
    assert!(
        ctx.memory(|m| m.has_focus(tension_id)),
        "sanity check: blend's tension field should have grabbed focus on first show"
    );

    // Switch directly to TEXT without Apply/Cancel — the tension field's
    // focus is left dangling in ctx.memory since nothing surrendered it.
    app.run_command("TEXT");

    let click_pos = egui::pos2(600.0, 400.0);
    frame(&ctx, &mut app, &mut ui_state, pointer_move(click_pos));
    frame(&ctx, &mut app, &mut ui_state, pointer_click(click_pos));
    assert!(
        matches!(
            app.tool,
            oxidraft_ui::tools::Tool::Text {
                anchor: Some(_),
                ..
            }
        ),
        "click should place the text anchor, tool={:?}",
        app.tool
    );

    frame(&ctx, &mut app, &mut ui_state, vec![]);
    let field_id = egui::Id::new("dyn_text_field");
    assert!(
        ctx.memory(|m| m.has_focus(field_id)),
        "text field must grab focus even though blend's tension field never explicitly surrendered it"
    );
}
