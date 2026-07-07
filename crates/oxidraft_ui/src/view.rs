use egui::{CentralPanel, Color32, Sense, Stroke, pos2, vec2};
use oxidraft_cad::GripRole;
use oxidraft_document::{EntityId, EntityKind};
use oxidraft_geometry::Point2d;

use crate::command::Command;
use crate::state::AppState;
use crate::tools::Tool;

mod chrome;
pub(crate) mod overlays;
mod palette;
mod render;
mod tessellate;
use chrome::{
    about_window, command_toast, constraint_bar, contextual_toolbar, handle_shortcuts, inspector,
    line_props_dialog, plot_dialog, ribbon, settings_dialog, status_pill, tool_hint_panel, top_bar,
};
use palette::command_bar;
use render::{
    HATCH_SELECT, draw_blend_preview, draw_corner_preview, draw_dashed_line, draw_dimension,
    draw_entity, draw_grid, draw_prompt_chip, draw_scale_bar, draw_transform_ghost,
    draw_trim_extend_preview, layer_visible, refresh_curve_cache, refresh_hatch_cache,
    refresh_text_cache, resolve_color, resolve_line_weight_px, tool_prompt,
};
use tessellate::draw_curve;

pub type HatchCache =
    std::collections::HashMap<EntityId, (u64, Vec<[Point2d; 3]>, Vec<Vec<Point2d>>)>;

pub type TextCache = std::collections::HashMap<EntityId, (u64, Vec<[Point2d; 3]>)>;

// World-space flattened points per curve entity, plus its closed/open flag.
pub type CurveCache = std::collections::HashMap<EntityId, (u64, Vec<Point2d>, bool)>;

#[derive(Default)]
pub struct UiState {
    pub command_input: String,
    pub dyn_length: String,
    pub dyn_angle: String,
    pub dyn_active: bool,
    pub dyn_radius: String,
    pub dyn_circle_active: bool,
    pub dyn_rect_width: String,
    pub dyn_rect_height: String,
    pub dyn_rect_active: bool,
    pub dyn_rect_stage_h: bool,
    pub dyn_poly_sides: String,
    pub dyn_poly_active: bool,
    pub dyn_ell_major: String,
    pub dyn_ell_minor: String,
    pub dyn_ell_active: bool,
    pub dyn_offset_dist: String,
    pub dyn_offset_active: bool,
    pub dyn_tf_dx: String,
    pub dyn_tf_dy: String,
    pub dyn_tf_angle: String,
    pub dyn_tf_factor: String,
    pub dyn_tf_active: bool,
    pub dyn_corner_val: String,
    pub dyn_corner_active: bool,
    pub blend_confirm_tension: String,
    pub blend_confirm_active: bool,
    pub dyn_text_content: String,
    pub dyn_text_active: bool,
    pub corner_input: String,
    pub grip_input: String,
    pub editing_text: Option<EntityId>,
    pub text_edit_buf: String,
    pub text_edit_active: bool,
    pub text_edit_font: Option<String>,
    pub text_edit_size: f64,
    pub palette_index: usize,
    pub palette_nav: bool,
    pub last_title: String,
    pub about_open: bool,
    pub line_props_open: bool,
    pub settings_open: bool,
    pub hatch_cache: HatchCache,
    pub text_cache: TextCache,
    pub curve_cache: CurveCache,
    pub last_autosave: Option<std::time::Instant>,
    pub recovery_offer: Option<std::path::PathBuf>,
    pub recovery_checked: bool,
}

pub fn draw_ui(ui: &mut egui::Ui, app: &mut AppState, ui_state: &mut UiState) {
    let ctx = ui.ctx().clone();
    crate::theme::apply(&ctx);
    crate::fonts::warm();
    if app.tick_zoom_anim() {
        ctx.request_repaint();
    }
    if !ui_state.recovery_checked {
        ui_state.recovery_checked = true;
        ui_state.recovery_offer = crate::autosave::pending_recovery();
    }
    crate::autosave::tick(app, &mut ui_state.last_autosave);
    if let Some(path) = ui_state.recovery_offer.clone() {
        egui::Window::new("Recover unsaved work")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(&ctx, |ui| {
                ui.label(
                    "oxiDRAFT found an autosaved drawing from a session \
                     that didn't close normally.",
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Restore").clicked() {
                        app.restore_recovery(&path);
                        ui_state.recovery_offer = None;
                    }
                    if ui.button("Discard").clicked() {
                        crate::autosave::discard_recovery();
                        ui_state.recovery_offer = None;
                    }
                });
            });
    }
    {
        let mut fams: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for e in app.document.iter() {
            if let oxidraft_document::EntityKind::Text { font: Some(f), .. } = &e.kind {
                fams.insert(f.clone());
            }
        }
        if let Some(f) = &app.text_font {
            fams.insert(f.clone());
        }
        crate::fonts::ensure_fonts(&ctx, &fams);
    }
    handle_shortcuts(&ctx, app, ui_state);
    let canvas_rect = ui.max_rect();
    top_bar(&ctx, app, canvas_rect);
    ribbon(&ctx, app, canvas_rect);
    inspector(&ctx, app, canvas_rect);
    constraint_bar(&ctx, app, canvas_rect);
    status_pill(&ctx, app, canvas_rect);
    command_toast(&ctx, app, canvas_rect);
    contextual_toolbar(&ctx, app, canvas_rect);
    tool_hint_panel(&ctx, app, canvas_rect);
    about_window(&ctx, ui_state);
    line_props_dialog(&ctx, app, ui_state);
    settings_dialog(&ctx, app, ui_state);
    plot_dialog(&ctx, app);
    let cmd_bar_focused = command_bar(&ctx, app, ui_state, canvas_rect);
    canvas(ui, app, ui_state, cmd_bar_focused);
}

