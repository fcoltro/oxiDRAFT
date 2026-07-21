use super::UiState;
use super::chrome::{self, Act, act_needs_selection, group_entries, group_id, run_act};
use crate::icons::{self, Icon};
use crate::state::AppState;
use crate::theme;
use egui::{Color32, Context, Key, Pos2, Rect, Stroke, vec2};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RadialRing {
    Draw,
    Modify,
}

// Neutral zone over the hub: no hover or click registers here.
const DEAD_ZONE: f32 = 72.0;

// Move past this (from the centre) to commit to a category / expand a group —
// set just inside the wedge ring so you commit as the cursor leaves the hub.
const ROOT_EXPAND: f32 = 72.0;
const CATEGORY_EXPAND: f32 = ROOT_EXPAND;

// The pie geometry: a large central hub, then a thin wedge ring hugging it.
const HUB_RADIUS: f32 = 80.0;
const RING_INNER: f32 = 84.0;
const RING_OUTER: f32 = 150.0;
// Wedges sit flush against each other; the divider lines alone separate them.
const WEDGE_HALF_GAP: f32 = 0.0;
const RING_ICON_SIZE: f32 = 23.0;

const VARIANT_ARC_STEP: f32 = 0.34;
const VARIANT_UNLATCH_MARGIN: f32 = 0.15;
// The variant sub-menu is a second concentric ring of slices just outside the
// tool ring — a continuation of the pizza — fanned around the parent wedge.
const VARIANT_INNER: f32 = 154.0;
const VARIANT_OUTER: f32 = 212.0;
const VARIANT_ICON_SIZE: f32 = 21.0;

// Local palette — the theme's WIDGET_* tokens are near-transparent (they sit on
// panels), but the radial menu floats over the canvas and needs solid fills.
const WEDGE_BG: Color32 = Color32::from_rgb(28, 32, 41);
const WEDGE_HOVER: Color32 = Color32::from_rgb(49, 57, 71);
const HUB_BG: Color32 = Color32::from_rgb(14, 16, 22);
const DIVIDER: Color32 = Color32::from_rgb(54, 60, 73);
const EDGE_HL: Color32 = Color32::from_rgb(150, 162, 182);

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

/// Unit direction for a menu angle. Angle 0 points straight up, matching
/// [`angle_of`] and [`wedge_at`], so drawing and hit-testing agree.
fn dir_of(angle: f32) -> egui::Vec2 {
    vec2(angle.sin(), -angle.cos())
}

fn variant_angle(parent_angle: f32, index: usize, count: usize) -> f32 {
    let offset = index as f32 - (count as f32 - 1.0) / 2.0;
    parent_angle + offset * VARIANT_ARC_STEP
}

fn angle_diff(a: f32, b: f32) -> f32 {
    let d = oxidraft_geometry::util::wrap_tau((a - b) as f64) as f32;
    if d > std::f32::consts::PI {
        d - std::f32::consts::TAU
    } else {
        d
    }
}

