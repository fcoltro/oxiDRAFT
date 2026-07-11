use super::UiState;
use super::chrome::{self, Act, act_needs_selection, group_entries, group_id, run_act};
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::theme;
use crate::tools::Tool;
use egui::{Color32, Context, Key, Pos2, Rect, Stroke, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RadialRing {
    Draw,
    Modify,
}

const DEAD_ZONE: f32 = 26.0;
const ROOT_RADIUS: f32 = 58.0;
const ROOT_EXPAND: f32 = 84.0;
const CATEGORY_RADIUS: f32 = 148.0;
const CATEGORY_EXPAND: f32 = 176.0;
const VARIANT_RADIUS: f32 = 222.0;
const ICON_SIZE: f32 = 30.0;
const ROOT_ICON_SIZE: f32 = 40.0;

/// Buckets a clockwise-from-up angle (radians, any range) into one of
/// `count` evenly-spaced wedges centred on multiples of `TAU / count`.
fn wedge_at(angle: f32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let step = std::f32::consts::TAU / count as f32;
    let a = oxidraft_geometry::util::wrap_tau(angle as f64) as f32;
    (((a + step / 2.0) / step).floor() as usize) % count
}

fn angle_of(center: Pos2, p: Pos2) -> f32 {
    let d = p - center;
    d.x.atan2(-d.y)
}

fn wedge_center_angle(index: usize, count: usize) -> f32 {
    index as f32 * std::f32::consts::TAU / count as f32
}

fn wedge_point(center: Pos2, index: usize, count: usize, radius: f32) -> Pos2 {
    let a = wedge_center_angle(index, count);
    center + vec2(a.sin(), -a.cos()) * radius
}

/// Strips the trailing "  (hotkey)" / " — explanation" suffixes dock
/// tooltips carry, so the pie shows just the tool name.
fn short_label(s: &str) -> &str {
    s.split("  (")
        .next()
        .unwrap_or(s)
        .split(" —")
        .next()
        .unwrap_or(s)
        .trim()
}

fn ring_entries(ring: RadialRing) -> Vec<(Icon, &'static str, Act)> {
    match ring {
        RadialRing::Draw => chrome::draw_entries(),
        RadialRing::Modify => chrome::modify_entries(),
    }
}

/// Shared shape for revealing a nested ring: `state` becomes `compute()`'s
/// result once `dist` first exceeds `expand`, and resets to `None` once the
/// user retreats back under it. Latching (rather than re-deriving the choice
/// from the current angle every frame) means the revealed ring occupies the
/// *full* circle instead of flipping underneath the cursor as it sweeps past
/// the angle where the ring was first entered.
fn latch<T>(state: &mut Option<T>, dist: f32, expand: f32, compute: impl FnOnce() -> Option<T>) {
    if state.is_none() {
        if dist > expand {
            *state = compute();
        }
    } else if dist <= expand {
        *state = None;
    }
}

/// The ellipse tool's dynamic-input HUD already binds Tab to hop between its
/// major/minor length fields (see `dyn_ellipse_hud`); the radial menu must
/// not steal that key while it's active.
fn ellipse_hud_wants_tab(app: &AppState) -> bool {
    app.dyn_on
        && matches!(
            app.tool,
            Tool::Ellipse {
                center: Some(_),
                ..
            }
        )
}

/// Fill + stroke for a wedge's circular backdrop, shared by
/// `draw_wedge_icon` and `draw_root_wedge`.
fn wedge_shell(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    bg: Color32,
    stroke_color: Color32,
    stroke_width: f32,
) {
    painter.circle_filled(center, radius, bg);
    painter.circle_stroke(center, radius, Stroke::new(stroke_width, stroke_color));
}

#[allow(clippy::too_many_arguments)]
fn draw_wedge_icon(
    painter: &egui::Painter,
    ctx: &egui::Context,
    center: Pos2,
    icon: Icon,
    label: &str,
    hovered: bool,
    active: bool,
    enabled: bool,
) {
    // Hover gets the app's ordinary neutral hover tint; `ACCENT` is reserved
    // for "this is the currently active tool" everywhere else in the UI
    // (see `icons::icon_button_sized`), so a wedge that's merely hovered
    // shouldn't look identical to the one that's actually active.
    let bg = if hovered {
        theme::WIDGET_HOVER
    } else {
        theme::WIDGET_BG
    };
    let stroke_color = if active {
        theme::ACCENT
    } else {
        theme::OUTLINE
    };
    wedge_shell(painter, center, ICON_SIZE * 0.62, bg, stroke_color, 1.0);
    let tint = if !enabled {
        theme::TEXT_DIM
    } else if hovered {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    icons::paint_icon(
        painter,
        ctx,
        icon,
        Rect::from_center_size(center, vec2(ICON_SIZE * 0.8, ICON_SIZE * 0.8)),
        tint,
    );
    if hovered {
        painter.text(
            center + vec2(0.0, ICON_SIZE * 0.9),
            egui::Align2::CENTER_TOP,
            short_label(label),
            egui::FontId::proportional(theme::tok::T_XS),
            theme::TEXT,
        );
    }
}

/// Draws one of the two root wedges ("Tools" / "Modifiers"). These are pure
/// navigation choices — not `Act`s — so they get their own, label-carrying
/// widget instead of `draw_wedge_icon`'s icon+tooltip-on-hover shape.
fn draw_root_wedge(
    painter: &egui::Painter,
    center: Pos2,
    label: &str,
    hovered: bool,
    dimmed: bool,
) {
    let bg = if hovered {
        theme::WIDGET_HOVER
    } else {
        theme::WIDGET_BG
    };
    let stroke_color = if hovered {
        theme::ACCENT
    } else {
        theme::OUTLINE
    };
    wedge_shell(
        painter,
        center,
        ROOT_ICON_SIZE * 0.62,
        bg,
        stroke_color,
        1.2,
    );
    let color = if dimmed {
        theme::TEXT_DIM
    } else if hovered {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(theme::tok::T_XS),
        color,
    );
}

/// Drives the hold-Tab radial tool menu. It always opens on a 2-item
/// Tools/Modifiers root at the cursor; dragging past `ROOT_EXPAND` while
/// over one of those reveals its full tool ring, and pushing further past
/// `CATEGORY_EXPAND` on a grouped tool (Circle/Arc/Dimension/Line) reveals
/// its construction-method variants as a third ring. Whatever's hovered
/// deepest activates on key-up via the same `run_act`/`Command::Activate`
/// path the dock uses. Returns whether the menu is open this frame, so
/// callers can suppress canvas picking the same way the command palette
/// already does.
pub(super) fn radial_menu(
    ctx: &Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    canvas_rect: Rect,
) -> bool {
    let pointer = ctx.pointer_latest_pos();

    if !ui_state.radial_open {
        let focused = ctx.memory(|m| m.focused()).is_some();
        let over_canvas = pointer.is_some_and(|p| canvas_rect.contains(p));
        let other_overlay_open = ui_state.about_open
            || ui_state.settings_open
            || ui_state.line_props_open
            || app.plot_dialog_open
            || ctx.data(|d| {
                d.get_temp::<bool>(egui::Id::new("palette_open_state"))
                    .unwrap_or(false)
            });
        let raw_keystroke_capture = app.grip_editing() || app.interaction.corner_action.is_some();
        if focused
            || !over_canvas
            || ellipse_hud_wants_tab(app)
            || other_overlay_open
            || raw_keystroke_capture
        {
            return false;
        }
        let mods = ctx.input(|i| i.modifiers);
        if mods.ctrl || mods.command || mods.alt {
            return false;
        }
        let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::Tab));
        if !pressed {
            return false;
        }
        ui_state.radial_open = true;
        ui_state.radial_center = pointer;
        ui_state.radial_category = None;
        ui_state.radial_expanded = None;
        return true;
    }

    let center = ui_state
        .radial_center
        .unwrap_or_else(|| canvas_rect.center());

    let cancel = |ui_state: &mut UiState| {
        ui_state.radial_open = false;
        ui_state.radial_category = None;
        ui_state.radial_expanded = None;
    };

    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        cancel(ui_state);
        return false;
    }

    let cur = pointer.unwrap_or(center);
    let dist = (cur - center).length();
    let angle = angle_of(center, cur);

    let root_idx = wedge_at(angle, 2);
    let root_choice = if root_idx == 0 {
        RadialRing::Draw
    } else {
        RadialRing::Modify
    };
    // The category, once chosen, occupies the *full* circle — Circle or
    // Mirror can sit anywhere around it — so the root choice is latched at
    // the moment of crossing `ROOT_EXPAND`, not re-derived from the current
    // angle every frame; otherwise sweeping past the halfway point while
    // picking a tool would flip the whole ring underneath the cursor.
    latch(&mut ui_state.radial_category, dist, ROOT_EXPAND, || {
        Some(root_choice)
    });
    let category = ui_state.radial_category;

    let category_entries = category.map(ring_entries);
    let category_hovered = category_entries
        .as_ref()
        .map(|entries| wedge_at(angle, entries.len()).min(entries.len().saturating_sub(1)));

    // Same latching logic one level deeper: once a grouped tool's variant
    // ring is showing, it also occupies the full circle.
    latch(
        &mut ui_state.radial_expanded,
        dist,
        CATEGORY_EXPAND,
        || match (&category_entries, category_hovered) {
            (Some(entries), Some(idx)) => group_id(&entries[idx].2),
            _ => None,
        },
    );
    let variant_gid = ui_state.radial_expanded;
    let variant_entries = variant_gid.map(group_entries);
    let variant_hovered = variant_entries
        .as_ref()
        .map(|v| wedge_at(angle, v.len()).min(v.len().saturating_sub(1)));

    egui::Area::new(egui::Id::new("radial_menu"))
        .order(egui::Order::Tooltip)
        .fixed_pos(canvas_rect.min)
        .show(ctx, |ui| {
            let catch = ui.allocate_rect(ctx.content_rect(), egui::Sense::click());
            let painter = ui.painter();
            let outer_r = if variant_entries.is_some() {
                VARIANT_RADIUS + 40.0
            } else if category_entries.is_some() {
                CATEGORY_RADIUS + 40.0
            } else {
                ROOT_EXPAND + 24.0
            };
            painter.circle_filled(center, outer_r, theme::PANEL_GLASS);
            painter.circle_stroke(center, outer_r, Stroke::new(1.0_f32, theme::OUTLINE));
            painter.circle_filled(center, DEAD_ZONE, theme::WIDGET_BG);

            let root_dimmed = category.is_some();
            for (i, label) in ["Tools", "Modifiers"].iter().enumerate() {
                let pos = wedge_point(center, i, 2, ROOT_RADIUS);
                let hovered = !root_dimmed && i == root_idx && dist > DEAD_ZONE;
                draw_root_wedge(painter, pos, label, hovered, root_dimmed);
            }

            let active_name = app.tool.name();
            let has_sel = app.has_selection();
            if let Some(entries) = &category_entries {
                let count = entries.len();
                let category_dimmed = variant_entries.is_some();
                for (i, (icon, label, act)) in entries.iter().enumerate() {
                    let pos = wedge_point(center, i, count, CATEGORY_RADIUS);
                    let hovered =
                        !category_dimmed && category_hovered == Some(i) && dist > ROOT_EXPAND;
                    let active = matches!(act, Act::Tool(t) if active_name == t.name());
                    let enabled = has_sel || !act_needs_selection(act);
                    draw_wedge_icon(painter, ctx, pos, *icon, label, hovered, active, enabled);
                    if group_id(act).is_some() {
                        let tick = wedge_point(center, i, count, CATEGORY_RADIUS + 20.0);
                        painter.circle_filled(tick, 2.5, theme::TEXT_DIM);
                    }
                }
            }
            if let Some(sub) = &variant_entries {
                let sub_count = sub.len();
                for (i, (icon, label, act)) in sub.iter().enumerate() {
                    let pos = wedge_point(center, i, sub_count, VARIANT_RADIUS);
                    let hovered = variant_hovered == Some(i);
                    let active = matches!(act, Act::Tool(t) if active_name == t.name());
                    let enabled = has_sel || !act_needs_selection(act);
                    draw_wedge_icon(painter, ctx, pos, *icon, label, hovered, active, enabled);
                }
            }
            catch
        });

    if ctx.input(|i| i.key_released(Key::Tab)) {
        if dist > DEAD_ZONE {
            if let (Some(sub), Some(si)) = (&variant_entries, variant_hovered) {
                let act = &sub[si].2;
                if app.has_selection() || !act_needs_selection(act) {
                    run_act(app, act);
                }
            } else if let (Some(entries), Some(idx)) = (&category_entries, category_hovered)
                && variant_entries.is_none()
            {
                let act = &entries[idx].2;
                if app.has_selection() || !act_needs_selection(act) {
                    run_act(app, act);
                }
            }
        }
        cancel(ui_state);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::wedge_at;
    use std::f32::consts::TAU;

    #[test]
    fn zero_angle_is_wedge_zero() {
        assert_eq!(wedge_at(0.0, 4), 0);
    }

    #[test]
    fn just_before_boundary_stays_in_wedge() {
        let step = TAU / 4.0;
        assert_eq!(wedge_at(step / 2.0 - 0.01, 4), 0);
    }

    #[test]
    fn just_after_boundary_moves_to_next_wedge() {
        let step = TAU / 4.0;
        assert_eq!(wedge_at(step / 2.0 + 0.01, 4), 1);
    }

    #[test]
    fn negative_angle_wraps_correctly() {
        let step = TAU / 4.0;
        assert_eq!(wedge_at(-step / 2.0 - 0.01, 4), 3);
    }

    #[test]
    fn full_turn_wraps_to_start() {
        assert_eq!(wedge_at(TAU - 0.001, 4), 0);
    }

    #[test]
    fn single_entry_ring_always_zero() {
        assert_eq!(wedge_at(1.2345, 1), 0);
    }

    #[test]
    fn thirteen_entry_ring_covers_full_circle() {
        let count = 13;
        let step = TAU / count as f32;
        for i in 0..count {
            let angle = i as f32 * step;
            assert_eq!(wedge_at(angle, count), i);
        }
    }

    #[test]
    fn two_entry_root_splits_into_halves() {
        // Straight up -> Tools (index 0); straight down -> Modifiers (index 1).
        assert_eq!(wedge_at(0.0, 2), 0);
        assert_eq!(wedge_at(std::f32::consts::PI, 2), 1);
    }
}