fn canvas(root_ui: &mut egui::Ui, app: &mut AppState, ui_state: &mut UiState, palette_open: bool) {
    let ctx_owned = root_ui.ctx().clone();
    let ctx = &ctx_owned;
    CentralPanel::default().show_inside(root_ui, |ui| {
        let avail = ui.available_size();
        app.view.width = avail.x as f64;
        app.view.height = avail.y as f64;
        app.sync_zoom_limits();
        let (rect, response) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        let origin = rect.min;
        let painter = ui.painter_at(rect);
        const GRIP_HIT: f32 = 7.0;
        if matches!(app.tool, Tool::Select)
            && app.interaction.corner_action.is_none()
            && !ctx.memory(|m| m.focused().is_some())
        {
            let hit = response.hover_pos().and_then(|pp| {
                app.selected_nurbs_all()
                    .into_iter()
                    .find_map(|(nid, ctrl, _w)| {
                        ctrl.iter()
                            .position(|p| {
                                let (sx, sy) = app.view.world_to_screen(p.x, p.y);
                                (egui::pos2(origin.x + sx as f32, origin.y + sy as f32) - pp)
                                    .length()
                                    <= GRIP_HIT
                            })
                            .map(|idx| (nid, idx))
                    })
            });
            if let Some((nid, idx)) = hit {
                let (inc, dec) = ui.input(|i| {
                    (
                        i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                        i.key_pressed(egui::Key::Minus),
                    )
                });
                if inc {
                    app.adjust_nurbs_weight(nid, idx, 1.25);
                }
                if dec {
                    app.adjust_nurbs_weight(nid, idx, 0.8);
                }
            }
        }

        let mut press_consumed = false;
        if ui_state.editing_text.is_none()
            && matches!(app.tool, Tool::Select)
            && response.double_clicked()
            && let Some(p) = response.interact_pointer_pos()
        {
            let (wx, wy) = app
                .view
                .screen_to_world((p.x - origin.x) as f64, (p.y - origin.y) as f64);
            if let Some(id) = oxidraft_cad::pick_at(
                &app.document,
                wx,
                wy,
                app.view.pixel_world_size() * app.pick_box * 0.5,
            ) && let Some(EntityKind::Text {
                content,
                font,
                height,
                ..
            }) = app.document.get(id).map(|e| &e.kind)
            {
                ui_state.editing_text = Some(id);
                ui_state.text_edit_buf = content.clone();
                ui_state.text_edit_font = font.clone();
                ui_state.text_edit_size = *height;
                ui_state.text_edit_active = false;
                app.selection = vec![id];
            }
        }
        if ui_state.editing_text.is_some() {
            press_consumed = true;
        }
        if let Some(p) = response.hover_pos() {
            app.pointer_moved((p.x - origin.x) as f64, (p.y - origin.y) as f64);
        }
        let corner_geoms: Vec<crate::state::CornerGeom> =
            if app.interaction.corner_action.is_none() && matches!(app.tool, Tool::Select) {
                app.detect_corners()
            } else {
                Vec::new()
            };
        const MIN_EDGE_PX: f32 = 16.0;
        let corner_dots: Vec<(crate::state::CornerGeom, egui::Pos2)> = corner_geoms
            .iter()
            .filter_map(|g| {
                let scr = |wx: f64, wy: f64| {
                    let (sx, sy) = app.view.world_to_screen(wx, wy);
                    pos2(origin.x + sx as f32, origin.y + sy as f32)
                };
                let c = scr(g.corner.0, g.corner.1);
                let va = scr(
                    g.corner.0 + g.dir_a.0 * g.len_a,
                    g.corner.1 + g.dir_a.1 * g.len_a,
                ) - c;
                let vb = scr(
                    g.corner.0 + g.dir_b.0 * g.len_b,
                    g.corner.1 + g.dir_b.1 * g.len_b,
                ) - c;
                let (la, lb) = (va.length(), vb.length());
                if la.min(lb) < MIN_EDGE_PX {
                    return None;
                }
                let a = va / la;
                let b = vb / lb;
                let mut bis = a + b;
                if bis.length() < 1e-3 {
                    bis = egui::vec2(-a.y, a.x);
                }
                let off = (0.32 * la.min(lb)).clamp(8.0, 30.0);
                Some((*g, c + bis.normalized() * off))
            })
            .collect();
        let corner_dots: Vec<(crate::state::CornerGeom, egui::Pos2)> = {
            const MIN_DOT_SEP: f32 = 30.0;
            let mut kept: Vec<(crate::state::CornerGeom, egui::Pos2)> = Vec::new();
            for (g, dp) in corner_dots {
                if kept.iter().all(|(_, k)| (dp - *k).length() >= MIN_DOT_SEP) {
                    kept.push((g, dp));
                }
            }
            kept
        };
        let hovered_dot: Option<usize> = response.hover_pos().and_then(|p| {
            corner_dots
                .iter()
                .position(|(_, dp)| (p - *dp).length() <= 9.0)
        });
        let over_dot = hovered_dot.is_some();

        let corner_busy = app.interaction.corner_action.is_some() || over_dot;
        if corner_busy {
            press_consumed = true;
        }
        if app.interaction.corner_action.is_some() {
            app.update_corner_drag();
            let typed: String = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect()
            });
            for ch in typed.chars() {
                if ch.is_ascii_digit() || ch == '.' {
                    ui_state.corner_input.push(ch);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Backspace)) {
                ui_state.corner_input.pop();
            }
            if let Ok(v) = ui_state.corner_input.parse::<f64>()
                && v > 0.0
            {
                app.set_corner_size(v);
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            if response.clicked() || enter {
                app.apply_corner_action();
                ui_state.corner_input.clear();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                app.cancel_corner_action();
                ui_state.corner_input.clear();
            }
        } else if response.clicked()
            && let Some(i) = hovered_dot
        {
            let (g, _) = corner_dots[i];
            app.begin_corner_action(g);
            ui_state.corner_input.clear();
        }
        overlays::dyn_line_hud(ctx, app, ui_state, origin);
        overlays::dyn_circle_hud(ctx, app, ui_state, origin);
        overlays::dyn_rect_hud(ctx, app, ui_state, origin);
        overlays::polygon_sides_hud(ctx, app, ui_state, origin);
        overlays::dyn_ellipse_hud(ctx, app, ui_state, origin);
        overlays::dyn_offset_hud(ctx, app, ui_state, origin);
        overlays::dyn_corner_hud(ctx, app, ui_state, origin);
        overlays::blend_confirm_hud(ctx, app, ui_state, origin);
        overlays::dyn_text_hud(ctx, app, ui_state, origin);
        overlays::dyn_transform_hud(ctx, app, ui_state, origin);
        overlays::cursor_readout(ctx, app, origin);
        let has_own_grips = !app.selection_grips().is_empty();
        let mut bbox_drag_active = false;
        if !press_consumed
            && !has_own_grips
            && matches!(app.tool, Tool::Select)
            && app.selection.len() == 1
            && app.interaction.corner_action.is_none()
            && let Some(e) = app.document.get(app.selection[0])
            && let Some(bbox) = e.bounding_box()
        {
            const HANDLE_RADIUS: f32 = 6.0;
            let (minx, miny) = bbox.min.to_f64();
            let (maxx, maxy) = bbox.max.to_f64();
            let p1 = app.view.world_to_screen(minx, miny);
            let p2 = app.view.world_to_screen(maxx, maxy);
            let screen_rect = egui::Rect::from_two_pos(
                pos2(origin.x + p1.0 as f32, origin.y + p1.1 as f32),
                pos2(origin.x + p2.0 as f32, origin.y + p2.1 as f32),
            );
            let cursor = response.hover_pos();
            let mut handle_under_cursor: Option<crate::state::BboxHandle> = None;
            if let Some(c) = cursor {
                let corners = [
                    (screen_rect.min, crate::state::BboxHandle::CornerNW),
                    (
                        pos2(screen_rect.max.x, screen_rect.min.y),
                        crate::state::BboxHandle::CornerNE,
                    ),
                    (
                        pos2(screen_rect.min.x, screen_rect.max.y),
                        crate::state::BboxHandle::CornerSW,
                    ),
                    (screen_rect.max, crate::state::BboxHandle::CornerSE),
                ];
                for (corner_pos, handle) in corners {
                    if (c - corner_pos).length() <= HANDLE_RADIUS {
                        handle_under_cursor = Some(handle);
                        break;
                    }
                }
                if handle_under_cursor.is_none() && screen_rect.contains(c) {
                    handle_under_cursor = Some(crate::state::BboxHandle::Body);
                }
            }
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(handle) = handle_under_cursor
            {
                let (cx, cy) = app.cursor_world;
                app.begin_bbox_drag(handle, (cx, cy));
                bbox_drag_active = true;
            }
            if response.dragged_by(egui::PointerButton::Primary)
                && app.interaction.bbox_drag.is_some()
            {
                let (cx, cy) = app.cursor_world;
                app.apply_bbox_drag_transform((cx, cy));
                bbox_drag_active = true;
            }
            if response.drag_stopped() {
                app.end_bbox_drag();
            }
        }
        if bbox_drag_active {
            press_consumed = true;
        }
        if ui.input(|i| i.pointer.primary_pressed()) {
            let in_select = matches!(app.tool, Tool::Select) && !app.grip_editing();
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("press_in_select"), in_select));
        }
        let press_in_select = ctx
            .data(|d| d.get_temp::<bool>(egui::Id::new("press_in_select")))
            .unwrap_or(true);
        let mut grip_consumed_click = false;
        let mut grip_consumed_enter = false;
        if app.grip_editing() {
            app.apply_grip_drag(app.cursor_world);
            if response.clicked() {
                app.end_grip_drag();
                ui_state.grip_input.clear();
                grip_consumed_click = true;
            }
        } else if !press_consumed
            && matches!(app.tool, Tool::Select)
            && press_in_select
            && app.interaction.corner_action.is_none()
            && response.clicked()
        {
            const GRIP_HIT: f32 = 8.0;
            let tan_hit = (app.selection.len() == 1)
                .then(|| app.selection[0])
                .and_then(|id| {
                    response.interact_pointer_pos().and_then(|c| {
                        app.tangent_markers(id).into_iter().find_map(|(i, tp)| {
                            let (sx, sy) = app.view.world_to_screen(tp.x, tp.y);
                            let gp = pos2(origin.x + sx as f32, origin.y + sy as f32);
                            ((gp - c).length() <= 9.0).then_some((id, i))
                        })
                    })
                });
            if let Some((id, which)) = tan_hit {
                app.remove_tangent(id, which);
                grip_consumed_click = true;
            } else {
                let grips = app.selection_grips();
                let hit = response.interact_pointer_pos().and_then(|c| {
                    grips.iter().position(|(_, g)| {
                        let (sx, sy) = app.view.world_to_screen(g.world.x, g.world.y);
                        let gp = pos2(origin.x + sx as f32, origin.y + sy as f32);
                        (gp - c).length() <= GRIP_HIT
                    })
                });
                if let Some(idx) = hit {
                    let (eid, grip) = grips[idx];
                    app.begin_grip_drag(eid, grip);
                    ui_state.grip_input.clear();
                    grip_consumed_click = true;
                }
            }
        }
        if app.grip_editing() {
            let mut commit_value: Option<f64> = None;
            ui.input(|i| {
                for ev in &i.events {
                    match ev {
                        egui::Event::Text(t) => {
                            for ch in t.chars() {
                                if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                                    ui_state.grip_input.push(ch);
                                }
                            }
                        }
                        egui::Event::Key {
                            key: egui::Key::Backspace,
                            pressed: true,
                            ..
                        } => {
                            ui_state.grip_input.pop();
                        }
                        egui::Event::Key {
                            key: egui::Key::Enter,
                            pressed: true,
                            ..
                        } => {
                            if let Ok(v) = ui_state.grip_input.trim().parse::<f64>() {
                                commit_value = Some(v);
                            }
                            grip_consumed_enter = true;
                        }
                        _ => {}
                    }
                }
            });
            if let Some(v) = commit_value {
                app.commit_grip_value(v);
                ui_state.grip_input.clear();
            }
        } else {
            ui_state.grip_input.clear();
        }
        if app.grip_editing() || grip_consumed_click {
            press_consumed = true;
        }
        if !press_consumed && matches!(app.tool, Tool::Select) {
            if response.drag_started_by(egui::PointerButton::Primary)
                && let Some(p) = response.interact_pointer_pos()
            {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new("marquee_start"), p);
                    d.insert_temp(egui::Id::new("marquee_on"), true);
                });
            }
            if response.drag_stopped()
                && ctx.data(|d| {
                    d.get_temp::<bool>(egui::Id::new("marquee_on"))
                        .unwrap_or(false)
                })
            {
                let start: Option<egui::Pos2> =
                    ctx.data(|d| d.get_temp(egui::Id::new("marquee_start")));
                let end = response
                    .interact_pointer_pos()
                    .or_else(|| response.hover_pos());
                if let (Some(s), Some(e)) = (start, end)
                    && (e - s).length() > 3.0
                {
                    let (x0, y0) = app
                        .view
                        .screen_to_world((s.x - origin.x) as f64, (s.y - origin.y) as f64);
                    let (x1, y1) = app
                        .view
                        .screen_to_world((e.x - origin.x) as f64, (e.y - origin.y) as f64);
                    let rect = oxidraft_geometry::BoundingBox::from_corners(x0, y0, x1, y1);
                    let sel = if e.x < s.x {
                        oxidraft_cad::select_crossing(&app.document, &rect)
                    } else {
                        oxidraft_cad::select_window(&app.document, &rect)
                    };
                    app.selection = sel
                        .into_iter()
                        .filter(|&id| id != app.origin_id)
                        .filter(|&id| {
                            app.document
                                .get(id)
                                .map(|e| layer_visible(app, e))
                                .unwrap_or(false)
                        })
                        .collect();
                }
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("marquee_on"), false));
            }
        }
        let place_point = !press_consumed
            && !palette_open
            && if matches!(app.tool, Tool::Select) {
                response.clicked() && press_in_select
            } else {
                response.contains_pointer() && ui.input(|i| i.pointer.primary_pressed())
            };
        if place_point
            && let Some(p) = response
                .interact_pointer_pos()
                .or_else(|| response.hover_pos())
        {
            app.canvas_click((p.x - origin.x) as f64, (p.y - origin.y) as f64);
        }
        if !palette_open
            && ui_state.editing_text.is_none()
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            if app.grip_editing() {
                app.cancel_grip_drag();
            } else {
                app.execute(Command::Cancel);
            }
        }
        let in_text_field = {
            let f = ctx.memory(|mem| mem.focused());
            f == Some(egui::Id::new("command_line_input"))
                || f == Some(egui::Id::new("dyn_len"))
                || f == Some(egui::Id::new("dyn_ang"))
                || f == Some(egui::Id::new("dyn_radius"))
                || f == Some(egui::Id::new("dyn_rect_field"))
                || f == Some(egui::Id::new("dyn_poly_sides"))
                || f == Some(egui::Id::new("dyn_ell_major"))
                || f == Some(egui::Id::new("dyn_ell_minor"))
                || f == Some(egui::Id::new("dyn_offset_dist"))
                || f == Some(egui::Id::new("dyn_corner_val"))
                || f == Some(egui::Id::new("dyn_text_field"))
                || f == Some(egui::Id::new("dyn_tf_dx"))
                || f == Some(egui::Id::new("dyn_tf_dy"))
                || f == Some(egui::Id::new("dyn_tf_angle"))
                || f == Some(egui::Id::new("dyn_tf_factor"))
                || f == Some(egui::Id::new("palette_input"))
        };
        if !palette_open
            && !grip_consumed_enter
            && !app.grip_editing()
            && ui_state.editing_text.is_none()
            && matches!(app.tool, Tool::Polyline { .. } | Tool::Spline { .. })
            && ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Space))
            && !in_text_field
        {
            app.run_command("");
        }
        let hovered_id = if matches!(app.tool, Tool::Select) || app.tool.picks_entities() {
            response
                .hover_pos()
                .and_then(|p| {
                    let (wx, wy) = app
                        .view
                        .screen_to_world((p.x - origin.x) as f64, (p.y - origin.y) as f64);
                    oxidraft_cad::pick_at(
                        &app.document,
                        wx,
                        wy,
                        app.view.pixel_world_size() * app.pick_box * 0.5,
                    )
                })
                .filter(|&id| id != app.origin_id)
                .filter(|&id| {
                    app.document
                        .get(id)
                        .map(|e| layer_visible(app, e))
                        .unwrap_or(false)
                })
        } else {
            None
        };
        if matches!(app.tool, Tool::Select) {
            if response.secondary_clicked()
                && app.selection.is_empty()
                && let Some(h) = hovered_id
            {
                app.selection = vec![h];
            }
            response.context_menu(|ui| {
                if !app.selection.is_empty() {
                    if ui.button("Cut").clicked() {
                        app.clipboard_cut();
                        ui.close();
                    }
                    if ui.button("Copy").clicked() {
                        app.clipboard_copy();
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Delete").clicked() {
                        app.erase_selection();
                        ui.close();
                    }
                    if ui.button("Disjoint").clicked() {
                        app.explode_selection();
                        ui.close();
                    }
                    if ui.button("Join").clicked() {
                        app.join_selection();
                        ui.close();
                    }
                    let has_text = app.selection.iter().any(|&id| {
                        matches!(
                            app.document.get(id).map(|e| &e.kind),
                            Some(EntityKind::Text { .. })
                        )
                    });
                    if has_text
                        && ui
                            .button("Create Outlines")
                            .on_hover_text("Convert text to editable vector geometry")
                            .clicked()
                    {
                        app.outline_text_selection();
                        ui.close();
                    }
                    if ui.button("Line Weight & Type…").clicked() {
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(egui::Id::new("open_line_props"), true));
                        ui.close();
                    }
                    ui.separator();
                    ui.menu_button("Modify", |ui| {
                        let acts: [(&str, Command); 12] = [
                            (
                                "Move",
                                Command::Activate(Tool::Move {
                                    base: None,
                                    ids: vec![],
                                }),
                            ),
                            (
                                "Duplicate",
                                Command::Activate(Tool::Copy {
                                    base: None,
                                    ids: vec![],
                                }),
                            ),
                            (
                                "Rotate",
                                Command::Activate(Tool::Rotate {
                                    base: None,
                                    ids: vec![],
                                }),
                            ),
                            (
                                "Scale",
                                Command::Activate(Tool::Scale {
                                    base: None,
                                    reference: None,
                                    ids: vec![],
                                }),
                            ),
                            (
                                "Mirror",
                                Command::Activate(Tool::Mirror {
                                    first: None,
                                    ids: vec![],
                                }),
                            ),
                            (
                                "Offset",
                                Command::Activate(Tool::Offset {
                                    dist: 1.0,
                                    source: None,
                                }),
                            ),
                            ("Trim", Command::Activate(Tool::Trim)),
                            ("Extend", Command::Activate(Tool::Extend)),
                            (
                                "Fillet",
                                Command::Activate(Tool::Fillet {
                                    radius: 1.0,
                                    first: None,
                                }),
                            ),
                            (
                                "Chamfer",
                                Command::Activate(Tool::Chamfer {
                                    dist: 1.0,
                                    first: None,
                                }),
                            ),
                            (
                                "Blend",
                                Command::Activate(Tool::Blend {
                                    continuity: oxidraft_geometry::Continuity::G1,
                                    tension: 1.0,
                                    first: None,
                                    second: None,
                                }),
                            ),
                            (
                                "Stretch",
                                Command::Activate(Tool::Stretch {
                                    c1: None,
                                    c2: None,
                                    base: None,
                                    ids: vec![],
                                }),
                            ),
                        ];
                        for (label, cmd) in acts {
                            if ui.button(label).clicked() {
                                app.execute(cmd);
                                ui.close();
                            }
                        }
                    });
                    ui.separator();
                }
                if ui
                    .add_enabled(!app.clipboard.is_empty(), egui::Button::new("Paste"))
                    .clicked()
                {
                    app.clipboard_paste();
                    ui.close();
                }
                if let Some(last) = app.last_command.clone()
                    && ui.button(format!("Repeat: {last}")).clicked()
                {
                    app.repeat_last_command();
                    ui.close();
                }
                if ui.button("Select All").clicked() {
                    app.execute(Command::SelectAll);
                    ui.close();
                }
                if ui.button("Zoom Extents").clicked() {
                    app.zoom_extents();
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut app.grid_on, "Grid");
                ui.checkbox(&mut app.snap_on, "Object Snap");
            });
        } else if response.secondary_clicked() {
            app.run_command("");
        }
        let focused_id = ctx.memory(|mem| mem.focused());
        if ui.input(|i| i.key_pressed(egui::Key::F7)) {
            app.snap_on = !app.snap_on;
        }
        if ui.input(|i| i.key_pressed(egui::Key::F8)) {
            app.grid_on = !app.grid_on;
        }
        if ui.input(|i| i.key_pressed(egui::Key::F9)) {
            app.grid_snap_on = !app.grid_snap_on;
        }
        if ui.input(|i| i.key_pressed(egui::Key::F10)) {
            app.polar_on = !app.polar_on;
            if app.polar_on {
                app.ortho_on = false;
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::F11)) {
            app.track_on = !app.track_on;
        }
        if ui.input(|i| i.key_pressed(egui::Key::F12)) {
            app.dyn_on = !app.dyn_on;
        }
        if focused_id.is_none() && !palette_open && ui_state.editing_text.is_none() {
            let mods = ui.input(|i| i.modifiers);
            let plain = !mods.ctrl && !mods.alt && !mods.command && !mods.shift;
            let shift_only = mods.shift && !mods.ctrl && !mods.alt && !mods.command;
            use egui::Key;
            if plain {
                const DRAW: &[(Key, &str)] = &[
                    (Key::L, "LINE"),
                    (Key::P, "POLYLINE"),
                    (Key::C, "CIRCLE"),
                    (Key::E, "ELLIPSE"),
                    (Key::A, "ARC"),
                    (Key::R, "RECTANGLE"),
                    (Key::G, "POLYGON"),
                    (Key::S, "SPLINE"),
                    (Key::T, "TEXT"),
                    (Key::H, "HATCH"),
                ];
                for (k, cmd) in DRAW {
                    if ui.input(|i| i.key_pressed(*k)) {
                        app.run_command(cmd);
                    }
                }
                if ui.input(|i| i.key_pressed(Key::Z)) {
                    app.zoom_extents();
                }
                if ui.input(|i| i.key_pressed(Key::Space)) {
                    app.repeat_last_command();
                }
            } else if shift_only {
                const MODIFY: &[(Key, &str)] = &[
                    (Key::M, "MOVE"),
                    (Key::C, "COPY"),
                    (Key::R, "ROTATE"),
                    (Key::S, "STRETCH"),
                    (Key::A, "SCALE"),
                    (Key::I, "MIRROR"),
                    (Key::O, "OFFSET"),
                    (Key::T, "TRIM"),
                    (Key::E, "EXTEND"),
                    (Key::F, "FILLET"),
                    (Key::H, "CHAMFER"),
                    (Key::B, "BLEND"),
                    (Key::X, "DISJOINT"),
                    (Key::J, "JOIN"),
                ];
                for (k, cmd) in MODIFY {
                    if ui.input(|i| i.key_pressed(*k)) {
                        app.run_command(cmd);
                    }
                }
            }
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let d = response.drag_delta();
            app.view.pan_pixels(d.x as f64, d.y as f64);
            app.zoom_target = None;
        }
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let signed = if app.invert_zoom { -scroll } else { scroll };
                let factor = (signed as f64 / 200.0 * app.zoom_speed).exp();
                let (wx, wy) = if app.zoom_to_cursor {
                    app.cursor_world
                } else {
                    app.view.center
                };
                app.view.zoom_at(wx, wy, factor);
                app.zoom_target = None;
            }
        }
        if !has_own_grips
            && matches!(app.tool, Tool::Select)
            && app.selection.len() == 1
            && let Some(e) = app.document.get(app.selection[0])
            && let Some(bbox) = e.bounding_box()
        {
            let bbox_stroke = Stroke::new(1.0, Color32::from_rgb(0, 200, 255));
            let (minx, miny) = bbox.min.to_f64();
            let (maxx, maxy) = bbox.max.to_f64();
            let p1 = app.view.world_to_screen(minx, miny);
            let p2 = app.view.world_to_screen(maxx, maxy);
            let screen_rect = egui::Rect::from_two_pos(
                pos2(origin.x + p1.0 as f32, origin.y + p1.1 as f32),
                pos2(origin.x + p2.0 as f32, origin.y + p2.1 as f32),
            );
            painter.rect_stroke(screen_rect, 0.0, bbox_stroke, egui::StrokeKind::Middle);
            const HANDLE_SIZE: f32 = 4.0;
            let corners = vec![
                (screen_rect.min.x, screen_rect.min.y, "NW"),
                (screen_rect.max.x, screen_rect.min.y, "NE"),
                (screen_rect.min.x, screen_rect.max.y, "SW"),
                (screen_rect.max.x, screen_rect.max.y, "SE"),
            ];
            for (x, y, _label) in corners {
                let handle_rect = egui::Rect::from_center_size(
                    pos2(x, y),
                    vec2(HANDLE_SIZE * 2.0, HANDLE_SIZE * 2.0),
                );
                painter.rect_filled(handle_rect, 0.0, Color32::from_rgb(0, 200, 255));
            }
        }
        let to_screen = |wx: f64, wy: f64| render::world_to_screen_pos(app, origin, wx, wy);
        painter.rect_filled(rect, 0.0, crate::theme::CANVAS_BG);
        if app.grid_on {
            draw_grid(&painter, app, rect, &to_screen);
        }
        refresh_hatch_cache(app, &mut ui_state.hatch_cache);
        refresh_text_cache(app, &mut ui_state.text_cache);
        refresh_curve_cache(app, &mut ui_state.curve_cache);
        let selected_set: std::collections::HashSet<EntityId> =
            app.selection.iter().copied().collect();
        let (vx0, vy0, vx1, vy1) = app.view.visible_bounds();
        let cull_pad = 32.0 * app.view.pixel_world_size();
        let view_bb = oxidraft_geometry::BoundingBox::from_corners(
            vx0 - cull_pad,
            vy0 - cull_pad,
            vx1 + cull_pad,
            vy1 + cull_pad,
        );
        for e in app.document.iter() {
            if e.id != app.origin_id && !layer_visible(app, e) {
                continue;
            }
            if e.id != app.origin_id
                && let Some(bb) = e.bounding_box()
                && !bb.intersects(&view_bb)
            {
                continue;
            }
            let (r, g, b) = resolve_color(app, e);
            let selected = selected_set.contains(&e.id);
            let hovered = !selected
                && Some(e.id) == hovered_id
                && !matches!(app.tool, Tool::Trim | Tool::Extend);
            let is_hatch = matches!(e.kind, EntityKind::Hatch { .. });
            let color = if selected {
                if is_hatch {
                    HATCH_SELECT
                } else {
                    Color32::from_rgb(0, 200, 255)
                }
            } else if hovered {
                if app.tool.picks_entities() {
                    crate::theme::SNAP
                } else {
                    Color32::from_rgb(120, 230, 255)
                }
            } else {
                Color32::from_rgb(r, g, b)
            };
            let base = resolve_line_weight_px(app, e);
            let width = if selected {
                base + 1.0
            } else if hovered {
                base + 0.5
            } else {
                base
            };
            let (hatch_tris, hatch_loops) = if matches!(e.kind, EntityKind::Hatch { .. }) {
                match ui_state.hatch_cache.get(&e.id) {
                    Some((_, t, l)) => (Some(t.as_slice()), Some(l.as_slice())),
                    None => (None, None),
                }
            } else {
                (None, None)
            };
            let text_tris = if matches!(e.kind, EntityKind::Text { .. }) {
                ui_state.text_cache.get(&e.id).map(|(_, t)| t.as_slice())
            } else {
                None
            };
            let curve_pts = if matches!(e.kind, EntityKind::Curve(_)) {
                ui_state
                    .curve_cache
                    .get(&e.id)
                    .map(|(_, pts, closed)| (pts.as_slice(), *closed))
            } else {
                None
            };
            draw_entity(
                &painter,
                app,
                e,
                origin,
                Stroke::new(width, color),
                selected,
                hatch_tris,
                hatch_loops,
                text_tris,
                curve_pts,
            );
        }
        if app.comb_on {
            for &id in &app.selection {
                if let Some(c) = app.document.get(id).and_then(|e| e.as_curve()) {
                    if c.as_line().is_some() {
                        continue;
                    }
                    overlays::curvature_comb(&painter, app, c, origin, app.comb_scale, 48);
                }
            }
        }
        // Badges only delete on a Select-mode click, so only hint then (and
        // not mid grip-drag / corner-action).
        let badge_hover = (matches!(app.tool, Tool::Select)
            && app.interaction.corner_action.is_none()
            && app.interaction.grip_drag.is_none())
        .then(|| response.hover_pos())
        .flatten();
        overlays::constraint_badges(&painter, app, origin, badge_hover);
        if matches!(app.tool, Tool::Select) && app.interaction.corner_action.is_none() {
            let guide = Stroke::new(1.0, crate::theme::CONTROL_LINE);
            for (_, ctrl, _weights) in app.selected_nurbs_all() {
                let pts: Vec<egui::Pos2> = ctrl.iter().map(|p| to_screen(p.x, p.y)).collect();
                for w in pts.windows(2) {
                    draw_dashed_line(&painter, w[0], w[1], guide, 5.0, 4.0);
                }
            }
        }
        {
            let hover = response.hover_pos();
            let near = |g: egui::Pos2| hover.map(|h| (h - g).length() <= 8.0).unwrap_or(false);
            for (_, grip) in app.selection_grips() {
                let p = to_screen(grip.world.x, grip.world.y);
                let hot = near(p);
                let col = if hot {
                    Color32::from_rgb(255, 220, 120)
                } else if grip.role == GripRole::Center {
                    Color32::from_rgb(100, 150, 255)
                } else {
                    crate::theme::SNAP
                };
                if grip.role == GripRole::Center {
                    painter.circle_filled(p, 5.0, crate::theme::CANVAS_BG);
                    painter.circle_filled(p, 4.0, col);
                } else if grip.role == GripRole::Rotation {
                    painter.circle_filled(p, 11.0, crate::theme::CANVAS_BG);
                    crate::icons::paint_icon(
                        &painter,
                        ctx,
                        crate::icons::Icon::Rotate,
                        egui::Rect::from_center_size(p, vec2(18.0, 18.0)),
                        col,
                    );
                } else {
                    painter.rect_filled(
                        egui::Rect::from_center_size(p, vec2(10.0, 10.0)),
                        2.0,
                        crate::theme::CANVAS_BG,
                    );
                    painter.rect_filled(egui::Rect::from_center_size(p, vec2(8.0, 8.0)), 2.0, col);
                }
            }
        }
        if app.selection.len() == 1 {
            let id = app.selection[0];
            let hoverp = response.hover_pos();
            for (_, tp) in app.tangent_markers(id) {
                let p = to_screen(tp.x, tp.y);
                let hot = hoverp.map(|h| (h - p).length() <= 9.0).unwrap_or(false);
                let col = if hot {
                    crate::theme::SNAP
                } else {
                    crate::theme::ACCENT_BRIGHT
                };
                painter.circle_filled(p, 8.0, Color32::from_rgba_unmultiplied(20, 26, 36, 235));
                painter.circle_stroke(p, 8.0, Stroke::new(1.0, crate::theme::OUTLINE));
                painter.circle_stroke(p, 3.2, Stroke::new(1.4, col));
                painter.line_segment(
                    [p + vec2(-5.0, 5.2), p + vec2(5.0, 5.2)],
                    Stroke::new(1.4, col),
                );
                if hot {
                    painter.text(
                        p + vec2(0.0, -12.0),
                        egui::Align2::CENTER_BOTTOM,
                        "click to remove tangency",
                        egui::FontId::proportional(11.0),
                        Color32::from_rgb(255, 200, 120),
                    );
                }
            }
        }
        if let Some(role) = app.grip_role() {
            let label = oxidraft_cad::grip_value_label(role);
            let shown = if ui_state.grip_input.is_empty() {
                "_"
            } else {
                ui_state.grip_input.as_str()
            };
            let txt = format!("{label}: {shown}");
            let cp = to_screen(app.cursor_world.0, app.cursor_world.1);
            let tp = pos2(cp.x + 14.0, cp.y - 28.0);
            let galley =
                painter.layout_no_wrap(txt, egui::FontId::proportional(13.0), Color32::WHITE);
            let bg = egui::Rect::from_min_size(tp, galley.size()).expand(5.0);
            painter.rect_filled(bg, 5.0, Color32::from_rgba_unmultiplied(26, 32, 42, 235));
            painter.rect_stroke(
                bg,
                5.0,
                Stroke::new(1.0, Color32::from_rgb(255, 220, 120)),
                egui::StrokeKind::Middle,
            );
            painter.galley(tp, galley, Color32::WHITE);
        }
        let mut text_commit: Option<(EntityId, String, Option<String>, f64)> = None;
        if let Some(id) = ui_state.editing_text {
            let anchor = app.document.get(id).and_then(|e| match &e.kind {
                EntityKind::Text { anchor, .. } => Some(anchor.to_f64()),
                _ => None,
            });
            match anchor {
                None => {
                    ui_state.editing_text = None;
                }
                Some((ax, ay)) => {
                    let sp = to_screen(ax, ay);
                    let first_show = !ui_state.text_edit_active;
                    let mut commit = false;
                    let mut cancel = false;
                    let area = egui::Area::new(egui::Id::new("text_edit_inline"))
                        .fixed_pos(pos2(sp.x, sp.y - 52.0))
                        .order(egui::Order::Foreground)
                        .show(ctx, |ui| {
                            egui::Frame::popup(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    chrome::font_combo(
                                        ui,
                                        "inline_font",
                                        &mut ui_state.text_edit_font,
                                    );
                                    ui.add(
                                        egui::DragValue::new(&mut ui_state.text_edit_size)
                                            .speed(0.05)
                                            .range(0.1..=1e6)
                                            .prefix("H "),
                                    );
                                });
                                let te = ui.add(
                                    egui::TextEdit::singleline(&mut ui_state.text_edit_buf)
                                        .id(egui::Id::new("text_edit_field"))
                                        .desired_width(220.0),
                                );
                                if first_show {
                                    te.request_focus();
                                }
                                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                if esc {
                                    cancel = true;
                                } else if enter {
                                    commit = true;
                                }
                            });
                        });
                    if area.response.clicked_elsewhere() {
                        commit = true;
                    }
                    ui_state.text_edit_active = true;
                    if commit {
                        text_commit = Some((
                            id,
                            ui_state.text_edit_buf.clone(),
                            ui_state.text_edit_font.clone(),
                            ui_state.text_edit_size,
                        ));
                        ui_state.editing_text = None;
                        ui_state.text_edit_active = false;
                    } else if cancel {
                        ui_state.editing_text = None;
                        ui_state.text_edit_active = false;
                    }
                }
            }
        }
        if let Some(ca) = app.interaction.corner_action {
            draw_corner_preview(&painter, app, &ca, &to_screen);
        } else {
            for (i, (g, dp)) in corner_dots.iter().enumerate() {
                let hovered = hovered_dot == Some(i);
                let h = ctx.animate_bool(egui::Id::new("corner_dot").with(i), hovered);
                let r = 5.0 + 2.0 * h;
                painter.circle_filled(*dp, r, Color32::from_rgb(0, 150, 255));
                painter.circle_stroke(*dp, r, Stroke::new(1.5, Color32::from_rgb(190, 225, 255)));
                if hovered {
                    let chamfer = g.chamfer_ok;
                    let txt = if chamfer {
                        "Chamfer      Fillet"
                    } else {
                        "Fillet — drag to size"
                    };
                    let galley = painter.layout_no_wrap(
                        txt.to_string(),
                        egui::FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    let isz = 14.0;
                    let pad = if chamfer { isz + 6.0 } else { 0.0 };
                    let tp = pos2(dp.x + 12.0, dp.y - 22.0);
                    let bg = egui::Rect::from_min_size(
                        tp,
                        vec2(pad * 2.0 + galley.size().x, galley.size().y),
                    )
                    .expand(5.0);
                    painter.rect_filled(bg, 6.0, Color32::from_rgba_unmultiplied(26, 32, 42, 235));
                    painter.rect_stroke(
                        bg,
                        6.0,
                        Stroke::new(1.0, Color32::from_rgb(0, 200, 255)),
                        egui::StrokeKind::Middle,
                    );
                    painter.galley(pos2(tp.x + pad, tp.y), galley, Color32::WHITE);
                    if chamfer {
                        let cy = bg.center().y;
                        let left = egui::Rect::from_center_size(
                            pos2(bg.left() + 6.0 + isz * 0.5, cy),
                            vec2(isz, isz),
                        );
                        let right = egui::Rect::from_center_size(
                            pos2(bg.right() - 6.0 - isz * 0.5, cy),
                            vec2(isz, isz),
                        );
                        crate::icons::paint_icon(
                            &painter,
                            ctx,
                            crate::icons::Icon::Undo,
                            left,
                            Color32::WHITE,
                        );
                        crate::icons::paint_icon(
                            &painter,
                            ctx,
                            crate::icons::Icon::Redo,
                            right,
                            Color32::WHITE,
                        );
                    }
                }
            }
        }
        if !app.grip_editing()
            && ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("marquee_on"))
                    .unwrap_or(false)
            })
            && let (Some(start), Some(cur)) = (
                ctx.data(|d| d.get_temp::<egui::Pos2>(egui::Id::new("marquee_start"))),
                response
                    .hover_pos()
                    .or_else(|| response.interact_pointer_pos()),
            )
        {
            let crossing = cur.x < start.x;
            let rect = egui::Rect::from_two_pos(start, cur);
            let (fill, line) = if crossing {
                (
                    Color32::from_rgba_unmultiplied(0, 200, 90, 32),
                    Color32::from_rgb(0, 220, 110),
                )
            } else {
                (
                    Color32::from_rgba_unmultiplied(0, 150, 240, 32),
                    Color32::from_rgb(0, 180, 255),
                )
            };
            painter.rect_filled(rect, 0.0, fill);
            if crossing {
                let c = [
                    rect.left_top(),
                    rect.right_top(),
                    rect.right_bottom(),
                    rect.left_bottom(),
                ];
                let st = Stroke::new(1.0, line);
                for i in 0..4 {
                    draw_dashed_line(&painter, c[i], c[(i + 1) % 4], st, 6.0, 4.0);
                }
            } else {
                painter.rect_stroke(rect, 0.0, Stroke::new(1.0, line), egui::StrokeKind::Middle);
            }
        }
        if !app.interaction.active_guides.is_empty() {
            let stroke = Stroke::new(1.0, crate::theme::SNAP);
            let overshoot = app.view.pixel_world_size() * 28.0;
            let (cwx, cwy) = app.cursor_world;
            for g in &app.interaction.active_guides {
                let along = (cwx - g.origin.0) * g.dir.0 + (cwy - g.origin.1) * g.dir.1;
                let sgn = if along >= 0.0 { 1.0 } else { -1.0 };
                let end = (
                    cwx + g.dir.0 * overshoot * sgn,
                    cwy + g.dir.1 * overshoot * sgn,
                );
                let a = to_screen(g.origin.0, g.origin.1);
                let b = to_screen(end.0, end.1);
                draw_dashed_line(&painter, a, b, stroke, 6.0, 6.0);
            }
            let mut labels: Vec<&str> = app
                .interaction
                .active_guides
                .iter()
                .map(|g| g.kind.label())
                .collect();
            labels.dedup();
            let cc = to_screen(app.cursor_world.0, app.cursor_world.1);
            painter.text(
                cc + vec2(15.0, -15.0),
                egui::Align2::LEFT_BOTTOM,
                labels.join(" + "),
                egui::FontId::proportional(11.0),
                crate::theme::SNAP,
            );
        }
        let cursor = Point2d::from_f64(app.cursor_world.0, app.cursor_world.1);
        let preview_stroke = Stroke::new(1.5, crate::theme::PREVIEW);
        match &app.tool {
            Tool::Spline { .. } => {
                let guide = Stroke::new(1.0, crate::theme::CONTROL_LINE);
                for c in app.tool.preview(&cursor) {
                    match &c {
                        oxidraft_geometry::Curve::Line(l) => draw_dashed_line(
                            &painter,
                            to_screen(l.p0.x, l.p0.y),
                            to_screen(l.p1.x, l.p1.y),
                            guide,
                            5.0,
                            4.0,
                        ),
                        other => draw_curve(&painter, other, &to_screen, preview_stroke),
                    }
                }
            }
            Tool::Rectangle { first: Some(_) }
            | Tool::Polygon {
                center: Some(_),
                sides: Some(_),
                ..
            } => {
                let pts: Vec<egui::Pos2> = app
                    .tool
                    .preview(&cursor)
                    .iter()
                    .filter_map(|c| match c {
                        oxidraft_geometry::Curve::Line(l) => Some(to_screen(l.p0.x, l.p0.y)),
                        _ => None,
                    })
                    .collect();
                if pts.len() >= 2 {
                    painter.add(egui::Shape::closed_line(pts, preview_stroke));
                }
            }
            _ => {
                for c in app.tool.preview(&cursor) {
                    draw_curve(&painter, &c, &to_screen, preview_stroke);
                }
            }
        }
        {
            let dim_col = app
                .document
                .layers
                .index_of(oxidraft_document::DIMENSION_LAYER)
                .and_then(|i| app.document.layers.get(i))
                .map(|l| Color32::from_rgb(l.color.0, l.color.1, l.color.2))
                .unwrap_or(Color32::from_rgb(46, 204, 113));
            match &app.tool {
                Tool::Dimension {
                    p1: Some(a),
                    p2: Some(b),
                } => match oxidraft_document::linear_orientation(*a, *b, cursor) {
                    None => {
                        draw_dimension(&painter, app, *a, *b, cursor, None, &to_screen, dim_col)
                    }
                    Some(vertical) => render::draw_ortho_dim(
                        &painter, app, *a, *b, cursor, vertical, None, &to_screen, dim_col,
                    ),
                },
                Tool::DimAngularLines {
                    geom: Some((v, a, b)),
                    ..
                } => {
                    render::draw_angular_dim(
                        &painter, app, *v, *a, *b, cursor, None, &to_screen, dim_col,
                    );
                }
                Tool::DimRadial {
                    diameter,
                    center: Some(c),
                    radius,
                } if *radius > 1e-9 => {
                    let (cx, cy) = c.to_f64();
                    let (dx, dy) = (cursor.x - cx, cursor.y - cy);
                    let len = (dx * dx + dy * dy).sqrt();
                    let edge = if len > 1e-9 {
                        Point2d::from_f64(cx + dx / len * *radius, cy + dy / len * *radius)
                    } else {
                        Point2d::from_f64(cx + *radius, cy)
                    };
                    render::draw_radial_dim(
                        &painter, app, *c, edge, *diameter, None, &to_screen, dim_col,
                    );
                }
                _ => {}
            }
        }
        draw_transform_ghost(&painter, app, &to_screen);
        draw_blend_preview(&painter, app, &to_screen, hovered_id);
        draw_trim_extend_preview(&painter, app, &to_screen);
        let cc = to_screen(app.cursor_world.0, app.cursor_world.1);
        let over_canvas = response.contains_pointer()
            && ctx
                .pointer_latest_pos()
                .map(|p| ctx.layer_id_at(p) == Some(response.layer_id))
                .unwrap_or(false);
        let panning = response.dragged_by(egui::PointerButton::Middle);
        if panning {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            crate::icons::paint_icon(
                &painter,
                ui.ctx(),
                crate::icons::Icon::Pan,
                egui::Rect::from_center_size(cc, vec2(26.0, 26.0)),
                Color32::from_rgb(232, 238, 248),
            );
        } else if over_canvas {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            let cross = Stroke::new(1.0, Color32::from_rgb(140, 150, 170));
            if app.crosshair {
                painter.line_segment([pos2(rect.left(), cc.y), pos2(rect.right(), cc.y)], cross);
                painter.line_segment([pos2(cc.x, rect.top()), pos2(cc.x, rect.bottom())], cross);
            } else {
                let (gap, arm) = (app.pick_box as f32 * 0.6, app.pick_box as f32 * 1.6);
                painter.line_segment([cc - vec2(arm, 0.0), cc - vec2(gap, 0.0)], cross);
                painter.line_segment([cc + vec2(gap, 0.0), cc + vec2(arm, 0.0)], cross);
                painter.line_segment([cc - vec2(0.0, arm), cc - vec2(0.0, gap)], cross);
                painter.line_segment([cc + vec2(0.0, gap), cc + vec2(0.0, arm)], cross);
            }
            if let Some(((rx, ry), angle_rad)) = app.interaction.active_guide {
                let view_diag =
                    (app.view.width * app.view.width + app.view.height * app.view.height).sqrt();
                let world_length = view_diag * app.view.pixel_world_size() * 2.0;
                let p_start = to_screen(rx, ry);
                let p_end = to_screen(
                    rx + world_length * angle_rad.cos(),
                    ry + world_length * angle_rad.sin(),
                );
                let guide_stroke = Stroke::new(1.5, crate::theme::SNAP);
                draw_dashed_line(&painter, p_start, p_end, guide_stroke, 6.0, 6.0);
            }
            if matches!(app.tool, Tool::Select) || app.tool.picks_entities() {
                let box_stroke = if matches!(app.tool, Tool::Select) {
                    cross
                } else {
                    Stroke::new(1.4, crate::theme::SNAP)
                };
                painter.rect_stroke(
                    egui::Rect::from_center_size(
                        cc,
                        vec2(app.pick_box as f32, app.pick_box as f32),
                    ),
                    0.0,
                    box_stroke,
                    egui::StrokeKind::Middle,
                );
            }
        }
        if let Some(sp) = &app.active_snap {
            let c = to_screen(sp.pos.0, sp.pos.1);
            const R: f32 = 8.0;
            let snap_col = crate::theme::SNAP;
            let stroke = Stroke::new(2.2, snap_col);
            let no_stroke = Stroke::NONE;

            match sp.kind {
                oxidraft_cad::SnapKind::Endpoint => {
                    painter.rect_filled(
                        egui::Rect::from_center_size(c, vec2(R * 2.0, R * 2.0)),
                        0.0,
                        snap_col,
                    );
                }
                oxidraft_cad::SnapKind::Midpoint => {
                    let top = pos2(c.x, c.y - R);
                    let left = pos2(c.x - R, c.y + R);
                    let right = pos2(c.x + R, c.y + R);
                    painter.add(egui::Shape::convex_polygon(
                        vec![top, left, right],
                        snap_col,
                        no_stroke,
                    ));
                }
                oxidraft_cad::SnapKind::Center | oxidraft_cad::SnapKind::Node => {
                    painter.circle_filled(c, R, snap_col);
                }
                oxidraft_cad::SnapKind::Quadrant => {
                    let pts = vec![
                        pos2(c.x, c.y - R),
                        pos2(c.x + R, c.y),
                        pos2(c.x, c.y + R),
                        pos2(c.x - R, c.y),
                    ];
                    painter.add(egui::Shape::convex_polygon(pts, snap_col, no_stroke));
                }
                oxidraft_cad::SnapKind::Tangent => {
                    let cr = R - 1.0;
                    let cc = pos2(c.x, c.y + 1.0);
                    painter.circle_filled(cc, cr, snap_col);
                    let ty = cc.y - cr - 1.5;
                    painter.line_segment([pos2(c.x - R, ty), pos2(c.x + R, ty)], stroke);
                }
                oxidraft_cad::SnapKind::Intersection => {
                    painter.line_segment([pos2(c.x - R, c.y - R), pos2(c.x + R, c.y + R)], stroke);
                    painter.line_segment([pos2(c.x + R, c.y - R), pos2(c.x - R, c.y + R)], stroke);
                }
                oxidraft_cad::SnapKind::Perpendicular => {
                    painter.line_segment([pos2(c.x - R, c.y - R), pos2(c.x - R, c.y + R)], stroke);
                    painter.line_segment([pos2(c.x - R, c.y + R), pos2(c.x + R, c.y + R)], stroke);
                    painter.line_segment([pos2(c.x, c.y + R), pos2(c.x, c.y)], stroke);
                    painter.line_segment([pos2(c.x, c.y), pos2(c.x - R, c.y)], stroke);
                }
                oxidraft_cad::SnapKind::Nearest => {
                    let tl = pos2(c.x - R, c.y - R);
                    let tr = pos2(c.x + R, c.y - R);
                    let bl = pos2(c.x - R, c.y + R);
                    let br = pos2(c.x + R, c.y + R);
                    painter.add(egui::Shape::convex_polygon(
                        vec![tl, tr, c],
                        snap_col,
                        no_stroke,
                    ));
                    painter.add(egui::Shape::convex_polygon(
                        vec![bl, br, c],
                        snap_col,
                        no_stroke,
                    ));
                }
                oxidraft_cad::SnapKind::Insertion => {
                    let s = R * 1.1;
                    painter.rect_filled(
                        egui::Rect::from_center_size(pos2(c.x - 2.5, c.y + 2.5), vec2(s, s)),
                        0.0,
                        snap_col.gamma_multiply(0.55),
                    );
                    painter.rect_filled(
                        egui::Rect::from_center_size(pos2(c.x + 2.5, c.y - 2.5), vec2(s, s)),
                        0.0,
                        snap_col,
                    );
                }
            }
            let label = match sp.kind {
                oxidraft_cad::SnapKind::Endpoint => "Endpoint",
                oxidraft_cad::SnapKind::Midpoint => "Midpoint",
                oxidraft_cad::SnapKind::Center => "Center",
                oxidraft_cad::SnapKind::Node => "Node",
                oxidraft_cad::SnapKind::Quadrant => "Quadrant",
                oxidraft_cad::SnapKind::Tangent => "Tangent",
                oxidraft_cad::SnapKind::Intersection => "Intersection",
                oxidraft_cad::SnapKind::Perpendicular => "Perpendicular",
                oxidraft_cad::SnapKind::Nearest => "Nearest",
                oxidraft_cad::SnapKind::Insertion => "Insertion",
            };
            let ink = Color32::from_rgb(20, 20, 20);
            let galley =
                painter.layout_no_wrap(label.to_owned(), egui::FontId::proportional(12.0), ink);
            let pad = vec2(7.0, 4.0);
            let size = galley.size() + pad * 2.0;
            let mut chip_min = pos2(c.x - R - 8.0 - size.x, c.y - R - 8.0 - size.y);
            chip_min.x = chip_min.x.max(rect.left() + 2.0).round();
            chip_min.y = chip_min.y.max(rect.top() + 2.0).round();
            let chip = egui::Rect::from_min_size(chip_min, size);
            painter.rect_filled(chip, 4.0, snap_col);
            painter.galley((chip_min + pad).round(), galley, ink);
        }

        let has_dims = app.tool.has_pending_input();
        let is_drawing = !matches!(app.tool, Tool::Select);
        let has_input = is_drawing || !ui_state.command_input.is_empty() || has_dims;

        if app.dyn_on && (has_dims || has_input) {
            let font_id = egui::FontId::monospace(11.0);
            let text_color = Color32::from_rgb(230, 240, 255);
            let bg_color = Color32::from_rgba_unmultiplied(20, 26, 36, 225);
            let dim_border = Stroke::new(1.0, Color32::from_rgb(80, 95, 115));
            let input_border = Stroke::new(1.0, Color32::from_rgb(0, 255, 0));

            let dims_text = if has_dims {
                let cursor = Point2d::from_f64(app.cursor_world.0, app.cursor_world.1);
                match &app.tool {
                    Tool::Line { last: Some(p0) } if !app.dyn_on => {
                        let d = p0.dist_f64(&cursor);
                        let (x0, y0) = p0.to_f64();
                        let (x1, y1) = cursor.to_f64();
                        let dx = x1 - x0;
                        let dy = y1 - y0;
                        let angle_deg = oxidraft_geometry::wrap_deg360(dy.atan2(dx).to_degrees());
                        Some(format!("L: {:.4}\nA: {:.1}°", d, angle_deg))
                    }
                    Tool::Circle { center: Some(c) } => {
                        let r = c.dist_f64(&cursor);
                        Some(format!("R: {:.4}", r))
                    }
                    Tool::Rectangle { first: Some(c0) } => {
                        let (x0, y0) = c0.to_f64();
                        let (x1, y1) = cursor.to_f64();
                        let w = (x1 - x0).abs();
                        let h = (y1 - y0).abs();
                        Some(format!("W: {:.4}\nH: {:.4}", w, h))
                    }
                    Tool::Arc3 { pts } => {
                        if pts.len() == 1 {
                            let d = pts[0].dist_f64(&cursor);
                            Some(format!("Dist: {:.4}", d))
                        } else if pts.len() == 2 {
                            match oxidraft_geometry::CircularArc::from_three_points(
                                &pts[0], &pts[1], &cursor,
                            ) {
                                Some(arc) => {
                                    let r = arc.radius;
                                    Some(format!("R: {:.4}", r))
                                }
                                None => Some("Collinear".to_string()),
                            }
                        } else {
                            None
                        }
                    }
                    Tool::Move { base: Some(b), .. } => {
                        let d = b.dist_f64(&cursor);
                        let (x0, y0) = b.to_f64();
                        let (x1, y1) = cursor.to_f64();
                        let dx = x1 - x0;
                        let dy = y1 - y0;
                        Some(format!("D: {:.4}\ndx: {:.4}\ndy: {:.4}", d, dx, dy))
                    }
                    Tool::Copy { base: Some(b), .. } => {
                        let d = b.dist_f64(&cursor);
                        let (x0, y0) = b.to_f64();
                        let (x1, y1) = cursor.to_f64();
                        let dx = x1 - x0;
                        let dy = y1 - y0;
                        Some(format!("D: {:.4}\ndx: {:.4}\ndy: {:.4}", d, dx, dy))
                    }
                    Tool::Polygon {
                        center: Some(c),
                        radius_point,
                        sides,
                    } => {
                        // Once the radius click has landed, the shape is fixed;
                        // report that point's numbers instead of the still-moving
                        // cursor, matching what `Tool::preview` now renders.
                        let rp = radius_point.unwrap_or(cursor);
                        let d = c.dist_f64(&rp);
                        let (x0, y0) = c.to_f64();
                        let (x1, y1) = rp.to_f64();
                        let dx = x1 - x0;
                        let dy = y1 - y0;
                        let angle_deg = oxidraft_geometry::wrap_deg360(dy.atan2(dx).to_degrees());
                        let n = sides.map(|s| s.to_string()).unwrap_or_else(|| "?".into());
                        Some(format!("R: {:.4}\nA: {:.1}°\nSides: {}", d, angle_deg, n))
                    }
                    Tool::Spline { pts } => {
                        if let Some(last) = pts.last() {
                            let d = last.dist_f64(&cursor);
                            Some(format!("Dist: {:.4}\nPoints: {}/4", d, pts.len()))
                        } else {
                            None
                        }
                    }
                    Tool::Polyline { pts } => {
                        if let Some(last) = pts.last() {
                            let d = last.dist_f64(&cursor);
                            Some(format!("L: {:.4}\nPoints: {}", d, pts.len()))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            } else {
                None
            };

            let input_text: Option<String> = None;

            if dims_text.is_some() || input_text.is_some() {
                let offset = vec2(15.0, 15.0);
                let padding = vec2(6.0, 4.0);

                let mut combined_rect = egui::Rect::NOTHING;
                let mut size1 = vec2(0.0, 0.0);
                let mut size2 = vec2(0.0, 0.0);
                let mut galley1 = None;
                let mut galley2 = None;

                if let Some(t1) = &dims_text {
                    let g1 = painter.layout_no_wrap(t1.clone(), font_id.clone(), text_color);
                    size1 = g1.size() + padding * 2.0;
                    galley1 = Some(g1);
                }
                if let Some(t2) = &input_text {
                    let g2 = painter.layout_no_wrap(t2.clone(), font_id.clone(), text_color);
                    size2 = g2.size() + padding * 2.0;
                    galley2 = Some(g2);
                }

                let mut rect1 = egui::Rect::NOTHING;
                let mut rect2 = egui::Rect::NOTHING;

                if galley1.is_some() && galley2.is_some() {
                    rect1 = egui::Rect::from_min_size(cc + offset, size1);
                    rect2 = egui::Rect::from_min_size(rect1.left_bottom() + vec2(0.0, 5.0), size2);
                    combined_rect = rect1.union(rect2);
                } else if galley1.is_some() {
                    rect1 = egui::Rect::from_min_size(cc + offset, size1);
                    combined_rect = rect1;
                } else if galley2.is_some() {
                    rect2 = egui::Rect::from_min_size(cc + offset, size2);
                    combined_rect = rect2;
                }

                let mut translation = vec2(0.0, 0.0);
                if combined_rect.right() > rect.right() {
                    translation.x = rect.right() - combined_rect.right();
                }
                if combined_rect.bottom() > rect.bottom() {
                    translation.y = rect.bottom() - combined_rect.bottom();
                }
                if combined_rect.left() + translation.x < rect.left() {
                    translation.x = rect.left() - combined_rect.left();
                }
                if combined_rect.top() + translation.y < rect.top() {
                    translation.y = rect.top() - combined_rect.top();
                }

                if let Some(g1) = galley1 {
                    let final_rect1 = rect1.translate(translation);
                    painter.rect(
                        final_rect1,
                        3.0,
                        bg_color,
                        dim_border,
                        egui::StrokeKind::Middle,
                    );
                    painter.galley(final_rect1.min + padding, g1, text_color);
                }
                if let Some(g2) = galley2 {
                    let final_rect2 = rect2.translate(translation);
                    painter.rect(
                        final_rect2,
                        3.0,
                        bg_color,
                        input_border,
                        egui::StrokeKind::Middle,
                    );
                    painter.galley(final_rect2.min + padding, g2, text_color);
                }
            }
        }
        draw_scale_bar(&painter, app, rect);
        if !matches!(app.tool, Tool::Select) {
            let prompt = tool_prompt(&app.tool);
            let chip = format!("{} — {}   ·   Esc cancel", app.tool.name(), prompt);
            draw_prompt_chip(&painter, rect, &chip);
        }
        if let Some((id, content, font, size)) = text_commit {
            app.commit_text_edit(id, content, font, size);
        }
    });
}
#[cfg(test)]
mod tess_tests {
    use super::tessellate::{flatten_curve, point_seg_dist_sq};
    use crate::view_transform::ViewTransform;
    use oxidraft_geometry::{CircularArc, Curve, CurveSegment, Point2d};

