use oxidraft_document::EntityKind;
use oxidraft_geometry::{Curve, LineSeg, Point2d};
use oxidraft_ui::AppState;

fn pt(x: i64, y: i64) -> Point2d {
    Point2d::from_i64(x, y)
}

fn app_with_line() -> AppState {
    let mut a = AppState::new(800.0, 600.0);
    a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
        pt(0, 0),
        pt(10, 0),
    ))));
    a
}

#[test]
fn line_tool_snaps_to_existing_endpoint() {
    let mut a = app_with_line();
    a.snap_on = true;
    a.run_command("LINE");

    let (s1x, s1y) = a.view.world_to_screen(0.05, 0.05);
    a.canvas_click(s1x, s1y);
    let (s2x, s2y) = a.view.world_to_screen(10.05, -0.03);
    a.canvas_click(s2x, s2y);

    let non_origin: Vec<_> = a.document.iter().filter(|e| e.id != a.origin_id).collect();
    assert_eq!(non_origin.len(), 2);
    let new_line = non_origin[1];
    if let Some(Curve::Line(l)) = new_line.as_curve() {
        let (x0, y0) = l.p0.to_f64();
        let (x1, y1) = l.p1.to_f64();
        assert!(
            x0.abs() < 1e-6 && y0.abs() < 1e-6,
            "start should snap to (0,0): ({},{})",
            x0,
            y0
        );
        assert!(
            (x1 - 10.0).abs() < 1e-6 && y1.abs() < 1e-6,
            "end should snap to (10,0): ({},{})",
            x1,
            y1
        );
    } else {
        panic!("expected a line");
    }
}

#[test]
fn click_select_then_erase() {
    let mut a = app_with_line();
    a.snap_on = false;
    let (sx, sy) = a.view.world_to_screen(5.0, 0.0);
    a.canvas_click(sx, sy);
    assert_eq!(
        a.selection.len(),
        1,
        "click should select the line under cursor"
    );
    a.run_command("ERASE");
    assert_eq!(a.document.len(), 1);
}

#[test]
fn draw_undo_redo_cycle() {
    let mut a = AppState::new(800.0, 600.0);
    a.snap_on = false;
    a.run_command("LINE");
    let (a1x, a1y) = a.view.world_to_screen(0.0, 0.0);
    let (a2x, a2y) = a.view.world_to_screen(4.0, 3.0);
    a.canvas_click(a1x, a1y);
    a.canvas_click(a2x, a2y);
    assert_eq!(a.document.len(), 2);

    a.undo();
    assert_eq!(a.document.len(), 1);
    a.redo();
    assert_eq!(a.document.len(), 2);
    let e = a.document.iter().find(|e| e.id != a.origin_id).unwrap();
    if let Some(Curve::Line(l)) = e.as_curve() {
        assert!((l.p1.x - 4.0).abs() < 1e-4 && (l.p1.y - 3.0).abs() < 1e-4);
    } else {
        panic!();
    }
}

#[test]
fn copy_command_full_flow() {
    let mut a = app_with_line();
    a.snap_on = false;
    let id = a.document.iter().find(|e| e.id != a.origin_id).unwrap().id;
    a.selection = vec![id];
    a.run_command("COPY");
    let (b1x, b1y) = a.view.world_to_screen(0.0, 0.0);
    let (b2x, b2y) = a.view.world_to_screen(0.0, 5.0);
    a.canvas_click(b1x, b1y);
    a.canvas_click(b2x, b2y);
    assert_eq!(a.document.len(), 3, "copy should add one entity");
    let ys: Vec<f64> = a
        .document
        .iter()
        .filter_map(|e| e.as_curve())
        .filter_map(|c| {
            if let Curve::Line(l) = c {
                Some(l.p0.y)
            } else {
                None
            }
        })
        .collect();
    assert!(
        ys.iter().any(|&y| y.abs() < 1e-4),
        "original at y=0 missing"
    );
    assert!(
        ys.iter().any(|&y| (y - 5.0).abs() < 1e-4),
        "copy at y=5 missing"
    );
}