fn nearest_variant(angle: f32, parent_angle: f32, count: usize) -> usize {
    (0..count)
        .min_by(|&i, &j| {
            let da = angle_diff(angle, variant_angle(parent_angle, i, count)).abs();
            let db = angle_diff(angle, variant_angle(parent_angle, j, count)).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0)
}

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

fn latch<T>(state: &mut Option<T>, dist: f32, expand: f32, compute: impl FnOnce() -> Option<T>) {
    if state.is_none() {
        if dist > expand {
            *state = compute();
        }
    } else if dist <= expand {
        *state = None;
    }
}

/// Points sampled along the arc at `radius`, spanning `±half_span` about
/// `center_angle` — used for the filled wedge and its hovered-edge highlight.
fn arc_points(center: Pos2, center_angle: f32, half_span: f32, radius: f32) -> Vec<Pos2> {
    const SEGS: usize = 28;
    (0..=SEGS)
        .map(|k| {
            let a = center_angle - half_span + 2.0 * half_span * (k as f32 / SEGS as f32);
            center + dir_of(a) * radius
        })
        .collect()
}

/// Fills the annular sector (a pie slice with the hub cut out) as a triangle
/// strip between the inner and outer arcs — a mesh so the concave inner edge
/// tessellates correctly, which a convex-polygon fill would not.
fn fill_sector(
    painter: &egui::Painter,
    center: Pos2,
    center_angle: f32,
    half_span: f32,
    inner_r: f32,
    outer_r: f32,
    color: Color32,
) {
    const SEGS: usize = 28;
    let mut mesh = egui::Mesh::default();
    for k in 0..=SEGS {
        let a = center_angle - half_span + 2.0 * half_span * (k as f32 / SEGS as f32);
        let d = dir_of(a);
        mesh.colored_vertex(center + d * inner_r, color);
        mesh.colored_vertex(center + d * outer_r, color);
    }
    for k in 0..SEGS {
        let i = (k * 2) as u32;
        mesh.add_triangle(i, i + 1, i + 2);
        mesh.add_triangle(i + 1, i + 3, i + 2);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// One pie wedge in the ring between `inner_r` and `outer_r`: filled sector,
/// side dividers, a hovered/active outer-edge highlight, and a centred icon or
/// text label. Shared by the tool ring and the variant ring so both read as
/// slices of the same pizza.
#[allow(clippy::too_many_arguments)]
fn draw_sector(
    painter: &egui::Painter,
    ctx: &egui::Context,
    center: Pos2,
    center_angle: f32,
    half_span: f32,
    inner_r: f32,
    outer_r: f32,
    icon_size: f32,
    hovered: bool,
    active: bool,
    enabled: bool,
    icon: Option<Icon>,
    text: Option<&str>,
) {
    let bg = if hovered { WEDGE_HOVER } else { WEDGE_BG };
    fill_sector(
        painter,
        center,
        center_angle,
        half_span,
        inner_r,
        outer_r,
        bg,
    );

    for edge in [center_angle - half_span, center_angle + half_span] {
        let d = dir_of(edge);
        painter.line_segment(
            [center + d * inner_r, center + d * outer_r],
            Stroke::new(1.0, DIVIDER),
        );
    }
    if hovered || active {
        let hl = if active { theme::ACCENT } else { EDGE_HL };
        painter.add(egui::Shape::line(
            arc_points(center, center_angle, half_span, outer_r - 1.5),
            Stroke::new(2.5, hl),
        ));
    }

    let mid = (inner_r + outer_r) * 0.5;
    let pos = center + dir_of(center_angle) * mid;
    let tint = if !enabled {
        theme::TEXT_DIM
    } else if hovered {
        Color32::WHITE
    } else {
        theme::TEXT
    };
    if let Some(icon) = icon {
        icons::paint_icon(
            painter,
            ctx,
            icon,
            Rect::from_center_size(pos, vec2(icon_size, icon_size)),
            tint,
        );
    }
    if let Some(text) = text {
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            short_label(text),
            egui::FontId::proportional(theme::tok::T_SM),
            tint,
        );
    }
}

/// The dark central hub, showing the hovered entry's label like the reference
/// design ("Toggle overlays"). Long labels wrap to the hub width.
fn draw_hub(painter: &egui::Painter, center: Pos2, label: Option<&str>) {
    painter.circle_filled(center, HUB_RADIUS, HUB_BG);
    painter.circle_stroke(center, HUB_RADIUS, Stroke::new(1.0, DIVIDER));
    match label {
        Some(text) => {
            let galley = painter.layout(
                text.to_string(),
                egui::FontId::proportional(theme::tok::T_LG),
                theme::TEXT,
                HUB_RADIUS * 1.5,
            );
            painter.galley(center - galley.size() * 0.5, galley, theme::TEXT);
        }
        None => {
            painter.text(
                center,
                egui::Align2::CENTER_CENTER,
                "Radial",
                egui::FontId::proportional(theme::tok::T_XS),
                theme::TEXT_DIM,
            );
        }
    }
}

/// A small round button for a group's expanded variants, which fan out from
/// their parent wedge rather than tiling the ring.
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
        if focused || !over_canvas || other_overlay_open || raw_keystroke_capture {
            return false;
        }
        let mods = ctx.input(|i| i.modifiers);
        if mods.ctrl || mods.command || mods.alt {
            return false;
        }
        let pressed = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Q))
            || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::Q));
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
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Q))
        || ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, Key::Q))
    {
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
    latch(&mut ui_state.radial_category, dist, ROOT_EXPAND, || {
        Some(root_choice)
    });
    let category = ui_state.radial_category;
    let category_entries = category.map(ring_entries);
    let category_hovered = category_entries
        .as_ref()
        .map(|entries| wedge_at(angle, entries.len()).min(entries.len().saturating_sub(1)));
    let parent_angle_of = |gid: u8| -> Option<f32> {
        let entries = category_entries.as_ref()?;
        let count = entries.len();
        entries
            .iter()
            .position(|(_, _, act)| group_id(act) == Some(gid))
            .map(|idx| wedge_center_angle(idx, count))
    };
    let hovered_gid = match (&category_entries, category_hovered) {
        (Some(entries), Some(idx)) => group_id(&entries[idx].2),
        _ => None,
    };
    let variant_still_relevant = ui_state.radial_expanded.is_some_and(|gid| {
        let sub_count = group_entries(gid).len();
        let half_span = if sub_count > 1 {
            VARIANT_ARC_STEP * (sub_count - 1) as f32 / 2.0
        } else {
            0.0
        };
        parent_angle_of(gid)
            .is_some_and(|pa| angle_diff(angle, pa).abs() <= half_span + VARIANT_UNLATCH_MARGIN)
    });
    if !variant_still_relevant {
        ui_state.radial_expanded = None;
    }
    latch(&mut ui_state.radial_expanded, dist, CATEGORY_EXPAND, || {
        hovered_gid
    });
    let variant_gid = ui_state.radial_expanded;
    let variant_entries = variant_gid.map(group_entries);
    let parent_angle = variant_gid.and_then(parent_angle_of);
    let variant_hovered = match (&variant_entries, parent_angle) {
        (Some(v), Some(pa)) => Some(nearest_variant(angle, pa, v.len())),
        _ => None,
    };

    // The hub shows the label of the deepest thing currently hovered.
    let hub_label: Option<String> =
        if let (Some(sub), Some(si)) = (&variant_entries, variant_hovered) {
            Some(short_label(sub[si].1).to_string())
        } else if variant_entries.is_none() {
            if let (Some(entries), Some(idx)) = (&category_entries, category_hovered) {
                Some(short_label(entries[idx].1).to_string())
            } else if category.is_none() && dist > DEAD_ZONE {
                Some(if root_idx == 0 { "Tools" } else { "Modifiers" }.to_string())
            } else {
                None
            }
        } else {
            None
        };

    let active_name = app.tool.name();
    let has_sel = app.has_selection();
    let catch = egui::Area::new(egui::Id::new("radial_menu"))
        .order(egui::Order::Tooltip)
        .fixed_pos(canvas_rect.min)
        .show(ctx, |ui| {
            let catch = ui.allocate_rect(ctx.content_rect(), egui::Sense::click());
            let painter = ui.painter();
            let outer_r = if variant_entries.is_some() {
                VARIANT_OUTER + 22.0
            } else {
                RING_OUTER + 28.0
            };
            painter.circle_filled(center, outer_r, theme::PANEL_GLASS);
            painter.circle_stroke(center, outer_r, Stroke::new(1.0, DIVIDER));

            if let Some(entries) = &category_entries {
                // A category was chosen: a full ring of tool wedges.
                let count = entries.len();
                let half = std::f32::consts::PI / count as f32 - WEDGE_HALF_GAP;
                let expanded = variant_entries.is_some();
                for (i, (icon, label, act)) in entries.iter().enumerate() {
                    let ca = wedge_center_angle(i, count);
                    // The group whose variants are open stays lit to show it's
                    // the one in use; otherwise the plain angular hover.
                    let is_parent =
                        expanded && variant_gid.is_some() && group_id(act) == variant_gid;
                    let hovered = is_parent
                        || (!expanded && category_hovered == Some(i) && dist > ROOT_EXPAND);
                    let active = matches!(act, Act::Tool(t) if active_name == t.name());
                    let enabled = has_sel || !act_needs_selection(act);
                    draw_sector(
                        painter,
                        ctx,
                        center,
                        ca,
                        half,
                        RING_INNER,
                        RING_OUTER,
                        RING_ICON_SIZE,
                        hovered,
                        active,
                        enabled,
                        Some(*icon),
                        None,
                    );
                    let _ = label;
                }
            } else {
                // Root: the circle split into two big halves — Tools / Modifiers.
                let half = std::f32::consts::PI / 2.0 - WEDGE_HALF_GAP;
                for (i, label) in ["Tools", "Modifiers"].iter().enumerate() {
                    let ca = wedge_center_angle(i, 2);
                    let hovered = i == root_idx && dist > DEAD_ZONE;
                    draw_sector(
                        painter,
                        ctx,
                        center,
                        ca,
                        half,
                        RING_INNER,
                        RING_OUTER,
                        RING_ICON_SIZE,
                        hovered,
                        false,
                        true,
                        None,
                        Some(label),
                    );
                }
            }

            // Variant sub-menu: a second ring of slices fanned around the parent.
            if let (Some(sub), Some(pa)) = (&variant_entries, parent_angle) {
                let count = sub.len();
                let half = VARIANT_ARC_STEP / 2.0 - WEDGE_HALF_GAP;
                for (i, (icon, _label, act)) in sub.iter().enumerate() {
                    let ca = variant_angle(pa, i, count);
                    let hovered = variant_hovered == Some(i);
                    let active = matches!(act, Act::Tool(t) if active_name == t.name());
                    let enabled = has_sel || !act_needs_selection(act);
                    draw_sector(
                        painter,
                        ctx,
                        center,
                        ca,
                        half,
                        VARIANT_INNER,
                        VARIANT_OUTER,
                        VARIANT_ICON_SIZE,
                        hovered,
                        active,
                        enabled,
                        Some(*icon),
                        None,
                    );
                }
            }

            draw_hub(painter, center, hub_label.as_deref());
            catch
        })
        .inner;
    if catch.clicked() {
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
        assert_eq!(wedge_at(0.0, 2), 0);
        assert_eq!(wedge_at(std::f32::consts::PI, 2), 1);
    }
}