    fn circle(r: f64) -> Curve {
        Curve::Arc(CircularArc::new(
            Point2d::from_i64(0, 0),
            r,
            0.0,
            std::f64::consts::TAU,
        ))
    }

    fn screen_polyline(view: &ViewTransform, c: &Curve) -> Vec<egui::Pos2> {
        let to_screen = |wx: f64, wy: f64| {
            let (sx, sy) = view.world_to_screen(wx, wy);
            egui::pos2(sx as f32, sy as f32)
        };
        flatten_curve(c, &to_screen)
    }
    #[test]
    fn circle_stays_smooth_when_zoomed_in() {
        let mut view = ViewTransform::new(1000.0, 1000.0);
        view.zoom = 500.0;
        let c = circle(2.0);
        let poly = screen_polyline(&view, &c);

        let to_screen = |wx: f64, wy: f64| {
            let (sx, sy) = view.world_to_screen(wx, wy);
            egui::pos2(sx as f32, sy as f32)
        };
        let mut worst = 0.0f32;
        for k in 0..2000 {
            let t = std::f64::consts::TAU * k as f64 / 2000.0;
            let (x, y) = c.evaluate_f64(t);
            let p = to_screen(x, y);
            let mut best = f32::INFINITY;
            for w in poly.windows(2) {
                best = best.min(point_seg_dist_sq(p, w[0], w[1]).sqrt());
            }
            worst = worst.max(best);
        }
        assert!(
            worst < 1.0,
            "max chord deviation {:.3}px exceeds 1px (faceting)",
            worst
        );
    }
    #[test]
    fn segment_count_tracks_zoom() {
        let c = circle(1.0);
        let mut small = ViewTransform::new(800.0, 600.0);
        small.zoom = 2.0;
        let mut big = ViewTransform::new(800.0, 600.0);
        big.zoom = 2000.0;
        let n_small = screen_polyline(&small, &c).len();
        let n_big = screen_polyline(&big, &c).len();
        assert!(
            n_big > n_small * 4,
            "expected far more segments when zoomed in: {} vs {}",
            n_big,
            n_small
        );
        assert!(
            n_small < 40,
            "tiny circle should be cheap, got {} points",
            n_small
        );
    }
}
