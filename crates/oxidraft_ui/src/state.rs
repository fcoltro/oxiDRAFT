use oxidraft_cad::{
    Grip, Guide, SnapPoint, SnapSettings, apply_grip, best_snap, edit, find_snaps_excluding,
    grips_for, infer_axis, pick_at,
};
use oxidraft_document::{
    ConstraintKind, Document, Entity, EntityId, EntityKind, Layer, LineTypeRef, LineWeight,
    SketchConstraint,
};
use oxidraft_geometry::{Curve, LineSeg, MinTracker, Point2d, Transform2d};

use crate::command::{Command, CoordInput, parse_command, parse_coordinate};
use crate::history::History;
use crate::tools::{Tool, ToolEvent};
use crate::view_transform::ViewTransform;

mod modify;
pub use modify::TrimExtendPreview;
mod contextual;

pub use contextual::{CornerAction, CornerGeom, CornerKind, fillet_arc};

pub struct AppState {
    pub document: Document,
    pub view: ViewTransform,
    pub tool: Tool,
    pub selection: Vec<EntityId>,
    pub snap: SnapSettings,
    pub snap_on: bool,
    pub grid_on: bool,
    pub grid_snap_on: bool,
    pub ortho_on: bool,
    pub polar_on: bool,
    pub track_on: bool,
    pub dyn_on: bool,
    pub last_command: Option<String>,
    pub history: History,
    pub command_log: Vec<String>,
    pub cursor_world: (f64, f64),
    pub active_snap: Option<SnapPoint>,
    pub click_count: u32,
    pub origin_id: EntityId,
    pub interaction: InteractionState,
    pub current_file_path: Option<std::path::PathBuf>,
    pub text_font: Option<String>,
    pub hatch_pattern: oxidraft_document::HatchPattern,
    /// [`History::current_revision`] at the last successful save; the
    /// document is dirty whenever the current revision differs.
    pub saved_revision: u64,
    pub zoom_target: Option<(f64, f64, f64)>,
    pub default_line_type: LineTypeRef,
    pub default_line_weight: LineWeight,
    pub comb_on: bool,
    pub comb_scale: f64,
    pub snap_px: f64,
    pub polar_step: f64,
    pub zoom_speed: f64,
    pub zoom_to_cursor: bool,
    pub invert_zoom: bool,
    pub crosshair: bool,
    pub pick_box: f64,
    pub show_lineweights: bool,
    pub lineweight_scale: f64,
    pub grid_dots: bool,
    pub grid_major_every: u32,
    pub grid_minor_rgb: (u8, u8, u8),
    pub grid_major_rgb: (u8, u8, u8),
    pub clipboard: Vec<Entity>,
    pub hint_tool: Option<Tool>,
    pub infer_constraints: bool,
    pub show_constraints: bool,
    /// A driving dimension the smart-dimension tool just created and wants
    /// the view layer to open its inline value editor on. The view moves it
    /// into `UiState::editing_dim` on the next frame and clears it here.
    pub pending_dim_edit: Option<SketchConstraint>,
    /// Last plot window picked on canvas (sorted world corners); used by
    /// the Plot dialog's "Window" area mode until re-picked.
    pub plot_window: Option<(f64, f64, f64, f64)>,
    /// Whether the Plot dialog is showing. Typed here (not egui temp-data)
    /// so both the dialog and the canvas overlay read one source of truth,
    /// and a finished window pick can reopen it directly.
    pub plot_dialog_open: bool,
    /// Plot area mode: `true` plots the picked [`Self::plot_window`],
    /// `false` the full drawing extents.
    pub plot_window_mode: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiPrefs {
    pub snap_on: bool,
    pub grid_on: bool,
    pub grid_snap_on: bool,
    pub ortho_on: bool,
    pub polar_on: bool,
    pub track_on: bool,
    pub dyn_on: bool,
    pub comb_on: bool,
    pub comb_scale: f64,
    pub snap_px: f64,
    pub polar_step: f64,
    pub zoom_speed: f64,
    pub zoom_to_cursor: bool,
    pub invert_zoom: bool,
    pub crosshair: bool,
    pub pick_box: f64,
    pub show_lineweights: bool,
    pub lineweight_scale: f64,
    pub grid_dots: bool,
    pub grid_major_every: u32,
    pub grid_minor_rgb: (u8, u8, u8),
    pub grid_major_rgb: (u8, u8, u8),
    pub text_font: Option<String>,
    pub infer_constraints: bool,
    pub show_constraints: bool,
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs {
            snap_on: true,
            grid_on: true,
            grid_snap_on: false,
            ortho_on: false,
            polar_on: true,
            track_on: true,
            dyn_on: true,
            comb_on: false,
            comb_scale: 5.0,
            snap_px: 12.0,
            polar_step: 45.0,
            zoom_speed: 1.0,
            zoom_to_cursor: true,
            invert_zoom: false,
            crosshair: true,
            pick_box: 11.0,
            show_lineweights: true,
            lineweight_scale: 5.0,
            grid_dots: false,
            grid_major_every: 5,
            grid_minor_rgb: (24, 28, 36),
            grid_major_rgb: (33, 39, 49),
            text_font: None,
            infer_constraints: true,
            show_constraints: true,
        }
    }
}

fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let p: Vec<u8> = s.split(',').filter_map(|v| v.trim().parse().ok()).collect();
    (p.len() == 3).then(|| (p[0], p[1], p[2]))
}

impl UiPrefs {
    pub fn serialize(&self) -> String {
        let b = |v: bool| if v { "1" } else { "0" };
        let rgb = |c: (u8, u8, u8)| format!("{},{},{}", c.0, c.1, c.2);
        let font = self.text_font.as_deref().unwrap_or("");
        format!(
            "snap={}\ngrid={}\ngsnap={}\northo={}\npolar={}\ntrack={}\ndyn={}\ncomb={}\ncomb_scale={}\nsnap_px={}\npolar_step={}\nzoom_speed={}\nzoom_cursor={}\ninvert_zoom={}\ncrosshair={}\npick_box={}\nlw_show={}\nlw_scale={}\ngrid_dots={}\ngrid_major={}\ngrid_minor={}\ngrid_majorc={}\nfont={}\ninfer_con={}\nshow_con={}\n",
            b(self.snap_on),
            b(self.grid_on),
            b(self.grid_snap_on),
            b(self.ortho_on),
            b(self.polar_on),
            b(self.track_on),
            b(self.dyn_on),
            b(self.comb_on),
            self.comb_scale,
            self.snap_px,
            self.polar_step,
            self.zoom_speed,
            b(self.zoom_to_cursor),
            b(self.invert_zoom),
            b(self.crosshair),
            self.pick_box,
            b(self.show_lineweights),
            self.lineweight_scale,
            b(self.grid_dots),
            self.grid_major_every,
            rgb(self.grid_minor_rgb),
            rgb(self.grid_major_rgb),
            font,
            b(self.infer_constraints),
            b(self.show_constraints),
        )
    }

    pub fn deserialize(s: &str) -> Self {
        let mut p = UiPrefs::default();
        for line in s.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let on = v == "1";
            match k.trim() {
                "snap" => p.snap_on = on,
                "grid" => p.grid_on = on,
                "gsnap" => p.grid_snap_on = on,
                "ortho" => p.ortho_on = on,
                "polar" => p.polar_on = on,
                "track" => p.track_on = on,
                "dyn" => p.dyn_on = on,
                "comb" => p.comb_on = on,
                "comb_scale" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.comb_scale = f;
                    }
                }
                "snap_px" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.snap_px = f.clamp(2.0, 40.0);
                    }
                }
                "polar_step" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.polar_step = f.clamp(1.0, 90.0);
                    }
                }
                "zoom_speed" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.zoom_speed = f.clamp(0.25, 4.0);
                    }
                }
                "zoom_cursor" => p.zoom_to_cursor = on,
                "invert_zoom" => p.invert_zoom = on,
                "crosshair" => p.crosshair = on,
                "pick_box" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.pick_box = f.clamp(5.0, 30.0);
                    }
                }
                "lw_show" => p.show_lineweights = on,
                "lw_scale" => {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        p.lineweight_scale = f.clamp(1.0, 20.0);
                    }
                }
                "grid_dots" => p.grid_dots = on,
                "grid_major" => {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        p.grid_major_every = n.clamp(2, 20);
                    }
                }
                "grid_minor" => {
                    if let Some(c) = parse_rgb(v) {
                        p.grid_minor_rgb = c;
                    }
                }
                "grid_majorc" => {
                    if let Some(c) = parse_rgb(v) {
                        p.grid_major_rgb = c;
                    }
                }
                "font" => p.text_font = (!v.is_empty()).then(|| v.to_string()),
                "infer_con" => p.infer_constraints = on,
                "show_con" => p.show_constraints = on,
                _ => {}
            }
        }
        p
    }
}

impl AppState {
    pub fn ui_prefs(&self) -> UiPrefs {
        UiPrefs {
            snap_on: self.snap_on,
            grid_on: self.grid_on,
            grid_snap_on: self.grid_snap_on,
            ortho_on: self.ortho_on,
            polar_on: self.polar_on,
            track_on: self.track_on,
            dyn_on: self.dyn_on,
            comb_on: self.comb_on,
            comb_scale: self.comb_scale,
            snap_px: self.snap_px,
            polar_step: self.polar_step,
            zoom_speed: self.zoom_speed,
            zoom_to_cursor: self.zoom_to_cursor,
            invert_zoom: self.invert_zoom,
            crosshair: self.crosshair,
            pick_box: self.pick_box,
            show_lineweights: self.show_lineweights,
            lineweight_scale: self.lineweight_scale,
            grid_dots: self.grid_dots,
            grid_major_every: self.grid_major_every,
            grid_minor_rgb: self.grid_minor_rgb,
            grid_major_rgb: self.grid_major_rgb,
            text_font: self.text_font.clone(),
            infer_constraints: self.infer_constraints,
            show_constraints: self.show_constraints,
        }
    }

    pub fn apply_prefs(&mut self, p: &UiPrefs) {
        self.snap_on = p.snap_on;
        self.grid_on = p.grid_on;
        self.grid_snap_on = p.grid_snap_on;
        self.ortho_on = p.ortho_on;
        self.polar_on = p.polar_on;
        self.track_on = p.track_on;
        self.dyn_on = p.dyn_on;
        self.comb_on = p.comb_on;
        self.comb_scale = p.comb_scale;
        self.snap_px = p.snap_px;
        self.polar_step = p.polar_step;
        self.zoom_speed = p.zoom_speed;
        self.zoom_to_cursor = p.zoom_to_cursor;
        self.invert_zoom = p.invert_zoom;
        self.crosshair = p.crosshair;
        self.pick_box = p.pick_box;
        self.show_lineweights = p.show_lineweights;
        self.lineweight_scale = p.lineweight_scale;
        self.grid_dots = p.grid_dots;
        self.grid_major_every = p.grid_major_every;
        self.grid_minor_rgb = p.grid_minor_rgb;
        self.grid_major_rgb = p.grid_major_rgb;
        self.text_font = p.text_font.clone();
        self.infer_constraints = p.infer_constraints;
        self.show_constraints = p.show_constraints;
        if self.ortho_on {
            self.polar_on = false;
        }
    }
}

#[derive(Default)]
pub struct InteractionState {
    pub grip_drag: Option<GripDrag>,
    pub bbox_drag: Option<BboxDrag>,
    pub corner_action: Option<CornerAction>,
    pub active_guide: Option<((f64, f64), f64)>,
    pub active_guides: Vec<Guide>,
    /// Snap used for the pending start point of the Line tool, so a
    /// finished segment can infer coincident constraints for both ends.
    pub line_snap_prev: Option<SnapPoint>,
    /// Previous segment of the active Line chain; consecutive segments
    /// share a corner by construction and get welded coincident.
    pub line_chain_prev: Option<EntityId>,
    /// First segment of the active Line chain, so a segment that ends back
    /// on the chain's start point closes the loop with a coincident weld.
    pub line_chain_first: Option<EntityId>,
}

#[derive(Clone, Debug)]
pub struct GripDrag {
    pub entity_id: EntityId,
    pub grip: Grip,
    pub start_kind: EntityKind,
}

#[derive(Clone, Debug)]
pub struct BboxDrag {
    pub handle: BboxHandle,
    pub bbox_start: oxidraft_geometry::BoundingBox,
    pub cursor_start: (f64, f64),
    pub originals: Vec<(EntityId, EntityKind)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BboxHandle {
    Body,
    CornerNW,
    CornerNE,
    CornerSW,
    CornerSE,
    RotateNW,
    RotateNE,
    RotateSW,
    RotateSE,
}

fn seed_default_layers(doc: &mut oxidraft_document::Document) {
    use oxidraft_document::Layer;
    for layer in [
        Layer::new("Dimensions").with_color(46, 204, 113),
        Layer::new("Centerlines")
            .with_color(232, 134, 108)
            .with_line_type("Center"),
        Layer::new("Construction")
            .with_color(169, 140, 255)
            .with_line_type("Dotted"),
        Layer::new("Hidden")
            .with_color(150, 160, 178)
            .with_line_type("Dashed"),
    ] {
        doc.layers.add(layer);
    }
}

/// Adds the origin marker and records it as permanently `Fixed` in the
/// constraint solver — not just protected from Select/Erase/Transform at the
/// UI layer (see `fixed_origin_test`), but a real anchor a coincident weld
/// to it can hold onto under solving, the same as welding to any other
/// entity would.
fn add_origin_point(doc: &mut oxidraft_document::Document) -> EntityId {
    let origin_id = doc.add(EntityKind::Point(Point2d::from_i64(0, 0)));
    doc.add_constraint(oxidraft_document::SketchConstraint::fixed(origin_id));
    origin_id
}

/// Whether the entity is a circular arc or full circle — the geometry the
/// smart-dimension tool drives with a radius rather than a length.
fn arc_or_circle(doc: &Document, id: EntityId) -> bool {
    matches!(doc.get(id).and_then(|e| e.as_curve()), Some(Curve::Arc(_)))
}

/// Whether both entities are lines pointing the same (or opposite) way —
/// the pair the smart-dimension tool reads as "the width between them"
/// rather than an angle. The tolerance is deliberately snug: lines made
/// parallel by a constraint or drawn axis-aligned pass, hand-drawn
/// almost-parallel lines still read as an angle pick.
pub(crate) fn lines_parallel(doc: &Document, a: EntityId, b: EntityId) -> bool {
    let line = |id| match doc.get(id).and_then(|e| e.as_curve()) {
        Some(Curve::Line(l)) => Some(l.clone()),
        _ => None,
    };
    let (Some(la), Some(lb)) = (line(a), line(b)) else {
        return false;
    };
    let (ux, uy) = (la.p1.x - la.p0.x, la.p1.y - la.p0.y);
    let (vx, vy) = (lb.p1.x - lb.p0.x, lb.p1.y - lb.p0.y);
    let n = (ux.hypot(uy) * vx.hypot(vy)).max(1e-12);
    ((ux * vy - uy * vx) / n).abs() < 1e-7
}

fn line_endpoints(kind: &EntityKind) -> Option<((f64, f64), (f64, f64))> {
    match kind {
        EntityKind::Curve(Curve::Line(l)) => Some((l.p0.to_f64(), l.p1.to_f64())),
        _ => None,
    }
}

/// Endpoints of the entity kinds the constraint solver can hold onto: a
/// line's two ends, or a partial arc's start/end (index order matches the
/// solver's arc endpoint convention: 0 = start, 1 = end).
fn segment_endpoints(kind: &EntityKind) -> Option<[(f64, f64); 2]> {
    match kind {
        EntityKind::Curve(Curve::Line(l)) => Some([l.p0.to_f64(), l.p1.to_f64()]),
        EntityKind::Curve(Curve::Arc(a)) => {
            if (a.end_angle - a.start_angle).abs() >= std::f64::consts::TAU - 1e-9 {
                return None;
            }
            Some([a.start_point(), a.end_point()])
        }
        _ => None,
    }
}

impl AppState {
    pub fn new(canvas_w: f64, canvas_h: f64) -> Self {
        let mut document = Document::new();
        seed_default_layers(&mut document);
        let origin_id = add_origin_point(&mut document);

        AppState {
            document,
            view: ViewTransform::new(canvas_w, canvas_h),
            tool: Tool::Select,
            selection: Vec::new(),
            snap: SnapSettings::default(),
            snap_on: true,
            grid_on: true,
            grid_snap_on: false,
            ortho_on: false,
            polar_on: true,
            track_on: true,
            dyn_on: true,
            last_command: None,
            history: History::new(),
            command_log: Vec::new(),
            cursor_world: (0.0, 0.0),
            active_snap: None,
            click_count: 0,
            origin_id,
            interaction: InteractionState::default(),

            current_file_path: None,
            text_font: None,
            hatch_pattern: oxidraft_document::HatchPattern::Solid,
            saved_revision: 0,
            zoom_target: None,
            default_line_type: LineTypeRef::ByLayer,
            default_line_weight: LineWeight::ByLayer,
            comb_on: false,
            comb_scale: 5.0,
            snap_px: 12.0,
            polar_step: 45.0,
            zoom_speed: 1.0,
            zoom_to_cursor: true,
            invert_zoom: false,
            crosshair: true,
            pick_box: 11.0,
            show_lineweights: true,
            lineweight_scale: 5.0,
            grid_dots: false,
            grid_major_every: 5,
            grid_minor_rgb: (24, 28, 36),
            grid_major_rgb: (33, 39, 49),
            clipboard: Vec::new(),
            hint_tool: None,
            infer_constraints: true,
            show_constraints: true,
            pending_dim_edit: None,
            plot_window: None,
            plot_dialog_open: false,
            plot_window_mode: false,
        }
    }

    pub fn has_selection(&self) -> bool {
        self.selection.iter().any(|&id| id != self.origin_id)
    }

    pub fn clipboard_copy(&mut self) -> usize {
        let items: Vec<Entity> = self
            .selection
            .iter()
            .filter(|&&id| id != self.origin_id)
            .filter_map(|&id| self.document.get(id).cloned())
            .collect();
        let n = items.len();
        if n > 0 {
            self.clipboard = items;
        }
        n
    }

    pub fn clipboard_cut(&mut self) {
        if self.clipboard_copy() > 0 {
            self.erase_selection();
        }
    }

    pub fn clipboard_paste(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let bbox = self
            .clipboard
            .iter()
            .filter_map(|e| e.bounding_box())
            .reduce(|a, b| a.union(&b));
        let (dx, dy) = match bbox {
            Some(bb) => {
                let cx = (bb.min.x + bb.max.x) * 0.5;
                let cy = (bb.min.y + bb.max.y) * 0.5;
                (self.cursor_world.0 - cx, self.cursor_world.1 - cy)
            }
            None => (0.0, 0.0),
        };
        let t = oxidraft_geometry::Transform2d::translation(dx, dy);
        self.history.snapshot(&self.document);
        let mut pasted = Vec::with_capacity(self.clipboard.len());
        for e in &self.clipboard {
            let mut copy = e.clone();
            copy.transform(&t);
            pasted.push(self.document.add_entity(copy));
        }
        self.selection = pasted;
        self.tool = Tool::Select;
    }

    fn apply_new_entity_defaults(&mut self, id: EntityId) {
        let (lt, lw) = (
            self.default_line_type.clone(),
            self.default_line_weight.clone(),
        );
        let is_dim = matches!(
            self.document.get(id).map(|e| &e.kind),
            Some(
                oxidraft_document::EntityKind::Dimension { .. }
                    | oxidraft_document::EntityKind::OrthoDim { .. }
                    | oxidraft_document::EntityKind::AngularDim { .. }
                    | oxidraft_document::EntityKind::RadialDim { .. }
            )
        );
        let dim_layer = is_dim.then(|| {
            self.document.layers.add(
                oxidraft_document::Layer::new(oxidraft_document::DIMENSION_LAYER)
                    .with_color(46, 204, 113),
            )
        });
        if let Some(e) = self.document.get_mut(id) {
            e.line_type = lt;
            e.line_weight = lw;
            if let Some(layer) = dim_layer {
                e.layer = layer;
            }
        }
    }

    pub fn pointer_moved(&mut self, sx: f64, sy: f64) {
        let (wx, wy) = self.view.screen_to_world(sx, sy);

        let dragged_entity = self.interaction.grip_drag.as_ref().map(|d| d.entity_id);
        let allow_snap = self.tool.wants_point_snap() || dragged_entity.is_some();

        self.active_snap = if self.snap_on && allow_snap {
            let mut s = self.snap.clone();
            s.tolerance = self.view.pixel_world_size() * self.snap_px;
            let ref_pt = self.tool.reference_point().map(|p| p.to_f64());
            let doc_snap = match dragged_entity {
                Some(ex) => find_snaps_excluding(&self.document, (wx, wy), &s, ref_pt, Some(ex))
                    .into_iter()
                    .next(),
                None => best_snap(&self.document, (wx, wy), &s, ref_pt),
            };
            let self_snap = self.nearest_self_snap((wx, wy), s.tolerance);
            match (doc_snap, self_snap) {
                (Some(a), Some(b)) => {
                    let da = (a.pos.0 - wx).hypot(a.pos.1 - wy);
                    let db = (b.pos.0 - wx).hypot(b.pos.1 - wy);
                    Some(if db < da { b } else { a })
                }
                (a, b) => a.or(b),
            }
        } else {
            None
        };

        self.interaction.active_guide = None;
        self.interaction.active_guides.clear();

        if let Some(ref sp) = self.active_snap {
            self.cursor_world = sp.pos;
        } else if self.grid_snap_on && allow_snap {
            self.cursor_world = self.view.snap_to_grid(wx, wy);
        } else if self.ortho_on {
            if let Some(ref_pt) = self.tool.reference_point() {
                let (rx, ry) = ref_pt.to_f64();
                let dx = wx - rx;
                let dy = wy - ry;
                let angle_rad = if dx.abs() >= dy.abs() {
                    self.cursor_world = (wx, ry);
                    if wx >= rx { 0.0 } else { std::f64::consts::PI }
                } else {
                    self.cursor_world = (rx, wy);
                    if wy >= ry {
                        std::f64::consts::FRAC_PI_2
                    } else {
                        -std::f64::consts::FRAC_PI_2
                    }
                };
                self.interaction.active_guide = Some(((rx, ry), angle_rad));
            } else {
                self.cursor_world = (wx, wy);
            }
        } else {
            if let Some(ref_pt) = self.tool.reference_point() {
                let (rx, ry) = ref_pt.to_f64();
                let dx = wx - rx;
                let dy = wy - ry;
                let dist = (dx * dx + dy * dy).sqrt();
                if self.polar_on && dist > 1e-4 {
                    // Angular capture zone for locking onto a polar guide —
                    // kept tight so a nearby object snap (checked above,
                    // before this branch even runs) isn't fought over by a
                    // guide that grabs the cursor too eagerly.
                    const POLAR_CAPTURE_DEG: f64 = 1.5;
                    let angle_rad = dy.atan2(dx);
                    let angle_deg_wrapped = oxidraft_geometry::wrap_deg360(angle_rad.to_degrees());
                    let step = self.polar_step.max(1.0);
                    let nearest = (angle_deg_wrapped / step).round() * step;
                    let diff = (angle_deg_wrapped - nearest).abs();
                    let diff = diff.min(360.0 - diff);

                    if diff <= POLAR_CAPTURE_DEG {
                        let snapped_rad = nearest.to_radians();
                        self.cursor_world =
                            (rx + dist * snapped_rad.cos(), ry + dist * snapped_rad.sin());
                        self.interaction.active_guide = Some(((rx, ry), snapped_rad));
                    } else {
                        self.cursor_world = (wx, wy);
                    }
                } else {
                    self.cursor_world = (wx, wy);
                }
            } else {
                self.cursor_world = (wx, wy);
            }
        }

        if self.track_on
            && self.active_snap.is_none()
            && let Some(drag) = self.interaction.grip_drag.as_ref()
            && let Some((a, b)) = line_endpoints(&drag.start_kind)
        {
            let tol = self.view.pixel_world_size() * 10.0;
            if let Some(res) = infer_axis(a, b, (wx, wy), tol) {
                self.cursor_world = res.point;
                self.interaction.active_guides = res.guides;
            }
        }
    }

    fn nearest_self_snap(&self, cursor: (f64, f64), tol: f64) -> Option<SnapPoint> {
        if !self
            .snap
            .enabled
            .contains(&oxidraft_cad::SnapKind::Endpoint)
        {
            return None;
        }
        let mut best = MinTracker::new();
        for p in self.tool.in_progress_points() {
            let (px, py) = p.to_f64();
            let d = (px - cursor.0).hypot(py - cursor.1);
            if d <= tol {
                best.offer(d, (px, py));
            }
        }
        best.value().map(|pos| SnapPoint {
            kind: oxidraft_cad::SnapKind::Endpoint,
            pos,
            entity: self.origin_id,
        })
    }

    pub fn resolved_point(&self) -> Point2d {
        match &self.active_snap {
            Some(sp) => Point2d::from_f64(sp.pos.0, sp.pos.1),
            None => Point2d::from_f64(self.cursor_world.0, self.cursor_world.1),
        }
    }

    pub fn canvas_click(&mut self, sx: f64, sy: f64) {
        self.click_count = self.click_count.wrapping_add(1);
        self.pointer_moved(sx, sy);
        let p = self.resolved_point();

        if self.handle_modify_click(&p) {
            return;
        }

        if let Tool::Text { anchor, height } = &self.tool {
            let height = *height;
            let need_anchor = anchor.is_none();
            if need_anchor {
                self.tool = Tool::Text {
                    anchor: Some(p),
                    height,
                };
            }
            return;
        }

        if matches!(self.tool, Tool::Select) {
            // Constraint badges sit on top of the drawing: a click on a
            // chip or weld dot deletes what it shows, undoably. Driving
            // dimensions are exempt — a click there opens their value
            // editor (handled in the view layer), never a silent delete.
            if let Some(hits) = crate::view::overlays::badge_hit(self, sx, sy) {
                let hits: Vec<SketchConstraint> =
                    hits.into_iter().filter(|c| !c.kind.is_valued()).collect();
                if !hits.is_empty() {
                    self.history.snapshot(&self.document);
                    self.document.constraints.retain(|c| !hits.contains(c));
                    self.command_log.push(if hits.len() == 1 {
                        "Removed constraint via its badge".into()
                    } else {
                        format!("Removed {} constraints via their badge", hits.len())
                    });
                    return;
                }
            }
            if let Some(id) = pick_at(&self.document, p.x, p.y, self.view.pixel_world_size() * 6.0)
            {
                self.toggle_selection(id);
            } else {
                self.selection.clear();
            }
            return;
        }

        if self.try_close_on_start(p) {
            self.interaction.line_snap_prev = None;
            self.interaction.line_chain_prev = None;
            self.interaction.line_chain_first = None;
            return;
        }

        let was_line = matches!(self.tool, Tool::Line { .. });
        let was_arc = matches!(
            self.tool,
            Tool::Arc3 { .. } | Tool::ArcStartCenterEnd { .. } | Tool::ArcCenterStartEnd { .. }
        );
        let snap_now = self.active_snap.clone();
        let ev = self.tool.on_point(p);
        let created = matches!(ev, ToolEvent::Create(_));
        self.apply_tool_event(ev);
        if was_line {
            self.after_line_point(created, snap_now.as_ref(), snap_now.is_some());
            self.interaction.line_snap_prev = snap_now;
        } else if was_arc && created {
            self.after_arc_create();
        }
    }

    pub fn place_tool_point(&mut self, p: Point2d) {
        if self.try_close_on_start(p) {
            self.interaction.line_snap_prev = None;
            self.interaction.line_chain_prev = None;
            self.interaction.line_chain_first = None;
            return;
        }
        let was_line = matches!(self.tool, Tool::Line { .. });
        let was_arc = matches!(
            self.tool,
            Tool::Arc3 { .. } | Tool::ArcStartCenterEnd { .. } | Tool::ArcCenterStartEnd { .. }
        );
        let ev = self.tool.on_point(p);
        let created = matches!(ev, ToolEvent::Create(_));
        self.apply_tool_event(ev);
        if was_line {
            // Typed points are exact, not snapped: the start of the segment
            // may still have come from a snap, but this end did not. The
            // typed end is authoritative, so axis inference must not move it.
            self.after_line_point(created, None, true);
            self.interaction.line_snap_prev = None;
        } else if was_arc && created {
            self.after_arc_create();
        }
    }

    /// Constraint inference bookkeeping after the Line tool consumed a
    /// point. On a finished segment, infers endpoint-snap coincidence,
    /// welds it to the previous segment of the chain (and to the chain's
    /// first segment when it closes the loop), and infers horizontal/
    /// vertical alignment. `end_pinned` marks an end the user placed
    /// exactly (snap or typed) that axis inference must not move.
    fn after_line_point(&mut self, created: bool, end_snap: Option<&SnapPoint>, end_pinned: bool) {
        if !created {
            self.interaction.line_chain_prev = None;
            self.interaction.line_chain_first = None;
            return;
        }
        let start_snap = self.interaction.line_snap_prev.take();
        let chain_prev = self.interaction.line_chain_prev;
        let Some(&new_id) = self.document.order.last() else {
            return;
        };
        self.infer_line_coincidence(new_id, start_snap.as_ref(), end_snap);
        match chain_prev {
            Some(prev_seg) => self.weld_chain_segments(prev_seg, new_id),
            None => self.interaction.line_chain_first = Some(new_id),
        }
        let closed = self.weld_chain_closure(new_id);
        let tangent = self.infer_arc_tangency(
            new_id,
            start_snap.as_ref(),
            end_snap,
            start_snap.is_some() || chain_prev.is_some(),
            end_pinned || closed,
        );
        self.infer_axis_alignment(new_id, end_pinned || closed || tangent);
        self.interaction.line_chain_prev = Some(new_id);
    }

    /// Welds consecutive Line-chain segments: the new segment starts exactly
    /// where the previous one ended.
    fn weld_chain_segments(&mut self, prev: EntityId, new_id: EntityId) {
        if !self.infer_constraints {
            return;
        }
        let (Some(lp), Some(ln)) = (
            self.document
                .get(prev)
                .and_then(|e| line_endpoints(&e.kind)),
            self.document
                .get(new_id)
                .and_then(|e| line_endpoints(&e.kind)),
        ) else {
            return;
        };
        let ((_, p1), (n0, _)) = (lp, ln);
        if (p1.0 - n0.0).hypot(p1.1 - n0.1) > 1e-9 {
            return;
        }
        self.document
            .add_constraint(oxidraft_document::SketchConstraint::coincident(
                prev, 1, new_id, 0,
            ));
    }

    /// Welds the closing segment of a Line chain: when the new segment ends
    /// exactly back on the chain's first point (endpoint snap and typed
    /// closes are exact), the loop corner is recorded coincident. Returns
    /// whether the chain closed, which pins the end for axis inference.
    fn weld_chain_closure(&mut self, new_id: EntityId) -> bool {
        if !self.infer_constraints {
            return false;
        }
        let Some(first) = self.interaction.line_chain_first else {
            return false;
        };
        if first == new_id {
            return false;
        }
        let (Some(lf), Some(ln)) = (
            self.document
                .get(first)
                .and_then(|e| line_endpoints(&e.kind)),
            self.document
                .get(new_id)
                .and_then(|e| line_endpoints(&e.kind)),
        ) else {
            return false;
        };
        let ((f0, _), (_, n1)) = (lf, ln);
        if (f0.0 - n1.0).hypot(f0.1 - n1.1) > 1e-9 {
            return false;
        }
        if self
            .document
            .add_constraint(oxidraft_document::SketchConstraint::coincident(
                first, 0, new_id, 1,
            ))
        {
            self.command_log
                .push("Chain closed: corner welded coincident".into());
        }
        true
    }

    /// Welds a freshly created batch of chained segments (a Rectangle,
    /// Polygon, or Polyline outline emitted as individual lines): each
    /// consecutive pair sharing an endpoint gets a coincident weld, plus the
    /// closing weld when the chain loops back to its start. The welds are
    /// unconditional — the user drew one connected shape. With
    /// auto-constrain on, the tool's implied shape constraints are added
    /// too: Horizontal/Vertical on a Rectangle's axis-aligned sides, and
    /// EqualLength across a Polygon's sides (so dimensioning one side of a
    /// hexagon drives all six).
    fn weld_created_loop(&mut self, ids: &[EntityId]) {
        self.weld_adjacent_segments(ids);
        if !self.infer_constraints {
            return;
        }
        match self.tool {
            Tool::Rectangle { .. } => {
                // Sides are axis-aligned by construction; still check each
                // one so a degenerate batch can't record a false H/V.
                for &id in ids {
                    let Some([p0, p1]) = self
                        .document
                        .get(id)
                        .and_then(|e| segment_endpoints(&e.kind))
                    else {
                        continue;
                    };
                    let kind = if (p0.1 - p1.1).abs() < 1e-9 {
                        ConstraintKind::Horizontal
                    } else if (p0.0 - p1.0).abs() < 1e-9 {
                        ConstraintKind::Vertical
                    } else {
                        continue;
                    };
                    self.document
                        .add_constraint(SketchConstraint::single(kind, id));
                }
            }
            Tool::Polygon { .. } => {
                for &id in &ids[1..] {
                    self.document.add_constraint(SketchConstraint::pair(
                        ConstraintKind::EqualLength,
                        ids[0],
                        id,
                    ));
                }
            }
            _ => {}
        }
    }

    /// Records the constraints a fillet/chamfer corner implies: the new
    /// connector is welded coincident to each trimmed source at the shared
    /// endpoints, and a fillet arc is additionally tangent to its line
    /// sources. This is what lets a filleted corner stay smooth when a leg
    /// is dragged later.
    pub(crate) fn record_corner_constraints(
        &mut self,
        sources: [EntityId; 2],
        new_id: EntityId,
        tangent: bool,
    ) {
        if !self.infer_constraints {
            return;
        }
        let Some(new_ends) = self
            .document
            .get(new_id)
            .and_then(|e| segment_endpoints(&e.kind))
        else {
            return;
        };
        let new_is_arc = matches!(
            self.document.get(new_id).map(|e| &e.kind),
            Some(EntityKind::Curve(Curve::Arc(_)))
        );
        for src in sources {
            let Some(e) = self.document.get(src) else {
                continue;
            };
            let src_constrainable = matches!(
                &e.kind,
                EntityKind::Curve(Curve::Line(_)) | EntityKind::Curve(Curve::Arc(_))
            );
            if let Some(src_ends) = segment_endpoints(&e.kind) {
                for (si, sp) in src_ends.iter().enumerate() {
                    for (ni, np) in new_ends.iter().enumerate() {
                        if (sp.0 - np.0).hypot(sp.1 - np.1) <= 1e-6 {
                            self.document.add_constraint(
                                oxidraft_document::SketchConstraint::coincident(
                                    src, si as u8, new_id, ni as u8,
                                ),
                            );
                        }
                    }
                }
            }
            if tangent && src_constrainable && new_is_arc {
                self.document
                    .add_constraint(oxidraft_document::SketchConstraint::pair(
                        oxidraft_document::ConstraintKind::Tangent,
                        src,
                        new_id,
                    ));
            }
        }
    }

    /// Infers a tangent constraint when a drawn line leaves an arc's
    /// endpoint along the tangent direction — the classic "continue the
    /// curve smoothly" stroke. A near miss (within 3 px of the tangent ray
    /// over the segment, capped at ~3°) is rotated exactly tangent about
    /// the end sitting on the arc — but a pinned far end (snap, typed,
    /// chain weld, or chain close) is authoritative, so only exact tangency
    /// is recorded there. The coincident weld to the arc endpoint is
    /// inference's job too, but it is already recorded by
    /// `infer_line_coincidence` before this runs (and the attached end
    /// never moves here, so the weld stays valid). Returns whether a
    /// tangency was recorded, which pins the segment against axis
    /// inference rotating it afterwards.
    fn infer_arc_tangency(
        &mut self,
        new_id: EntityId,
        start_snap: Option<&SnapPoint>,
        end_snap: Option<&SnapPoint>,
        start_pinned: bool,
        end_pinned: bool,
    ) -> bool {
        if !self.infer_constraints {
            return false;
        }
        let mut recorded = false;
        let cases = [(0u8, start_snap, end_pinned), (1u8, end_snap, start_pinned)];
        for (attached_end, sp, far_pinned) in cases {
            let Some(sp) = sp else { continue };
            if sp.kind != oxidraft_cad::SnapKind::Endpoint || sp.entity == new_id {
                continue;
            }
            let Some(EntityKind::Curve(Curve::Arc(arc))) =
                self.document.get(sp.entity).map(|e| &e.kind)
            else {
                continue;
            };
            if (arc.end_angle - arc.start_angle).abs() >= std::f64::consts::TAU - 1e-9 {
                continue;
            }
            let (s, e) = (arc.start_point(), arc.end_point());
            let ds = (s.0 - sp.pos.0).hypot(s.1 - sp.pos.1);
            let de = (e.0 - sp.pos.0).hypot(e.1 - sp.pos.1);
            if ds.min(de) > 1e-6 {
                continue;
            }
            let theta = if ds <= de {
                arc.start_angle
            } else {
                arc.end_angle
            };
            let arc_id = sp.entity;
            // Reread endpoints each pass: the other end may have attached
            // to a different arc and rotated the line already.
            let Some((p0, p1)) = self
                .document
                .get(new_id)
                .and_then(|e| line_endpoints(&e.kind))
            else {
                return recorded;
            };
            let (att, far) = if attached_end == 0 {
                (p0, p1)
            } else {
                (p1, p0)
            };
            let anchor = if ds <= de { s } else { e };
            if (att.0 - anchor.0).hypot(att.1 - anchor.1) > 1e-6 {
                continue;
            }
            let t = (-theta.sin(), theta.cos());
            let v = (far.0 - att.0, far.1 - att.1);
            let len = v.0.hypot(v.1);
            let px = self.view.pixel_world_size();
            if len < px * 12.0 {
                continue;
            }
            let dev = (t.0 * v.1 - t.1 * v.0).abs();
            if dev > (px * 3.0).min(len * 0.05) {
                continue;
            }
            if dev > 1e-9 {
                if far_pinned {
                    continue;
                }
                let dir = if t.0 * v.0 + t.1 * v.1 >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let new_far = (att.0 + t.0 * len * dir, att.1 + t.1 * len * dir);
                let (q0, q1) = if attached_end == 0 {
                    (att, new_far)
                } else {
                    (new_far, att)
                };
                if let Some(e) = self.document.get_mut(new_id) {
                    e.kind = EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                        Point2d::from_f64(q0.0, q0.1),
                        Point2d::from_f64(q1.0, q1.1),
                    )));
                }
                // The chain continues from wherever the tool last clicked;
                // keep it attached to the rotated end.
                if attached_end == 0
                    && let Tool::Line { last: Some(lp) } = &mut self.tool
                {
                    *lp = Point2d::from_f64(new_far.0, new_far.1);
                }
            }
            if self
                .document
                .add_constraint(oxidraft_document::SketchConstraint::pair(
                    oxidraft_document::ConstraintKind::Tangent,
                    arc_id,
                    new_id,
                ))
            {
                self.command_log
                    .push("Inferred tangent constraint from arc endpoint".into());
            }
            recorded = true;
        }
        recorded
    }

    /// Constraint inference after an arc-draw tool created an arc: the mirror
    /// of `infer_arc_tangency` (which handles a line drawn off an arc). See
    /// [`AppState::infer_arc_onset_tangency`].
    fn after_arc_create(&mut self) {
        let Some(&new_id) = self.document.order.last() else {
            return;
        };
        if matches!(
            self.document.get(new_id).map(|e| &e.kind),
            Some(EntityKind::Curve(Curve::Arc(_)))
        ) {
            self.infer_arc_onset_tangency(new_id);
        }
    }

    /// Infers a tangent constraint (plus the coincident weld that keeps them
    /// attached) when a freshly drawn arc starts or ends exactly on a line's
    /// endpoint and leaves it nearly tangent — the mirror of
    /// `infer_arc_tangency`, which handles a line drawn off an arc. Here the
    /// line is authoritative (it was there first): the arc is pulled exactly
    /// tangent about the shared endpoint by the solver while the line stays
    /// put. Only the first shared end is inferred (a fresh arc off one line);
    /// arc↔arc onset tangency is not covered.
    fn infer_arc_onset_tangency(&mut self, arc_id: EntityId) {
        if !self.infer_constraints {
            return;
        }
        let Some(EntityKind::Curve(Curve::Arc(arc))) = self.document.get(arc_id).map(|e| &e.kind)
        else {
            return;
        };
        if (arc.end_angle - arc.start_angle).abs() >= std::f64::consts::TAU - 1e-9 {
            return;
        }
        let px = self.view.pixel_world_size();
        let ends = [
            (0u8, arc.start_point(), arc.start_angle),
            (1u8, arc.end_point(), arc.end_angle),
        ];
        // Scan (immutably) for the first line whose endpoint coincides with an
        // arc end and lies nearly along the arc's tangent line there. The
        // tangent line at angle theta is (-sin, cos), perpendicular to the
        // radius — the same ray the line-off-arc case levels onto.
        let mut hit: Option<(EntityId, u8, u8)> = None;
        'search: for (arc_end, apos, theta) in ends {
            let t = (-theta.sin(), theta.cos());
            for &id in &self.document.order {
                if id == arc_id {
                    continue;
                }
                let Some((l0, l1)) = self.document.get(id).and_then(|e| line_endpoints(&e.kind))
                else {
                    continue;
                };
                for (line_end, near, far) in [(0u8, l0, l1), (1u8, l1, l0)] {
                    if (near.0 - apos.0).hypot(near.1 - apos.1) > 1e-6 {
                        continue;
                    }
                    let v = (far.0 - apos.0, far.1 - apos.1);
                    let len = v.0.hypot(v.1);
                    if len < px * 12.0 {
                        continue;
                    }
                    let dev = (t.0 * v.1 - t.1 * v.0).abs();
                    if dev > (px * 3.0).min(len * 0.05) {
                        continue;
                    }
                    hit = Some((id, line_end, arc_end));
                    break 'search;
                }
            }
        }
        let Some((line_id, line_end, arc_end)) = hit else {
            return;
        };
        // Record the weld and tangency, then pull the arc exactly tangent with
        // the line pinned where it was drawn. A failed solve drops the pair so
        // the raw arc is left untouched (same policy as the other inferences).
        let prev = self.document.constraints.clone();
        self.document
            .add_constraint(oxidraft_document::SketchConstraint::coincident(
                arc_id, arc_end, line_id, line_end,
            ));
        let recorded = self
            .document
            .add_constraint(oxidraft_document::SketchConstraint::pair(
                oxidraft_document::ConstraintKind::Tangent,
                arc_id,
                line_id,
            ));
        if oxidraft_cad::resolve_after_transform(&mut self.document, &[line_id]) {
            if recorded {
                self.command_log
                    .push("Inferred tangent constraint from arc onset".into());
            }
        } else {
            self.document.constraints = prev;
        }
    }

    /// Infers a horizontal/vertical constraint on a freshly drawn line that
    /// is exactly or nearly axis-aligned. A near miss (within 3 px of level
    /// over the segment, capped at ~3°) is snapped level about its start —
    /// but a pinned end (snap, typed, or chain close) is authoritative, so
    /// only exact alignment is recorded there.
    fn infer_axis_alignment(&mut self, new_id: EntityId, end_pinned: bool) {
        if !self.infer_constraints {
            return;
        }
        let Some((p0, p1)) = self
            .document
            .get(new_id)
            .and_then(|e| line_endpoints(&e.kind))
        else {
            return;
        };
        let (adx, ady) = ((p1.0 - p0.0).abs(), (p1.1 - p0.1).abs());
        let px = self.view.pixel_world_size();
        let len = adx.max(ady);
        if len < px * 12.0 {
            return;
        }
        let slack = (px * 3.0).min(len * 0.05);
        let kind = if ady <= slack && ady < adx {
            oxidraft_document::ConstraintKind::Horizontal
        } else if adx <= slack && adx < ady {
            oxidraft_document::ConstraintKind::Vertical
        } else {
            return;
        };
        let off_axis = match kind {
            oxidraft_document::ConstraintKind::Horizontal => ady,
            _ => adx,
        };
        if off_axis > 1e-9 {
            if end_pinned {
                return;
            }
            let new_p1 = match kind {
                oxidraft_document::ConstraintKind::Horizontal => (p1.0, p0.1),
                _ => (p0.0, p1.1),
            };
            if let Some(e) = self.document.get_mut(new_id) {
                e.kind = EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                    Point2d::from_f64(p0.0, p0.1),
                    Point2d::from_f64(new_p1.0, new_p1.1),
                )));
            }
            // The chain continues from wherever the tool last clicked;
            // keep it attached to the levelled end.
            if let Tool::Line { last: Some(lp) } = &mut self.tool {
                *lp = Point2d::from_f64(new_p1.0, new_p1.1);
            }
        }
        if self
            .document
            .add_constraint(oxidraft_document::SketchConstraint::single(kind, new_id))
        {
            self.command_log.push(format!(
                "Inferred {} constraint on drawn line",
                kind.label()
            ));
        }
    }

    /// Records coincident constraints for a just-drawn line whose endpoints
    /// were placed via Endpoint snaps onto other lines or arcs. This is what
    /// makes corners drawn with snaps stay attached under later edits.
    fn infer_line_coincidence(
        &mut self,
        new_id: EntityId,
        p0_snap: Option<&SnapPoint>,
        p1_snap: Option<&SnapPoint>,
    ) {
        if !self.infer_constraints {
            return;
        }
        let mut inferred = 0;
        for (my_end, sp) in [(0u8, p0_snap), (1u8, p1_snap)] {
            let Some(sp) = sp else { continue };
            if sp.kind != oxidraft_cad::SnapKind::Endpoint
                || sp.entity == new_id
                || sp.entity == self.origin_id
            {
                continue;
            }
            let Some(target) = self.document.get(sp.entity) else {
                continue;
            };
            let Some([t0, t1]) = segment_endpoints(&target.kind) else {
                continue;
            };
            let d0 = (t0.0 - sp.pos.0).hypot(t0.1 - sp.pos.1);
            let d1 = (t1.0 - sp.pos.0).hypot(t1.1 - sp.pos.1);
            if d0.min(d1) > 1e-6 {
                continue;
            }
            let their_end = if d0 <= d1 { 0u8 } else { 1u8 };
            if self
                .document
                .add_constraint(oxidraft_document::SketchConstraint::coincident(
                    sp.entity, their_end, new_id, my_end,
                ))
            {
                inferred += 1;
            }
        }
        if inferred > 0 {
            self.command_log.push(format!(
                "Inferred {inferred} coincident constraint(s) from endpoint snap"
            ));
        }
    }

    /// Commits the pending polygon (center + radius both picked, popup
    /// showing) with whatever side count is currently set. No-op outside
    /// that pending state.
    pub fn confirm_pending_polygon(&mut self) {
        if !matches!(
            self.tool,
            Tool::Polygon {
                center: Some(_),
                radius_point: Some(_),
                ..
            }
        ) {
            return;
        }
        let ev = self.tool.commit();
        self.apply_tool_event(ev);
    }

    /// Drops the pending polygon pick (both clicks made, popup showing)
    /// without committing, returning to "click the center point" — the last
    /// side count used is kept for next time.
    pub fn cancel_pending_polygon(&mut self) {
        if let Tool::Polygon { sides, .. } = self.tool.clone() {
            self.tool = Tool::Polygon {
                center: None,
                radius_point: None,
                sides,
            };
        }
    }

    fn try_close_on_start(&mut self, p: Point2d) -> bool {
        let close = match &self.tool {
            Tool::Polyline { pts } | Tool::Spline { pts } => {
                pts.len() >= 3 && pts[0].dist_f64(&p) <= self.view.pixel_world_size() * self.snap_px
            }
            _ => false,
        };
        if close {
            let ev = self.tool.close_and_commit();
            self.apply_tool_event(ev);
        }
        close
    }

    /// True when `t` is non-finite: logs the reason and returns to Select
    /// so the caller can bail before snapshotting. The geometry itself is
    /// already protected by `Entity::transform`'s floor; this is the UX
    /// layer (don't record a no-op undo step, tell the user why).
    fn reject_nonfinite_transform(&mut self, t: &Transform2d) -> bool {
        if t.is_finite() {
            return false;
        }
        self.command_log
            .push("Transform undefined for those picks — nothing changed".into());
        self.tool = Tool::Select;
        true
    }

    fn apply_tool_event(&mut self, ev: ToolEvent) {
        match ev {
            ToolEvent::Pending => {}
            ToolEvent::PlotWindow(a, b) => {
                // Doesn't touch the document — no history snapshot. Validate
                // through the same predicate the exporter uses, rather than
                // re-deriving the sort/finite/area check here.
                let win = oxidraft_io::PlotWindow {
                    x0: a.x,
                    y0: a.y,
                    x1: b.x,
                    y1: b.y,
                };
                match win.normalized() {
                    Some(corners) => {
                        self.plot_window = Some(corners);
                        self.command_log.push("Plot window set".into());
                    }
                    None => self
                        .command_log
                        .push("Plot window has no area — pick two different corners".into()),
                }
                // Reopen the dialog in Window mode directly — no cross-frame
                // flag to translate.
                self.plot_dialog_open = true;
                self.plot_window_mode = true;
            }
            ToolEvent::Create(kinds) => {
                self.history.snapshot(&self.document);
                let mut created = Vec::with_capacity(kinds.len());
                for k in kinds {
                    let id = self.document.add(k);
                    self.apply_new_entity_defaults(id);
                    created.push(id);
                }
                // A shape tool that emits its outline as a batch of lines
                // (Rectangle, Polygon, Polyline) drew one connected figure:
                // weld its corners so it behaves as joined geometry. The
                // welds are structural facts of the drawn shape — recorded
                // regardless of the auto-constrain toggle, which only gates
                // the *inferred* extras added inside `weld_created_loop`.
                if created.len() >= 2
                    && matches!(
                        self.tool,
                        Tool::Rectangle { .. } | Tool::Polygon { .. } | Tool::Polyline { .. }
                    )
                {
                    self.weld_created_loop(&created);
                }
            }
            ToolEvent::Transform { ids, t } => {
                // A degenerate gesture (mirror across a single point, NaN
                // cursor) yields a non-finite transform: skip the wasted
                // history snapshot and tell the user, rather than record a
                // no-op step. (Entity::transform floors the geometry.)
                if self.reject_nonfinite_transform(&t) {
                    return;
                }
                self.history.snapshot(&self.document);
                let mut moved = Vec::new();
                for id in ids {
                    if id != self.origin_id
                        && let Some(e) = self.document.get_mut(id)
                    {
                        e.transform(&t);
                        moved.push(id);
                    }
                }
                if !oxidraft_cad::resolve_after_transform(&mut self.document, &moved) {
                    self.command_log.push(
                        "Constraints not satisfiable after transform (UNCONSTRAIN to drop)".into(),
                    );
                }
                self.selection = moved;
                self.tool = Tool::Select;
            }
            ToolEvent::CopyOf { ids, t } => {
                // Same contract as Transform above.
                if self.reject_nonfinite_transform(&t) {
                    return;
                }
                self.history.snapshot(&self.document);
                let mut new_ids = Vec::new();
                for id in ids {
                    if id != self.origin_id
                        && let Some(e) = self.document.get(id)
                    {
                        let copy = e.transformed(&t);
                        new_ids.push(self.document.add_entity(copy));
                    }
                }
                self.selection = new_ids;
                self.tool = Tool::Select;
            }
        }
    }

    fn toggle_selection(&mut self, id: EntityId) {
        if id == self.origin_id {
            return;
        }
        if let Some(pos) = self.selection.iter().position(|&s| s == id) {
            self.selection.remove(pos);
        } else {
            self.selection.push(id);
        }
    }

    pub fn run_command(&mut self, text: &str) {
        let trimmed = text.trim();

        if let Tool::Text {
            anchor: Some(p),
            height,
        } = self.tool.clone()
        {
            if !trimmed.is_empty() {
                self.history.snapshot(&self.document);
                self.document.add(EntityKind::Text {
                    anchor: p,
                    content: trimmed.replace("\\n", "\n"),
                    height,
                    rotation: 0.0,
                    font: self.text_font.clone(),
                });
            }
            self.tool = Tool::Select;
            self.command_log.push(trimmed.to_string());
            return;
        }

        if matches!(self.tool, Tool::Polyline { .. } | Tool::Spline { .. }) {
            if trimmed.is_empty() {
                let ev = self.tool.commit();
                self.apply_tool_event(ev);
                self.tool = Tool::Select;
                return;
            }
            let upper = trimmed.to_ascii_uppercase();
            if upper == "C" || upper == "CLOSE" {
                let ev = self.tool.close_and_commit();
                self.apply_tool_event(ev);
                self.tool = Tool::Select;
                self.command_log.push(trimmed.to_string());
                return;
            }
        }

        if let Tool::Polygon { center: None, .. } = self.tool
            && let Ok(n) = trimmed.parse::<usize>()
            && n >= 3
        {
            self.tool = Tool::Polygon {
                center: None,
                radius_point: None,
                sides: Some(n),
            };
            self.command_log.push(trimmed.to_string());
            return;
        }

        if let Ok(v) = trimmed.parse::<f64>()
            && v > 0.0
        {
            match &self.tool {
                Tool::Offset { source, .. } => {
                    self.tool = Tool::Offset {
                        dist: v,
                        source: *source,
                    };
                    self.command_log.push(trimmed.to_string());
                    return;
                }
                Tool::Fillet { first, .. } => {
                    self.tool = Tool::Fillet {
                        radius: v,
                        first: *first,
                    };
                    self.command_log.push(trimmed.to_string());
                    return;
                }
                Tool::Chamfer { first, .. } => {
                    self.tool = Tool::Chamfer {
                        dist: v,
                        first: *first,
                    };
                    self.command_log.push(trimmed.to_string());
                    return;
                }
                Tool::Blend {
                    continuity,
                    first,
                    second,
                    ..
                } => {
                    self.tool = Tool::Blend {
                        continuity: *continuity,
                        tension: v,
                        first: *first,
                        second: *second,
                    };
                    self.command_log.push(trimmed.to_string());
                    return;
                }
                Tool::CircleTtr { first, .. } => {
                    self.tool = Tool::CircleTtr {
                        radius: v,
                        first: *first,
                    };
                    self.command_log.push(trimmed.to_string());
                    return;
                }
                _ => {}
            }
        }

        if let Ok(dist) = trimmed.parse::<f64>()
            && let Some(ref_pt) = self.tool.reference_point()
        {
            let (rx, ry) = ref_pt.to_f64();
            let (cx, cy) = self.cursor_world;
            let dx = cx - rx;
            let dy = cy - ry;
            let len = (dx * dx + dy * dy).sqrt();
            let (ux, uy) = if len > 1e-9 {
                (dx / len, dy / len)
            } else if let Some((_, angle_rad)) = self.interaction.active_guide {
                (angle_rad.cos(), angle_rad.sin())
            } else {
                (1.0, 0.0)
            };
            let target_pt = Point2d::from_f64(rx + dist * ux, ry + dist * uy);
            let ev = self.tool.on_point(target_pt);
            self.apply_tool_event(ev);
            self.command_log.push(trimmed.to_string());
            return;
        }

        if let Some(coord) = parse_coordinate(trimmed) {
            let (rx, ry) = self
                .tool
                .reference_point()
                .map(|p| p.to_f64())
                .unwrap_or((0.0, 0.0));
            let (x, y) = match coord {
                CoordInput::Absolute(x, y) => (x, y),
                CoordInput::Relative(dx, dy) => (rx + dx, ry + dy),
                CoordInput::PolarAbsolute { dist, angle_deg } => {
                    let a = angle_deg.to_radians();
                    (dist * a.cos(), dist * a.sin())
                }
                CoordInput::PolarRelative { dist, angle_deg } => {
                    let a = angle_deg.to_radians();
                    (rx + dist * a.cos(), ry + dist * a.sin())
                }
            };
            let ev = self.tool.on_point(Point2d::from_f64(x, y));
            self.apply_tool_event(ev);
            self.command_log.push(trimmed.to_string());
            return;
        }

        let cmd = parse_command(text);
        self.command_log.push(text.trim().to_string());
        if !matches!(cmd, Command::Cancel | Command::Unknown(_)) {
            self.last_command = Some(trimmed.to_string());
        }
        self.execute(cmd);
    }

    pub fn repeat_last_command(&mut self) {
        if let Some(cmd) = self.last_command.clone() {
            self.run_command(&cmd);
        }
    }

    pub fn execute(&mut self, cmd: Command) {
        match cmd {
            Command::Activate(mut tool) => {
                match &mut tool {
                    Tool::Move { ids, .. }
                    | Tool::Copy { ids, .. }
                    | Tool::Rotate { ids, .. }
                    | Tool::Scale { ids, .. }
                    | Tool::Mirror { ids, .. }
                    | Tool::Stretch { ids, .. } => *ids = self.selection.clone(),
                    _ => {}
                }
                self.tool = tool;
            }
            Command::Cancel => {
                self.tool.reset();
                if matches!(self.tool, Tool::Select) {
                    self.selection.clear();
                }
                self.tool = Tool::Select;
            }
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::Erase => self.erase_selection(),
            Command::Explode => self.explode_selection(),
            Command::Join => self.join_selection(),
            Command::Constrain(kind) => self.constrain_selection(kind),
            Command::ConstrainRadius(value) => self.constrain_radius_selection(value),
            Command::ConstrainDistance(value) => self.constrain_distance_selection(value),
            Command::ConstrainAngle(value) => self.constrain_angle_selection(value),
            Command::Divide(n) => self.divide_selection(n),
            Command::Measure(interval) => self.measure_selection(interval),
            Command::Unconstrain => self.unconstrain_selection(),
            Command::Fix => self.fix_selection(),
            Command::Hatch => {
                if self.selection.is_empty() {
                    self.tool = Tool::Hatch;
                } else {
                    self.hatch_selection();
                }
            }
            Command::SelectAll => {
                self.selection = self
                    .document
                    .iter()
                    .map(|e| e.id)
                    .filter(|&id| id != self.origin_id)
                    .collect();
            }
            Command::ZoomExtents => self.zoom_extents(),
            Command::ZoomScale(s) => {
                self.view.zoom = s.clamp(1e-9, 1e12);
            }
            Command::LayerSet(name) => {
                self.document.layers.set_current(&name);
            }
            Command::LayerNew(name) => {
                let idx = self.document.layers.add(Layer::new(name));
                self.document.layers.current = idx;
            }
            Command::Unknown(_) => {}
        }
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.undo(&self.document) {
            self.document = prev;
            self.selection.clear();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.history.redo(&self.document) {
            self.document = next;
            self.selection.clear();
        }
    }

    pub fn erase_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.history.snapshot(&self.document);
        for id in std::mem::take(&mut self.selection) {
            if id != self.origin_id {
                self.document.remove(id);
            }
        }
    }

    pub fn explode_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.history.snapshot(&self.document);
        let ids: Vec<_> = std::mem::take(&mut self.selection)
            .into_iter()
            .filter(|&id| id != self.origin_id)
            .collect();
        // Explode one source at a time so each polycurve's own segments can
        // be welded as a group: with auto-constrain on, EXPLODE is the
        // migration path that turns an existing polyline/hexagon into
        // constraint-ready lines without letting the shape fall apart.
        let mut new_ids = Vec::new();
        for &id in &ids {
            let group = edit::explode(&mut self.document, &[id]);
            if self.infer_constraints && group.len() >= 2 {
                self.weld_adjacent_segments(&group);
            }
            new_ids.extend(group);
        }
        let survived: Vec<_> = ids
            .into_iter()
            .filter(|&id| self.document.get(id).is_some())
            .collect();
        self.selection = survived.into_iter().chain(new_ids).collect();
    }

    /// Coincident-welds consecutive segments of one exploded polycurve where
    /// their endpoints actually touch, closing the loop when it loops.
    /// Handles lines and partial arcs alike (`segment_endpoints`).
    fn weld_adjacent_segments(&mut self, ids: &[EntityId]) {
        let ends: Vec<Option<[(f64, f64); 2]>> = ids
            .iter()
            .map(|&id| {
                self.document
                    .get(id)
                    .and_then(|e| segment_endpoints(&e.kind))
            })
            .collect();
        let touch = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1) < 1e-9;
        for i in 0..ids.len() - 1 {
            if let (Some(pa), Some(pb)) = (ends[i], ends[i + 1])
                && touch(pa[1], pb[0])
            {
                self.document
                    .add_constraint(oxidraft_document::SketchConstraint::coincident(
                        ids[i],
                        1,
                        ids[i + 1],
                        0,
                    ));
            }
        }
        if ids.len() >= 3
            && let (Some(first), Some(last)) = (ends[0], ends[ids.len() - 1])
            && touch(first[0], last[1])
        {
            self.document
                .add_constraint(oxidraft_document::SketchConstraint::coincident(
                    ids[0],
                    0,
                    ids[ids.len() - 1],
                    1,
                ));
        }
    }

    pub fn hatch_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        let fill = self.document.layers.current_layer().color;
        let loops: Vec<Vec<Curve>> = self
            .selection
            .iter()
            .filter(|&&id| id != self.origin_id)
            .filter_map(|&id| self.document.get(id).and_then(oxidraft_cad::boundary_loop))
            .collect();
        if loops.is_empty() {
            self.command_log.push(
                "HATCH: select a closed boundary, or run HATCH and click inside an area".into(),
            );
            return;
        }
        self.history.snapshot(&self.document);
        self.selection = loops
            .into_iter()
            .map(|b| {
                self.document.add(EntityKind::Hatch {
                    boundary: b,
                    holes: Vec::new(),
                    fill,
                    pattern: self.hatch_pattern,
                })
            })
            .collect();
    }

    pub fn hatch_at_point(&mut self, x: f64, y: f64) -> bool {
        let (boundary, holes) = match oxidraft_cad::trace_pick_region(&self.document, x, y) {
            Ok(r) => r,
            Err(oxidraft_cad::PickRegionError::TooComplex) => {
                self.command_log
                    .push("HATCH: boundary too complex to trace (over 4000 segments)".into());
                return false;
            }
            Err(oxidraft_cad::PickRegionError::NotFound) => {
                self.command_log
                    .push("HATCH: no enclosed area found at that point".into());
                return false;
            }
        };
        let fill = self.document.layers.current_layer().color;
        self.history.snapshot(&self.document);
        let id = self.document.add(EntityKind::Hatch {
            boundary,
            holes,
            fill,
            pattern: self.hatch_pattern,
        });
        self.selection = vec![id];
        true
    }

    /// Applies and records a geometric constraint on the selected lines.
    /// Solving happens on a scratch copy so a failed solve leaves the
    /// document (and the undo stack) untouched.
    pub fn constrain_selection(&mut self, kind: oxidraft_cad::ConstraintKind) {
        // Coincident without two selected lines switches to the pick-based
        // WELD tool: click any two points — endpoint, midpoint, center, or
        // the origin — instead of pre-selecting geometry.
        if kind == oxidraft_cad::ConstraintKind::Coincident {
            let lines = self
                .selection
                .iter()
                .filter(|&&id| {
                    matches!(
                        self.document.get(id).and_then(|e| e.as_curve()),
                        Some(oxidraft_geometry::Curve::Line(_))
                    )
                })
                .count();
            if lines != 2 {
                self.tool = Tool::Weld { first: None };
                self.command_log
                    .push("WELD: pick two points to make them coincident".into());
                return;
            }
        }
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_lines(&mut doc, &self.selection, kind) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Welds two picked anchors coincident — an endpoint, a line midpoint,
    /// an arc/circle center, or a point entity like the origin — re-solving
    /// so they actually meet. Backs the pick-based WELD tool; solving
    /// happens on a scratch copy so a failed weld leaves the document
    /// untouched.
    pub fn weld_points(&mut self, a: (EntityId, u8), b: (EntityId, u8)) {
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_coincident_points(&mut doc, a, b) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Applies and records a driving radius on the selected circles/arcs;
    /// `None` locks the current radius. Solving happens on a scratch copy
    /// so a failed solve leaves the document untouched.
    pub fn constrain_radius_selection(&mut self, value: Option<f64>) {
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_radius(&mut doc, &self.selection, value) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Applies and records a driving length on the selected lines; `None`
    /// locks the current length. Solving happens on a scratch copy so a
    /// failed solve leaves the document untouched.
    pub fn constrain_distance_selection(&mut self, value: Option<f64>) {
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_distance(&mut doc, &self.selection, value) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Applies and records a driving angle between the two selected lines;
    /// `None` locks the current angle. Solving happens on a scratch copy so
    /// a failed solve leaves the document untouched.
    pub fn constrain_angle_selection(&mut self, value: Option<f64>) {
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_angle(&mut doc, &self.selection, value) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Retargets an existing driving dimension (length, radius, or angle) to
    /// `value`, re-solving its constraint component on a scratch copy so a
    /// failed solve leaves the document untouched. `value` is in the
    /// constraint's own unit — world length for length/radius, degrees for
    /// angle. Non-driving kinds are ignored. Backs the inline dimension
    /// editor opened by clicking a dimension badge.
    pub fn set_constraint_value(&mut self, target: SketchConstraint, value: f64) {
        let mut doc = self.document.clone();
        let res = match target.kind {
            ConstraintKind::Radius => {
                oxidraft_cad::constrain_radius(&mut doc, &[target.a], Some(value))
            }
            ConstraintKind::Distance => {
                oxidraft_cad::constrain_distance(&mut doc, &[target.a], Some(value))
            }
            ConstraintKind::LineDistance => match target.b {
                Some(b) => {
                    oxidraft_cad::constrain_line_distance(&mut doc, &[target.a, b], Some(value))
                }
                None => return,
            },
            ConstraintKind::Angle => match target.b {
                Some(b) => oxidraft_cad::constrain_angle(&mut doc, &[target.a, b], Some(value)),
                None => return,
            },
            _ => return,
        };
        match res {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Pins the selected geometry in place with a driving Fix constraint.
    pub fn fix_selection(&mut self) {
        let sel: Vec<EntityId> = self
            .selection
            .iter()
            .copied()
            .filter(|&id| id != self.origin_id)
            .collect();
        let mut doc = self.document.clone();
        match oxidraft_cad::constrain_fixed(&mut doc, &sel) {
            Ok(msg) => {
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
            }
            Err(e) => self.command_log.push(e),
        }
    }

    /// Smart-dimension entry point: adds a driving dimension to the picked
    /// geometry — a length on a line, a radius on a circle/arc, or (with a
    /// second line) an angle — then queues its inline value editor. `b` is
    /// the optional second line for an angle. `place` is where the user
    /// dropped the annotation (world coordinates); `None` leaves it to the
    /// automatic layout. Returns whether a constraint was added.
    pub fn smart_dimension(
        &mut self,
        a: EntityId,
        b: Option<EntityId>,
        place: Option<(f64, f64)>,
    ) -> bool {
        let mut doc = self.document.clone();
        let (res, kind): (Result<String, String>, ConstraintKind) = match b {
            // Two parallel lines mean the width *between* them — the angle
            // between parallels is a meaningless 180°. Crossing lines mean
            // the angle, exactly like the big CAD sketchers.
            Some(b) if lines_parallel(&self.document, a, b) => (
                oxidraft_cad::constrain_line_distance(&mut doc, &[a, b], None),
                ConstraintKind::LineDistance,
            ),
            Some(b) => (
                oxidraft_cad::constrain_angle(&mut doc, &[a, b], None),
                ConstraintKind::Angle,
            ),
            None if arc_or_circle(&self.document, a) => (
                oxidraft_cad::constrain_radius(&mut doc, &[a], None),
                ConstraintKind::Radius,
            ),
            None => (
                oxidraft_cad::constrain_distance(&mut doc, &[a], None),
                ConstraintKind::Distance,
            ),
        };
        match res {
            Ok(msg) => {
                // Pin the annotation where the user dropped it before the
                // document swap, so undo/redo carry the placement too.
                if place.is_some()
                    && let Some(c) = doc
                        .constraints
                        .iter_mut()
                        .rev()
                        .find(|c| c.kind == kind && c.a == a && c.b == b && c.val.is_some())
                {
                    c.place = place;
                }
                self.history.snapshot(&self.document);
                self.document = doc;
                self.command_log.push(msg);
                // Surface the new dimension and open its editor so the user
                // can type the value straight away.
                self.show_constraints = true;
                self.pending_dim_edit = self
                    .document
                    .constraints
                    .iter()
                    .rev()
                    .find(|c| c.kind == kind && c.a == a && c.b == b && c.val.is_some())
                    .copied();
                true
            }
            Err(e) => {
                self.command_log.push(e);
                false
            }
        }
    }

    /// Removes one specific constraint (by exact match), undoably. Backs the
    /// ✕ button in the inline dimension editor.
    pub fn remove_constraint(&mut self, target: SketchConstraint) {
        if self.document.constraints.contains(&target) {
            self.history.snapshot(&self.document);
            self.document.constraints.retain(|c| c != &target);
            self.command_log
                .push(format!("Removed {} constraint", target.kind.label()));
        }
    }

    /// Places n−1 division points at equal arc-length spacing on every
    /// selected curve (DIVIDE).
    pub fn divide_selection(&mut self, n: Option<u32>) {
        let Some(n) = n else {
            self.command_log
                .push("DIVIDE needs a segment count of 2 or more (DIVIDE 5)".into());
            return;
        };
        self.place_points_on_selection(
            "Select at least one curve to divide",
            "Nothing to divide on that selection",
            |doc, c| oxidraft_cad::commands::divide(doc, c, n).len(),
        );
    }

    /// Places points every `interval` of arc length on every selected
    /// curve (MEASURE).
    pub fn measure_selection(&mut self, interval: Option<f64>) {
        let Some(interval) = interval else {
            self.command_log
                .push("MEASURE needs a positive interval (MEASURE 2.5)".into());
            return;
        };
        self.place_points_on_selection(
            "Select at least one curve to measure",
            "The interval doesn't fit on that selection",
            |doc, c| oxidraft_cad::commands::measure(doc, c, interval).len(),
        );
    }

    /// Shared body for DIVIDE/MEASURE: run `per_curve` over each selected
    /// curve as one undo step, discarding the snapshot when it places
    /// nothing. `empty_msg` is logged for an empty selection, `zero_msg`
    /// when the operation places no points.
    fn place_points_on_selection(
        &mut self,
        empty_msg: &str,
        zero_msg: &str,
        per_curve: impl Fn(&mut Document, &oxidraft_geometry::Curve) -> usize,
    ) {
        let curves: Vec<oxidraft_geometry::Curve> = self
            .selection
            .iter()
            .filter_map(|&id| self.document.get(id).and_then(|e| e.as_curve()).cloned())
            .collect();
        if curves.is_empty() {
            self.command_log.push(empty_msg.into());
            return;
        }
        self.history.snapshot(&self.document);
        let placed: usize = curves
            .iter()
            .map(|c| per_curve(&mut self.document, c))
            .sum();
        if placed == 0 {
            self.history.discard_last();
            self.command_log.push(zero_msg.into());
        } else {
            self.command_log.push(format!("Placed {placed} point(s)"));
        }
    }

    /// Drops every recorded constraint that references a selected entity;
    /// the geometry stays exactly where it is.
    pub fn unconstrain_selection(&mut self) {
        if self.selection.is_empty() {
            self.command_log
                .push("Select entities to remove constraints from".into());
            return;
        }
        let count: usize = self
            .selection
            .iter()
            .map(|&id| self.document.constraints_on(id).count())
            .sum();
        if count == 0 {
            self.command_log
                .push("No constraints on the selection".into());
            return;
        }
        self.history.snapshot(&self.document);
        for id in self.selection.clone() {
            self.document.remove_constraints_on(id);
        }
        let remaining = self.document.constraints.len();
        self.command_log.push(format!(
            "Removed constraints touching the selection ({remaining} left in drawing)"
        ));
    }

    pub fn join_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        self.history.snapshot(&self.document);
        let ids: Vec<_> = std::mem::take(&mut self.selection)
            .into_iter()
            .filter(|&id| id != self.origin_id)
            .collect();
        let new_ids = edit::join(&mut self.document, &ids);
        if new_ids.is_empty() {
            self.selection = ids;
            self.history.discard_last();
            return;
        }
        let survived: Vec<_> = ids
            .into_iter()
            .filter(|&id| self.document.get(id).is_some())
            .collect();
        self.selection = survived.into_iter().chain(new_ids).collect();
    }

    pub fn zoom_extents(&mut self) {
        if let Some(bb) = self.document.extents() {
            let (x0, y0) = bb.min.to_f64();
            let (x1, y1) = bb.max.to_f64();
            let mut target = self.view.clone();
            target.zoom_to_bounds(x0, y0, x1, y1);
            self.zoom_target = Some((target.center.0, target.center.1, target.zoom));
        }
    }

    pub fn tick_zoom_anim(&mut self) -> bool {
        let Some((tx, ty, tz)) = self.zoom_target else {
            return false;
        };
        let k = 0.25;
        self.view.center.0 += (tx - self.view.center.0) * k;
        self.view.center.1 += (ty - self.view.center.1) * k;
        self.view.zoom = (self.view.zoom.ln() + (tz.ln() - self.view.zoom.ln()) * k).exp();
        let dc = (tx - self.view.center.0).hypot(ty - self.view.center.1) * self.view.zoom;
        let dz = (tz / self.view.zoom).ln().abs();
        if dc < 0.5 && dz < 2e-3 {
            self.view.center = (tx, ty);
            self.view.zoom = tz;
            self.zoom_target = None;
            return false;
        }
        true
    }

    pub fn add_entity(&mut self, kind: EntityKind) -> EntityId {
        self.history.snapshot(&self.document);
        self.document.add(kind)
    }

    pub fn selected_nurbs(&self) -> Option<(EntityId, Vec<Point2d>, Vec<f64>)> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = self.selection[0];
        if let EntityKind::Curve(Curve::Nurbs(nc)) = &self.document.get(id)?.kind {
            Some((id, nc.control.clone(), nc.weights.clone()))
        } else {
            None
        }
    }

    pub fn selected_nurbs_all(&self) -> Vec<(EntityId, Vec<Point2d>, Vec<f64>)> {
        self.selection
            .iter()
            .filter_map(|&id| match &self.document.get(id)?.kind {
                EntityKind::Curve(Curve::Nurbs(nc)) => {
                    Some((id, nc.control.clone(), nc.weights.clone()))
                }
                _ => None,
            })
            .collect()
    }

    pub fn begin_edit(&mut self) {
        self.history.snapshot(&self.document);
    }

    pub fn dim_override(&self, id: EntityId) -> Option<String> {
        match &self.document.get(id)?.kind {
            EntityKind::Dimension { override_text, .. }
            | EntityKind::OrthoDim { override_text, .. }
            | EntityKind::AngularDim { override_text, .. }
            | EntityKind::RadialDim { override_text, .. } => override_text.clone(),
            _ => None,
        }
    }

    pub fn set_dim_override(&mut self, id: EntityId, text: Option<String>) {
        let text = text.filter(|t| !t.trim().is_empty());
        if let Some(e) = self.document.get_mut(id) {
            match &mut e.kind {
                EntityKind::Dimension { override_text, .. }
                | EntityKind::OrthoDim { override_text, .. }
                | EntityKind::AngularDim { override_text, .. }
                | EntityKind::RadialDim { override_text, .. } => *override_text = text,
                _ => {}
            }
        }
    }

    pub fn commit_text_edit(
        &mut self,
        id: EntityId,
        content: String,
        font: Option<String>,
        size: f64,
    ) {
        let size = size.max(0.1);
        let changed = matches!(self.document.get(id).map(|e| &e.kind),
            Some(EntityKind::Text { content: c, font: f, height: h, .. })
                if *c != content || *f != font || (*h - size).abs() > 1e-9);
        if !changed {
            return;
        }
        self.history.snapshot(&self.document);
        if let Some(EntityKind::Text {
            content: c,
            font: f,
            height: h,
            ..
        }) = self.document.get_mut(id).map(|e| &mut e.kind)
        {
            *c = content;
            *f = font.clone();
            *h = size;
        }
        self.text_font = font;
    }

    pub fn outline_text_selection(&mut self) {
        let texts: Vec<EntityId> = self
            .selection
            .iter()
            .copied()
            .filter(|&id| {
                matches!(
                    self.document.get(id).map(|e| &e.kind),
                    Some(EntityKind::Text { .. })
                )
            })
            .collect();
        if texts.is_empty() {
            return;
        }
        self.history.snapshot(&self.document);
        let mut new_ids = Vec::new();
        for id in texts {
            let info = match self.document.get(id) {
                Some(e) => match &e.kind {
                    EntityKind::Text {
                        content,
                        font,
                        height,
                        anchor,
                        rotation,
                    } => Some((
                        content.clone(),
                        font.clone(),
                        *height,
                        *anchor,
                        *rotation,
                        e.layer,
                        e.color.clone(),
                    )),
                    _ => None,
                },
                None => None,
            };
            let Some((content, font, height, anchor, rotation, layer, color)) = info else {
                continue;
            };
            let curves =
                crate::fonts::outline_text(&content, font.as_deref(), height, anchor, rotation);
            if curves.is_empty() {
                continue;
            }

            for c in curves {
                let cid = self.document.add_on_layer(EntityKind::Curve(c), layer);
                if let Some(e) = self.document.get_mut(cid) {
                    e.color = color.clone();
                }
                new_ids.push(cid);
            }
            self.document.remove(id);
        }
        if !new_ids.is_empty() {
            self.selection = new_ids;
        }
    }

    pub fn set_nurbs_control(&mut self, id: EntityId, index: usize, p: Point2d) {
        if let Some(e) = self.document.get_mut(id)
            && let EntityKind::Curve(Curve::Nurbs(nc)) = &mut e.kind
            && index < nc.control.len()
        {
            nc.control[index] = p;
        }
    }

    pub fn adjust_nurbs_weight(&mut self, id: EntityId, index: usize, factor: f64) -> bool {
        let ok = matches!(self.document.get(id).map(|e| &e.kind),
            Some(EntityKind::Curve(Curve::Nurbs(nc))) if index < nc.weights.len());
        if !ok {
            return false;
        }
        self.history.snapshot(&self.document);
        if let Some(EntityKind::Curve(Curve::Nurbs(nc))) =
            self.document.get_mut(id).map(|e| &mut e.kind)
        {
            nc.weights[index] = (nc.weights[index] * factor).clamp(0.05, 20.0);
        }
        true
    }

    pub fn new_document(&mut self) {
        self.document = Document::new();
        seed_default_layers(&mut self.document);
        self.origin_id = add_origin_point(&mut self.document);
        self.selection.clear();
        self.history = History::new();
        self.tool = Tool::Select;
        self.current_file_path = None;
        self.saved_revision = self.history.current_revision();
    }

    pub fn open_file(&mut self, path: std::path::PathBuf) {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let result = match ext.as_str() {
            "dxf" => std::fs::read_to_string(&path)
                .map(|t| oxidraft_io::import_dxf(&t))
                .map_err(|e| e.to_string()),
            "svg" => std::fs::read_to_string(&path)
                .map(|t| oxidraft_io::import_svg(&t))
                .map_err(|e| e.to_string()),
            "dwg" => Err("DWG is a proprietary binary format oxiDRAFT can't read. \
                          Re-export it as DXF from your CAD app, then open the .dxf."
                .to_string()),
            _ => oxidraft_io::load_native(&path).map_err(|e| e.to_string()),
        };
        match result {
            Ok(mut doc) => {
                let origin_id = add_origin_point(&mut doc);
                self.document = doc;
                self.origin_id = origin_id;
                self.selection.clear();
                self.history = History::new();
                self.tool = Tool::Select;
                self.current_file_path = Some(path);
                self.saved_revision = self.history.current_revision();
            }
            Err(e) => self.command_log.push(format!("Cannot open: {e}")),
        }
    }

    pub fn save_file(&mut self) -> bool {
        if let Some(path) = self.current_file_path.clone() {
            self.save_file_to(path)
        } else {
            false
        }
    }

    pub fn save_file_to(&mut self, path: std::path::PathBuf) -> bool {
        let mut save_doc = self.document.clone();
        save_doc.remove(self.origin_id);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Every format saves atomically: a crash or full disk mid-write must
        // never truncate the existing good file on disk.
        let result = match ext.as_str() {
            "dxf" => {
                oxidraft_io::write_atomic(&path, oxidraft_io::export_dxf(&save_doc).as_bytes())
                    .map_err(|e| e.to_string())
            }
            "svg" => {
                oxidraft_io::write_atomic(&path, oxidraft_io::export_svg(&save_doc).as_bytes())
                    .map_err(|e| e.to_string())
            }
            "dwg" => Err("oxiDRAFT can't write DWG (proprietary binary). \
                          Save as DXF for CAD interchange."
                .to_string()),
            _ => oxidraft_io::save_native(&save_doc, &path).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) => {
                self.current_file_path = Some(path);
                self.saved_revision = self.history.current_revision();
                // The work is on disk now; the crash-recovery copy is stale.
                crate::autosave::discard_recovery();
                true
            }
            Err(e) => {
                self.command_log.push(format!("Save failed: {e}"));
                false
            }
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.history.current_revision() != self.saved_revision
    }

    /// Loads the crash-recovery copy as an *untitled, unsaved* document: no
    /// file path (so a save never targets the recovery file itself) and a
    /// saved-depth sentinel that keeps the document dirty until the user
    /// saves it somewhere real.
    pub fn restore_recovery(&mut self, path: &std::path::Path) -> bool {
        match oxidraft_io::load_native(path) {
            Ok(mut doc) => {
                let origin_id = add_origin_point(&mut doc);
                self.document = doc;
                self.origin_id = origin_id;
                self.selection.clear();
                self.history = History::new();
                self.tool = Tool::Select;
                self.current_file_path = None;
                self.saved_revision = u64::MAX;
                true
            }
            Err(e) => {
                self.command_log.push(format!("Recovery failed: {e}"));
                false
            }
        }
    }

    pub fn window_title(&self) -> String {
        let name = self
            .current_file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        let star = if self.is_dirty() { "*" } else { "" };
        format!("oxiDRAFT — {name}{star}")
    }

    pub fn document_label(&self) -> String {
        let name = self
            .current_file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Untitled".to_string());
        let star = if self.is_dirty() { "*" } else { "" };
        format!("{name}{star}")
    }

    pub fn coord_readout(&self) -> String {
        format!("{:.4}, {:.4}", self.cursor_world.0, self.cursor_world.1)
    }

    pub fn current_layer_name(&self) -> &str {
        &self.document.layers.current_layer().name
    }

    pub fn units_label(&self) -> &'static str {
        match self.document.settings.units.short_name() {
            "" => "none",
            s => s,
        }
    }

    pub fn sync_zoom_limits(&mut self) {
        let (mn, mx) = self.document.settings.units.visible_range();
        self.view.set_visible_range(mn, mx);
    }

    pub fn begin_bbox_drag(&mut self, handle: BboxHandle, cursor: (f64, f64)) {
        if self.selection.is_empty() {
            return;
        }

        let mut bbox: Option<oxidraft_geometry::BoundingBox> = None;
        for &id in &self.selection {
            if let Some(e) = self.document.get(id)
                && let Some(b) = e.bounding_box()
            {
                bbox = Some(if let Some(existing) = bbox {
                    existing.union(&b)
                } else {
                    b
                });
            }
        }

        if let Some(bbox_start) = bbox {
            let originals: Vec<(EntityId, EntityKind)> = self
                .selection
                .iter()
                .filter_map(|&id| self.document.get(id).map(|e| (id, e.kind.clone())))
                .collect();
            self.interaction.bbox_drag = Some(BboxDrag {
                handle,
                bbox_start,
                cursor_start: cursor,
                originals,
            });
            self.history.snapshot(&self.document);
        }
    }

    pub fn end_bbox_drag(&mut self) {
        self.interaction.bbox_drag = None;
    }

    pub fn apply_bbox_drag_transform(&mut self, cursor: (f64, f64)) {
        let Some(drag) = self.interaction.bbox_drag.as_ref() else {
            return;
        };
        let ids: Vec<_> = self.selection.clone();
        // Last solvable state for the moved set — this frame rebuilds from the
        // drag originals, so `prev` is the geometry we hold if the transform
        // can't satisfy the constraints (stick at the boundary, don't tear
        // welded neighbours apart).
        let prev: Vec<(EntityId, EntityKind)> = ids
            .iter()
            .filter_map(|&id| self.document.get(id).map(|e| (id, e.kind.clone())))
            .collect();

        for (id, kind) in &drag.originals {
            if let Some(e) = self.document.get_mut(*id) {
                e.kind = kind.clone();
            }
        }

        let (cx, cy) = cursor;
        let (sx, sy) = drag.cursor_start;
        let (dx, dy) = (cx - sx, cy - sy);

        let bbox = drag.bbox_start;
        let (bx0, by0) = (bbox.min.x, bbox.min.y);
        let (bx1, by1) = (bbox.max.x, bbox.max.y);

        match drag.handle {
            BboxHandle::Body => {
                edit::move_by(&mut self.document, &ids, dx, dy);
            }
            BboxHandle::CornerNW => {
                self.scale_bbox_from_opposite(&ids, bbox, cursor, (bx1, by1));
            }
            BboxHandle::CornerNE => {
                self.scale_bbox_from_opposite(&ids, bbox, cursor, (bx0, by1));
            }
            BboxHandle::CornerSW => {
                self.scale_bbox_from_opposite(&ids, bbox, cursor, (bx1, by0));
            }
            BboxHandle::CornerSE => {
                self.scale_bbox_from_opposite(&ids, bbox, cursor, (bx0, by0));
            }
            BboxHandle::RotateNW
            | BboxHandle::RotateNE
            | BboxHandle::RotateSW
            | BboxHandle::RotateSE => {
                let center = Point2d::from_f64((bx0 + bx1) / 2.0, (by0 + by1) / 2.0);
                let angle_start = (sy - center.y).atan2(sx - center.x);
                let angle_current = (cy - center.y).atan2(cx - center.x);
                let angle = angle_current - angle_start;

                if angle.abs() > 1e-9 {
                    edit::rotate(&mut self.document, &ids, &center, angle);
                }
            }
        }
        if !oxidraft_cad::resolve_after_transform(&mut self.document, &ids) {
            for (id, kind) in &prev {
                if let Some(e) = self.document.get_mut(*id) {
                    e.kind = kind.clone();
                }
            }
        }
    }

    fn scale_bbox_from_opposite(
        &mut self,
        ids: &[EntityId],
        bbox: oxidraft_geometry::BoundingBox,
        cursor: (f64, f64),
        opposite: (f64, f64),
    ) {
        let (cx, cy) = cursor;
        let (ox, oy) = opposite;
        let w = (cx - ox).abs();
        let h = (cy - oy).abs();
        let orig_w = (bbox.max.x - bbox.min.x).abs();
        let orig_h = (bbox.max.y - bbox.min.y).abs();

        if orig_w > 1e-9 && orig_h > 1e-9 {
            let sx = w / orig_w;
            let sy = h / orig_h;
            let s = sx.max(sy);

            if (s - 1.0).abs() > 1e-6 {
                let base = Point2d::from_f64(ox, oy);
                edit::scale(&mut self.document, ids, &base, s);
            }
        }
    }

    pub fn begin_grip_drag(&mut self, id: EntityId, grip: Grip) {
        if let Some(e) = self.document.get(id) {
            self.history.snapshot(&self.document);
            self.interaction.grip_drag = Some(GripDrag {
                entity_id: id,
                grip,
                start_kind: e.kind.clone(),
            });
        }
    }

    pub fn apply_grip_drag(&mut self, cursor: (f64, f64)) {
        let Some(drag) = self.interaction.grip_drag.as_ref() else {
            return;
        };
        let to = Point2d::from_f64(cursor.0, cursor.1);
        let edited = apply_grip(&drag.start_kind, &drag.grip, to);
        let id = drag.entity_id;
        // Snapshot the last solvable state. If this cursor position can't
        // satisfy the constraints, roll back to it so welded/constrained
        // geometry never visibly separates — the grip just stops at the
        // boundary instead of dragging a broken edit past the solver.
        let prev_kind = self.document.get(id).map(|e| e.kind.clone());
        let prev_constraints = self.document.constraints.clone();
        if let Some(e) = self.document.get_mut(id) {
            e.kind = edited;
        }
        self.reconstrain_tangency(id);
        if !self.resolve_constraints_after(id) {
            if let Some(k) = prev_kind
                && let Some(e) = self.document.get_mut(id)
            {
                e.kind = k;
            }
            self.document.constraints = prev_constraints;
        }
    }

    /// Re-satisfies persistent sketch constraints after `id` was edited.
    /// During a grip drag the dragged endpoint is pinned so the cursor wins;
    /// constrained partners follow. A failed solve leaves the raw edit.
    /// Returns whether the solve converged; `false` means the edit left the
    /// constraints unsatisfiable, and the caller should roll back.
    fn resolve_constraints_after(&mut self, id: EntityId) -> bool {
        let role = self.interaction.grip_drag.as_ref().map(|d| d.grip.role);
        self.retarget_driven_dimension(id, role);
        let pinned = match role {
            Some(oxidraft_cad::GripRole::Endpoint(i)) => Some(i),
            _ => None,
        };
        oxidraft_cad::resolve_after_edit(&mut self.document, id, pinned)
    }

    /// A grip that redefines a driven dimension — an arc's radius grip, a
    /// line's endpoint — retargets that dimension to the value just drawn,
    /// so the drag wins and the badge follows instead of the solver fighting
    /// the old value. Only an existing constraint is retargeted; dragging a
    /// grip never creates one.
    fn retarget_driven_dimension(&mut self, id: EntityId, role: Option<oxidraft_cad::GripRole>) {
        let retarget = match (role, self.document.get(id).and_then(|e| e.as_curve())) {
            (Some(oxidraft_cad::GripRole::Radius), Some(Curve::Arc(a))) => {
                oxidraft_document::SketchConstraint::radius(id, a.radius)
            }
            (Some(oxidraft_cad::GripRole::Endpoint(_)), Some(Curve::Line(l))) => {
                let len = (l.p1.x - l.p0.x).hypot(l.p1.y - l.p0.y);
                oxidraft_document::SketchConstraint::distance(id, len)
            }
            _ => return,
        };
        if self
            .document
            .constraints
            .iter()
            .any(|c| c.same_relation(&retarget))
        {
            self.document.add_constraint(retarget);
        }
    }

    fn reconstrain_tangency(&mut self, id: EntityId) {
        let Some(e) = self.document.get(id) else {
            return;
        };
        if e.tangents.is_empty() {
            return;
        }
        let Some(Curve::Arc(arc)) = e.as_curve() else {
            return;
        };
        let (center, radius) = (arc.center, arc.radius);
        let tangents = e.tangents.clone();
        let curves: Vec<Curve> = tangents
            .iter()
            .filter_map(|tr| {
                self.document
                    .get(tr.target)
                    .and_then(|t| t.as_curve())
                    .cloned()
            })
            .collect();
        if curves.len() != tangents.len() {
            return;
        }
        let solved = match curves.len() {
            3 => oxidraft_geometry::tangent_circle_ttt(&curves[0], &curves[1], &curves[2], center),
            2 => oxidraft_geometry::tangent_circle_ttr(&curves[0], &curves[1], radius, center),
            1 => {
                let r = oxidraft_geometry::point_to_curve_distance(&curves[0], center.x, center.y);
                (r > 1e-9).then_some((center, r))
            }
            _ => None,
        };
        if let Some((c, r)) = solved
            && r > 1e-9
            && let Some(e) = self.document.get_mut(id)
        {
            e.kind = EntityKind::Curve(Curve::Arc(oxidraft_geometry::CircularArc::new(
                c,
                r,
                0.0,
                std::f64::consts::TAU,
            )));
        }
    }

    pub fn remove_tangent(&mut self, id: EntityId, which: usize) {
        self.history.snapshot(&self.document);
        if let Some(e) = self.document.get_mut(id)
            && which < e.tangents.len()
        {
            e.tangents.remove(which);
        }
    }

    pub fn tangent_markers(&self, id: EntityId) -> Vec<(usize, Point2d)> {
        let Some(e) = self.document.get(id) else {
            return vec![];
        };
        let Some(Curve::Arc(arc)) = e.as_curve() else {
            return vec![];
        };
        e.tangents
            .iter()
            .enumerate()
            .filter_map(|(i, tr)| {
                let target = self.document.get(tr.target)?.as_curve()?;
                let (cx, cy) = arc.center.to_f64();
                let foot = oxidraft_geometry::project_point_onto_curve(target, cx, cy).point;
                let (fx, fy) = foot;
                let (dx, dy) = (fx - cx, fy - cy);
                let len = (dx * dx + dy * dy).sqrt();
                let tp = if len > 1e-9 {
                    Point2d::from_f64(cx + dx / len * arc.radius, cy + dy / len * arc.radius)
                } else {
                    Point2d::from_f64(fx, fy)
                };
                Some((i, tp))
            })
            .collect()
    }

    pub fn end_grip_drag(&mut self) {
        self.interaction.grip_drag = None;
    }

    pub fn cancel_grip_drag(&mut self) {
        if self.interaction.grip_drag.take().is_some() {
            // Constraint re-solve may have moved partner entities during the
            // drag, so restore the whole pre-drag document.
            if let Some(prev) = self.history.rollback() {
                self.document = prev;
            }
        }
    }

    pub fn grip_editing(&self) -> bool {
        self.interaction.grip_drag.is_some()
    }

    pub fn grip_role(&self) -> Option<oxidraft_cad::GripRole> {
        self.interaction.grip_drag.as_ref().map(|d| d.grip.role)
    }

    pub fn commit_grip_value(&mut self, value: f64) {
        let Some(drag) = self.interaction.grip_drag.as_ref() else {
            return;
        };
        let to = Point2d::from_f64(self.cursor_world.0, self.cursor_world.1);
        let edited = oxidraft_cad::apply_grip_value(&drag.start_kind, &drag.grip, value, to);
        let id = drag.entity_id;
        let prev_kind = self.document.get(id).map(|e| e.kind.clone());
        let prev_constraints = self.document.constraints.clone();
        if let Some(e) = self.document.get_mut(id) {
            e.kind = edited;
        }
        self.reconstrain_tangency(id);
        if !self.resolve_constraints_after(id) {
            if let Some(k) = prev_kind
                && let Some(e) = self.document.get_mut(id)
            {
                e.kind = k;
            }
            self.document.constraints = prev_constraints;
        }
        self.interaction.grip_drag = None;
    }

    pub fn selection_grips(&self) -> Vec<(EntityId, Grip)> {
        if !matches!(self.tool, Tool::Select) || self.interaction.corner_action.is_some() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for &id in &self.selection {
            if let Some(e) = self.document.get(id) {
                for g in grips_for(&e.kind) {
                    out.push((id, g));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{Curve, LineSeg};

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }

    fn app() -> AppState {
        AppState::new(800.0, 600.0)
    }

    fn line(x0: i64, y0: i64, x1: i64, y1: i64) -> EntityKind {
        EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(pt(x0, y0), pt(x1, y1))))
    }

    /// Every fresh document carries one `Fixed` constraint pinning the
    /// origin (see `add_origin_point`) — filtered out here so constraint
    /// assertions written before that existed still read as "the
    /// constraints this test's own actions produced."
    fn user_constraints(a: &AppState) -> Vec<oxidraft_document::SketchConstraint> {
        a.document
            .constraints
            .iter()
            .filter(|c| c.kind != oxidraft_document::ConstraintKind::Fixed)
            .copied()
            .collect()
    }

    #[test]
    fn polyline_closes_when_clicking_start_vertex() {
        let mut a = app();
        a.tool = Tool::Polyline { pts: Vec::new() };
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(pt(10, 0));
        a.place_tool_point(pt(5, 8));
        a.place_tool_point(pt(0, 0));

        // The closed chain commits as three individual welded lines, not
        // one PolyCurve — that's what lets each segment take constraints.
        let lines: Vec<_> = a
            .document
            .iter()
            .filter_map(|e| match &e.kind {
                EntityKind::Curve(Curve::Line(l)) => Some(l.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 3, "closed triangle = three line entities");
        assert!(
            lines[0].p0.dist_f64(&lines[2].p1) < 1e-9,
            "ends must coincide (closed)"
        );
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 3, "every corner of the closed chain is welded");
        assert!(matches!(a.tool, Tool::Polyline { ref pts } if pts.is_empty()));
    }

    #[test]
    fn rectangle_creates_welded_lines_with_axis_constraints() {
        let mut a = app();
        a.infer_constraints = true;
        a.tool = Tool::Rectangle { first: None };
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(pt(8, 5));

        let lines = a
            .document
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Curve(Curve::Line(_))))
            .count();
        assert_eq!(lines, 4, "four individual sides");
        let count = |k: oxidraft_document::ConstraintKind| {
            a.document
                .constraints
                .iter()
                .filter(|c| c.kind == k)
                .count()
        };
        assert_eq!(
            count(oxidraft_document::ConstraintKind::Coincident),
            4,
            "all four corners welded"
        );
        assert_eq!(count(oxidraft_document::ConstraintKind::Horizontal), 2);
        assert_eq!(count(oxidraft_document::ConstraintKind::Vertical), 2);
    }

    #[test]
    fn rectangle_welds_corners_even_without_auto_constrain() {
        let mut a = app();
        a.infer_constraints = false;
        a.tool = Tool::Rectangle { first: None };
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(pt(8, 5));

        let count = |k: oxidraft_document::ConstraintKind| {
            a.document
                .constraints
                .iter()
                .filter(|c| c.kind == k)
                .count()
        };
        assert_eq!(
            count(oxidraft_document::ConstraintKind::Coincident),
            4,
            "the welds are structural, not inferred — always recorded"
        );
        assert_eq!(
            count(oxidraft_document::ConstraintKind::Horizontal)
                + count(oxidraft_document::ConstraintKind::Vertical),
            0,
            "the axis constraints are inferred extras, gated on the toggle"
        );
    }

    #[test]
    fn hexagon_side_dimension_drives_all_sides() {
        let mut a = app();
        a.infer_constraints = true;
        a.snap_on = false;
        a.tool = Tool::Polygon {
            center: None,
            radius_point: None,
            sides: Some(6),
        };
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(pt(10, 0));
        a.confirm_pending_polygon();

        let sides: Vec<EntityId> = a
            .document
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Curve(Curve::Line(_))))
            .map(|e| e.id)
            .collect();
        assert_eq!(sides.len(), 6, "six individual sides");
        let count = |k: oxidraft_document::ConstraintKind| {
            a.document
                .constraints
                .iter()
                .filter(|c| c.kind == k)
                .count()
        };
        assert_eq!(count(oxidraft_document::ConstraintKind::Coincident), 6);
        assert_eq!(
            count(oxidraft_document::ConstraintKind::EqualLength),
            5,
            "every side equal to the first"
        );

        // The scenario that motivated all of this: smart-dimension one side
        // of the hexagon, type a value, and the equal-length chain resizes
        // every side.
        assert!(a.smart_dimension(sides[0], None, None));
        let dim = a
            .document
            .constraints
            .iter()
            .find(|c| c.kind == oxidraft_document::ConstraintKind::Distance && c.a == sides[0])
            .copied()
            .expect("a driving length landed on the picked side");
        a.set_constraint_value(dim, 5.0);
        for &id in &sides {
            let Some(EntityKind::Curve(Curve::Line(l))) = a.document.get(id).map(|e| &e.kind)
            else {
                panic!("side is still a line");
            };
            let len = (l.p1.x - l.p0.x).hypot(l.p1.y - l.p0.y);
            assert!(
                (len - 5.0).abs() < 1e-4,
                "every side follows the dimension: got {len}"
            );
        }
    }

    #[test]
    fn exploding_a_polycurve_welds_segments_when_auto_constrain_is_on() {
        let mut a = app();
        a.infer_constraints = true;
        let tri = oxidraft_geometry::PolyCurve::new(vec![
            Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 0), pt(4, 3))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 3), pt(0, 0))),
        ]);
        let id = a.add_entity(EntityKind::Curve(Curve::Poly(Box::new(tri))));
        a.selection = vec![id];
        a.explode_selection();

        assert!(a.document.get(id).is_none(), "the polycurve is gone");
        let lines = a
            .document
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Curve(Curve::Line(_))))
            .count();
        assert_eq!(lines, 3);
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 3, "explode weds the closed loop's corners");
    }

    #[test]
    fn exploding_stays_loose_with_auto_constrain_off() {
        let mut a = app();
        a.infer_constraints = false;
        let chain = oxidraft_geometry::PolyCurve::new(vec![
            Curve::Line(LineSeg::from_endpoints(pt(0, 0), pt(4, 0))),
            Curve::Line(LineSeg::from_endpoints(pt(4, 0), pt(4, 3))),
        ]);
        let id = a.add_entity(EntityKind::Curve(Curve::Poly(Box::new(chain))));
        a.selection = vec![id];
        a.explode_selection();
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 0, "DISJOINT keeps its word when inference is off");
    }

    #[test]
    fn polyline_does_not_close_on_a_non_start_point() {
        let mut a = app();
        a.tool = Tool::Polyline { pts: Vec::new() };
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(pt(10, 0));
        a.place_tool_point(pt(5, 8));
        a.place_tool_point(pt(12, 8));
        assert!(matches!(a.tool, Tool::Polyline { ref pts } if pts.len() == 4));
        assert!(
            !a.document
                .iter()
                .any(|e| matches!(&e.kind, EntityKind::Curve(Curve::Poly(_))))
        );
    }

    #[test]
    fn tangent_markers_and_removal() {
        use oxidraft_geometry::CircularArc;
        let mut a = app();
        a.snap_on = false;
        let l1 = a.document.add(line(0, 0, 10, 0));
        let l2 = a.document.add(line(0, 0, 0, 10));
        let cid = a
            .document
            .add(EntityKind::Curve(Curve::Arc(CircularArc::new(
                Point2d::from_f64(2.0, 2.0),
                2.0,
                0.0,
                std::f64::consts::TAU,
            ))));
        if let Some(e) = a.document.get_mut(cid) {
            e.tangents = vec![
                oxidraft_document::TangentRef {
                    target: l1,
                    near: Point2d::from_f64(2.0, 0.0),
                },
                oxidraft_document::TangentRef {
                    target: l2,
                    near: Point2d::from_f64(0.0, 2.0),
                },
            ];
        }
        a.selection = vec![cid];
        let markers = a.tangent_markers(cid);
        assert_eq!(markers.len(), 2);
        for (_, p) in &markers {
            assert!((p.dist_f64(&Point2d::from_f64(2.0, 2.0)) - 2.0).abs() < 1e-6);
        }
        a.remove_tangent(cid, 0);
        assert_eq!(a.tangent_markers(cid).len(), 1);
    }

    #[test]
    fn clipboard_copy_paste_duplicates_at_cursor() {
        let mut a = app();
        let id = a.document.add(line(0, 0, 10, 0));
        a.selection = vec![id];
        assert_eq!(a.clipboard_copy(), 1);

        a.cursor_world = (50.0, 20.0);
        let before = a.document.len();
        a.clipboard_paste();
        assert_eq!(a.document.len(), before + 1);
        assert_eq!(a.selection.len(), 1, "pasted entity becomes the selection");

        let pasted = a.document.get(a.selection[0]).unwrap();
        if let EntityKind::Curve(Curve::Line(l)) = &pasted.kind {
            assert!((l.p0.x - 45.0).abs() < 1e-9 && (l.p0.y - 20.0).abs() < 1e-9);
            assert!((l.p1.x - 55.0).abs() < 1e-9 && (l.p1.y - 20.0).abs() < 1e-9);
        } else {
            panic!("expected a pasted line");
        }
    }

    #[test]
    fn clipboard_cut_removes_then_pastes() {
        let mut a = app();
        let id = a.document.add(line(0, 0, 2, 2));
        a.selection = vec![id];
        let with_entity = a.document.len();
        a.clipboard_cut();
        assert_eq!(a.document.len(), with_entity - 1, "cut removes the entity");
        a.cursor_world = (0.0, 0.0);
        a.clipboard_paste();
        assert_eq!(a.document.len(), with_entity, "paste restores one entity");
    }

    #[test]
    fn paste_with_empty_clipboard_is_noop() {
        let mut a = app();
        let before = a.document.len();
        a.clipboard_paste();
        assert_eq!(a.document.len(), before);
    }

    #[test]
    fn ui_prefs_round_trip() {
        let p = UiPrefs {
            snap_on: false,
            grid_on: true,
            grid_snap_on: true,
            ortho_on: true,
            polar_on: false,
            track_on: false,
            dyn_on: true,
            comb_on: true,
            comb_scale: 7.5,
            snap_px: 8.0,
            polar_step: 30.0,
            zoom_speed: 1.5,
            zoom_to_cursor: false,
            invert_zoom: true,
            crosshair: false,
            pick_box: 14.0,
            show_lineweights: false,
            lineweight_scale: 3.0,
            grid_dots: true,
            grid_major_every: 4,
            grid_minor_rgb: (20, 30, 40),
            grid_major_rgb: (50, 60, 70),
            text_font: Some("Arial".into()),
            infer_constraints: false,
            show_constraints: false,
        };
        assert_eq!(UiPrefs::deserialize(&p.serialize()), p);
        let q = UiPrefs {
            text_font: None,
            ..Default::default()
        };
        assert_eq!(UiPrefs::deserialize(&q.serialize()).text_font, None);
    }

    #[test]
    fn dimension_lands_on_dimension_layer() {
        let mut a = app();
        a.tool = crate::tools::Tool::Dimension { p1: None, p2: None };
        a.place_tool_point(Point2d::from_f64(0.0, 0.0));
        a.place_tool_point(Point2d::from_f64(10.0, 0.0));
        a.place_tool_point(Point2d::from_f64(0.0, 3.0));
        let dim = a
            .document
            .iter()
            .find(|e| matches!(e.kind, oxidraft_document::EntityKind::Dimension { .. }))
            .expect("a dimension entity");
        let layer = a.document.layers.get(dim.layer).expect("its layer");
        assert_eq!(layer.name, oxidraft_document::DIMENSION_LAYER);
    }

    #[test]
    fn weld_tool_welds_origin_to_a_line_midpoint() {
        let mut a = app();
        a.snap_on = false;
        let l = a
            .document
            .add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(2.0, 3.0),
                Point2d::from_f64(6.0, 5.0),
            ))));
        a.tool = crate::tools::Tool::Weld { first: None };
        // First pick: the origin point entity at (0,0).
        let (ox, oy) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(ox, oy);
        assert!(
            matches!(a.tool, crate::tools::Tool::Weld { first: Some((id, 0, _)) } if id == a.origin_id),
            "origin picked as the first anchor: {:?}",
            a.tool
        );
        // Second pick: the line's midpoint at (4,4).
        let (mx, my) = a.view.world_to_screen(4.0, 4.0);
        a.canvas_click(mx, my);
        let c = a
            .document
            .constraints
            .iter()
            .find(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .expect("the weld was recorded");
        assert_eq!(
            c.pts,
            Some((0, oxidraft_document::ANCHOR_DERIVED)),
            "origin anchor 0 welded to the line's midpoint anchor"
        );
        let ls = match a.document.get(l).and_then(|e| e.as_curve()) {
            Some(Curve::Line(ls)) => ls.clone(),
            other => panic!("expected the line, got {other:?}"),
        };
        let mid = ((ls.p0.x + ls.p1.x) * 0.5, (ls.p0.y + ls.p1.y) * 0.5);
        assert!(
            mid.0.abs() < 1e-6 && mid.1.abs() < 1e-6,
            "the line slid so its midpoint sits on the origin: {mid:?}"
        );
        assert!(
            matches!(a.tool, crate::tools::Tool::Weld { first: None }),
            "the tool reset for the next weld"
        );
    }

    #[test]
    fn radial_dimension_tool_dimensions_a_circle() {
        let mut a = app();
        a.snap_on = false;
        let circle = a.document.add(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(
                Point2d::from_f64(0.0, 0.0),
                5.0,
                0.0,
                std::f64::consts::TAU,
            ),
        )));
        a.tool = crate::tools::Tool::DimRadial {
            diameter: false,
            center: None,
            radius: 0.0,
        };
        let (sx, sy) = a.view.world_to_screen(5.0, 0.0);
        a.canvas_click(sx, sy);
        assert!(
            matches!(a.tool, crate::tools::Tool::DimRadial { center: Some(_), radius, .. } if (radius - 5.0).abs() < 1e-9),
            "circle pick set centre+radius"
        );
        let (lx, ly) = a.view.world_to_screen(0.0, 6.0);
        a.canvas_click(lx, ly);
        let made = a
            .document
            .iter()
            .any(|e| matches!(&e.kind, EntityKind::RadialDim { center, .. } if *center == Point2d::from_f64(0.0, 0.0)));
        assert!(made, "radial dimension created on the circle");
        let _ = circle;
    }

    #[test]
    fn angular_from_two_lines_creates_dim_at_intersection() {
        let mut a = app();
        a.snap_on = false;
        a.document
            .add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(10.0, 0.0),
            ))));
        a.document
            .add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                Point2d::from_f64(0.0, 0.0),
                Point2d::from_f64(0.0, 10.0),
            ))));
        a.tool = crate::tools::Tool::DimAngularLines {
            a: None,
            geom: None,
        };
        let (s1x, s1y) = a.view.world_to_screen(5.0, 0.0);
        a.canvas_click(s1x, s1y);
        let (s2x, s2y) = a.view.world_to_screen(0.0, 5.0);
        a.canvas_click(s2x, s2y);
        assert!(
            matches!(
                a.tool,
                crate::tools::Tool::DimAngularLines { geom: Some(_), .. }
            ),
            "two line picks produced the angle geometry"
        );
        let (lx, ly) = a.view.world_to_screen(3.0, 3.0);
        a.canvas_click(lx, ly);
        let dim = a
            .document
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::AngularDim { center, .. } => Some(*center),
                _ => None,
            })
            .expect("an angular dimension");
        assert!(
            dim.dist_f64(&Point2d::from_f64(0.0, 0.0)) < 1e-6,
            "vertex at intersection"
        );
    }

    #[test]
    fn apply_prefs_keeps_ortho_polar_exclusive() {
        let mut a = app();
        let p = UiPrefs {
            ortho_on: true,
            polar_on: true,
            ..Default::default()
        };
        a.apply_prefs(&p);
        assert!(a.ortho_on && !a.polar_on, "ortho should win over polar");
    }

    #[test]
    fn save_open_dispatches_by_extension() {
        // save_file_to retires the process-wide recovery file on success;
        // serialize with the autosave test so neither deletes the other's.
        let _guard = crate::autosave::RECOVERY_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for ext in ["o2d", "dxf", "svg"] {
            let mut a = app();
            a.document
                .add(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
                    pt(0, 0),
                    pt(10, 5),
                ))));
            a.document.add(EntityKind::Curve(Curve::Arc(
                oxidraft_geometry::CircularArc::new(pt(3, 4), 5.0, 0.0, std::f64::consts::TAU),
            )));
            let want = a.document.iter().filter(|e| e.id != a.origin_id).count();

            let path = std::env::temp_dir()
                .join(format!("o2d_io_test_{}_{ext}.{ext}", std::process::id()));
            assert!(a.save_file_to(path.clone()), "save .{ext} should succeed");

            let mut b = app();
            b.open_file(path.clone());
            let got = b.document.iter().filter(|e| e.id != b.origin_id).count();
            assert_eq!(
                got, want,
                ".{ext} round-trip lost entities: {want} -> {got}"
            );
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn opening_a_legacy_e2d_file_still_works() {
        // Files saved before the rename to oxiDRAFT (extension .e2d, magic
        // "E2D") must still open through the real File > Open path.
        let path = std::env::temp_dir().join(format!("o2d_legacy_test_{}.e2d", std::process::id()));
        std::fs::write(&path, "E2D 1\nE LINE 0 bylayer 0;0 4;0 ByLayer bylayer\n").unwrap();

        let mut a = app();
        a.open_file(path.clone());
        let _ = std::fs::remove_file(path);

        assert!(
            a.command_log.is_empty(),
            "opening a legacy .e2d file should not log an error: {:?}",
            a.command_log
        );
        assert_eq!(a.document.iter().filter(|e| e.id != a.origin_id).count(), 1);
    }

    #[test]
    fn line_command_then_two_clicks_creates_segment() {
        let mut a = app();
        a.run_command("LINE");
        assert_eq!(a.tool.name(), "LINE");
        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        let (s2x, s2y) = a.view.world_to_screen(5.0, 0.0);
        a.snap_on = false;
        a.canvas_click(s1x, s1y);
        assert_eq!(a.document.len(), 1);
        a.canvas_click(s2x, s2y);
        assert_eq!(a.document.len(), 2);
    }

    #[test]
    fn undo_redo_through_state() {
        let mut a = app();
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(1, 1),
        ))));
        assert_eq!(a.document.len(), 2);
        a.undo();
        assert_eq!(a.document.len(), 1);
        a.redo();
        assert_eq!(a.document.len(), 2);
    }

    #[test]
    fn erase_removes_selection() {
        let mut a = app();
        let id = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(2, 2),
        ))));
        a.selection = vec![id];
        a.run_command("ERASE");
        assert_eq!(a.document.len(), 1);
    }

    #[test]
    fn select_all_then_erase() {
        let mut a = app();
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(1, 0),
        ))));
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(0, 1),
        ))));
        a.run_command("ALL");
        assert_eq!(a.selection.len(), 2);
        a.run_command("ERASE");
        assert_eq!(a.document.len(), 1);
    }

    #[test]
    fn layer_commands() {
        let mut a = app();
        a.run_command("LAYER NEW walls");
        assert_eq!(a.current_layer_name(), "walls");
        a.run_command("LAYER SET 0");
        assert_eq!(a.current_layer_name(), "0");
    }

    #[test]
    fn move_command_uses_selection() {
        let mut a = app();
        let id = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(2, 0),
        ))));
        a.selection = vec![id];
        a.run_command("MOVE");
        a.snap_on = false;
        let (b1x, b1y) = a.view.world_to_screen(0.0, 0.0);
        let (b2x, b2y) = a.view.world_to_screen(10.0, 5.0);
        a.canvas_click(b1x, b1y);
        a.canvas_click(b2x, b2y);
        if let Some(Curve::Line(l)) = a.document.get(id).unwrap().as_curve() {
            assert!((l.p0.x - 10.0).abs() < 1e-4);
            assert!((l.p0.y - 5.0).abs() < 1e-4);
        } else {
            panic!()
        }
    }

    #[test]
    fn zoom_extents_frames_geometry() {
        let mut a = app();
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(100, 80),
        ))));
        a.run_command("ZOOM E");
        for _ in 0..200 {
            if !a.tick_zoom_anim() {
                break;
            }
        }
        let (x0, y0, x1, y1) = a.view.visible_bounds();
        assert!(x0 <= 0.0 && x1 >= 100.0 && y0 <= 0.0 && y1 >= 80.0);
    }

    #[test]
    fn coord_readout_tracks_cursor() {
        let mut a = app();
        let (sx, sy) = a.view.world_to_screen(3.0, 7.0);
        a.pointer_moved(sx, sy);
        let r = a.coord_readout();
        assert!(r.starts_with("3.0000, 7.0000"));
    }

    #[test]
    fn perpendicular_snapping_uses_tool_reference_point() {
        let mut a = app();
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 0),
        ))));
        a.snap.enabled = vec![oxidraft_cad::SnapKind::Perpendicular];
        a.snap_on = true;

        a.run_command("LINE");

        let (s1x, s1y) = a.view.world_to_screen(3.0, 5.0);
        a.canvas_click(s1x, s1y);

        let (s2x, s2y) = a.view.world_to_screen(3.1, 0.1);
        a.pointer_moved(s2x, s2y);

        assert!(a.active_snap.is_some());
        let sp = a.active_snap.as_ref().unwrap();
        assert_eq!(sp.kind, oxidraft_cad::SnapKind::Perpendicular);
        assert!((sp.pos.0 - 3.0).abs() < 1e-4);
        assert!(sp.pos.1.abs() < 1e-4);
    }

    #[test]
    fn grid_snap_locks_cursor_to_grid_intersection() {
        let mut a = app();
        a.snap_on = false;
        a.grid_snap_on = true;
        a.run_command("LINE");

        let g = a.view.grid_spacing();
        let (sx, sy) = a.view.world_to_screen(2.0 * g + g * 0.2, -g - g * 0.1);
        a.pointer_moved(sx, sy);
        assert!(
            (a.cursor_world.0 - 2.0 * g).abs() < 1e-6,
            "x={}",
            a.cursor_world.0
        );
        assert!(
            (a.cursor_world.1 - (-g)).abs() < 1e-6,
            "y={}",
            a.cursor_world.1
        );
    }

    #[test]
    fn grip_drag_snaps_to_other_entity() {
        let mut a = app();
        a.snap_on = true;
        let l1 = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 0),
        ))));
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(5, 5),
            pt(20, 5),
        ))));
        a.selection = vec![l1];
        let grip = a
            .selection_grips()
            .into_iter()
            .find(|(id, _)| *id == l1)
            .map(|(_, g)| g)
            .expect("line should expose grips");
        a.begin_grip_drag(l1, grip);
        let (sx, sy) = a.view.world_to_screen(5.0, 5.0);
        a.pointer_moved(sx, sy);
        assert!(
            a.active_snap.is_some(),
            "expected a snap while grip-dragging"
        );
        assert!(
            (a.cursor_world.0 - 5.0).abs() < 1e-6 && (a.cursor_world.1 - 5.0).abs() < 1e-6,
            "cursor did not snap to the other entity: {:?}",
            a.cursor_world
        );
    }

    fn perpendicular_pair(a: &mut AppState) -> (EntityId, EntityId) {
        let l1 = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(5, 0),
        ))));
        let l2 = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(0, 3),
        ))));
        a.selection = vec![l1, l2];
        a.constrain_selection(oxidraft_cad::ConstraintKind::Perpendicular);
        assert_eq!(user_constraints(a).len(), 1, "constraint recorded");
        (l1, l2)
    }

    fn line_of(a: &AppState, id: EntityId) -> LineSeg {
        match a.document.get(id).unwrap().as_curve().unwrap() {
            Curve::Line(l) => l.clone(),
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn grip_drag_maintains_perpendicular_constraint() {
        let mut a = app();
        a.snap_on = false;
        let (l1, l2) = perpendicular_pair(&mut a);
        let grip = oxidraft_cad::grips_for(&a.document.get(l1).unwrap().kind)[1];
        a.begin_grip_drag(l1, grip);
        a.apply_grip_drag((4.0, 3.0));
        a.end_grip_drag();
        let la = line_of(&a, l1);
        let lb = line_of(&a, l2);
        assert!(
            (la.p1.x - 4.0).abs() < 1e-6 && (la.p1.y - 3.0).abs() < 1e-6,
            "dragged endpoint follows the cursor: {la:?}"
        );
        let dot =
            (la.p1.x - la.p0.x) * (lb.p1.x - lb.p0.x) + (la.p1.y - la.p0.y) * (lb.p1.y - lb.p0.y);
        assert!(dot.abs() < 1e-6, "partner re-solved, dot={dot}");
    }

    #[test]
    fn cancel_grip_drag_restores_constrained_partners() {
        let mut a = app();
        a.snap_on = false;
        let (l1, l2) = perpendicular_pair(&mut a);
        let before_a = line_of(&a, l1);
        let before_b = line_of(&a, l2);
        let grip = oxidraft_cad::grips_for(&a.document.get(l1).unwrap().kind)[1];
        a.begin_grip_drag(l1, grip);
        a.apply_grip_drag((4.0, 3.0));
        let moved_b = line_of(&a, l2);
        assert!(
            (moved_b.p1.x - before_b.p1.x).abs() > 1e-3
                || (moved_b.p1.y - before_b.p1.y).abs() > 1e-3,
            "partner moved during the drag"
        );
        a.cancel_grip_drag();
        let la = line_of(&a, l1);
        let lb = line_of(&a, l2);
        assert_eq!(
            (la.p0, la.p1),
            (before_a.p0, before_a.p1),
            "dragged line restored"
        );
        assert_eq!(
            (lb.p0, lb.p1),
            (before_b.p0, before_b.p1),
            "partner restored"
        );
        assert_eq!(user_constraints(&a).len(), 1, "constraint survives cancel");
    }

    #[test]
    fn infeasible_grip_drag_holds_the_last_solvable_state() {
        // Two length locks that can never both hold: any solve stays above
        // tolerance, so the drag is unsatisfiable at every cursor position.
        let mut a = app();
        a.snap_on = false;
        let id = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(5, 0),
        ))));
        a.document
            .constraints
            .push(oxidraft_document::SketchConstraint::distance(id, 5.0));
        a.document
            .constraints
            .push(oxidraft_document::SketchConstraint::distance(id, 3.0));
        let before = line_of(&a, id);
        let grip = oxidraft_cad::grips_for(&a.document.get(id).unwrap().kind)[1];
        a.begin_grip_drag(id, grip);
        a.apply_grip_drag((9.0, 9.0));
        let after = line_of(&a, id);
        assert_eq!(
            (before.p0, before.p1),
            (after.p0, after.p1),
            "an unsatisfiable drag must hold the last solvable geometry, not tear it: {after:?}"
        );
        assert_eq!(
            user_constraints(&a).len(),
            2,
            "the rolled-back step must not leave a retargeted constraint behind"
        );
    }

    #[test]
    fn dragging_an_endpoint_retargets_a_driven_length() {
        let mut a = app();
        a.snap_on = false;
        let id = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(4, 0),
        ))));
        a.selection = vec![id];
        a.constrain_distance_selection(Some(4.0));
        // Grab the far endpoint and stretch it out along the axis.
        let grip = oxidraft_cad::grips_for(&a.document.get(id).unwrap().kind)[1];
        a.begin_grip_drag(id, grip);
        a.apply_grip_drag((7.0, 0.0));
        a.end_grip_drag();
        let l = line_of(&a, id);
        assert!(
            (l.p1.x - 7.0).abs() < 1e-6 && l.p0.x.abs() < 1e-6,
            "drag wins, the untouched end stays put: {l:?}"
        );
        let c = a
            .document
            .constraints
            .iter()
            .find(|c| c.kind == oxidraft_document::ConstraintKind::Distance && c.a == id)
            .expect("length constraint survives");
        assert_eq!(c.val, Some(7.0), "the length dimension followed the drag");
    }

    #[test]
    fn dragging_the_radius_grip_retargets_a_driven_radius() {
        use oxidraft_geometry::CircularArc;
        let mut a = app();
        a.snap_on = false;
        let circle = a.add_entity(EntityKind::Curve(Curve::Arc(CircularArc::new(
            Point2d::from_f64(0.0, 0.0),
            2.0,
            0.0,
            std::f64::consts::TAU,
        ))));
        a.selection = vec![circle];
        a.constrain_radius_selection(Some(2.0));
        // A full circle's mid grips drive the radius; drag one out to r = 3.
        let grip = oxidraft_cad::grips_for(&a.document.get(circle).unwrap().kind)
            .into_iter()
            .find(|g| g.role == oxidraft_cad::GripRole::Radius)
            .expect("a full circle exposes radius grips");
        a.begin_grip_drag(circle, grip);
        a.apply_grip_drag((0.0, 3.0));
        a.end_grip_drag();
        let r = match a.document.get(circle).unwrap().as_curve().unwrap() {
            Curve::Arc(arc) => arc.radius,
            other => panic!("expected arc, got {other:?}"),
        };
        assert!((r - 3.0).abs() < 1e-6, "drag resized the circle: {r}");
        let c = a
            .document
            .constraints
            .iter()
            .find(|c| c.kind == oxidraft_document::ConstraintKind::Radius && c.a == circle)
            .expect("radius constraint survives");
        assert_eq!(c.val, Some(3.0), "the radius dimension followed the drag");
    }

    #[test]
    fn chained_line_segments_weld_and_stay_attached() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.run_command("LINE");
        for (x, y) in [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0)] {
            let (sx, sy) = a.view.world_to_screen(x, y);
            a.canvas_click(sx, sy);
        }
        let l1 = a.document.order[1];
        let l2 = a.document.order[2];
        let welds: Vec<_> = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .collect();
        assert_eq!(welds.len(), 1, "chain weld recorded");
        let c = welds[0];
        assert_eq!((c.a, c.b), (l1, Some(l2)));
        assert_eq!(c.pts, Some((1, 0)));

        // Drag the shared corner of the first segment; the second follows.
        let grip = oxidraft_cad::grips_for(&a.document.get(l1).unwrap().kind)[1];
        a.begin_grip_drag(l1, grip);
        a.apply_grip_drag((9.0, 1.0));
        a.end_grip_drag();
        let s2 = line_of(&a, l2);
        assert!(
            (s2.p0.x - 9.0).abs() < 1e-6 && (s2.p0.y - 1.0).abs() < 1e-6,
            "welded corner followed: {s2:?}"
        );
    }

    #[test]
    fn closing_a_line_chain_welds_the_loop_corner() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.run_command("LINE");
        // Typed points are exact; the last one lands back on the chain start.
        for (x, y) in [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0), (0.0, 0.0)] {
            a.place_tool_point(Point2d::from_f64(x, y));
        }
        let l1 = a.document.order[1];
        let l3 = a.document.order[3];
        let closure = a
            .document
            .constraints
            .iter()
            .find(|c| {
                c.kind == oxidraft_document::ConstraintKind::Coincident
                    && (c.a, c.b) == (l1, Some(l3))
            })
            .expect("closing segment welded to the chain start");
        assert_eq!(closure.pts, Some((0, 1)));
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 3, "two chain welds plus the closure");
    }

    #[test]
    fn near_axis_drawn_line_is_leveled_and_constrained_horizontal() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        a.run_command("LINE");
        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);
        // 0.04 world units = 2 px at the default zoom: inside the 3 px slack.
        let (s2x, s2y) = a.view.world_to_screen(8.0, 0.04);
        a.canvas_click(s2x, s2y);
        let id = *a.document.order.last().unwrap();
        let l = line_of(&a, id);
        assert!((l.p1.y - l.p0.y).abs() < 1e-12, "line snapped level: {l:?}");
        assert!(
            a.document
                .constraints
                .iter()
                .any(|c| c.kind == oxidraft_document::ConstraintKind::Horizontal && c.a == id),
            "horizontal constraint recorded"
        );
        // The chain continues from the levelled end, not the raw click.
        match &a.tool {
            Tool::Line { last: Some(p) } => {
                assert!((p.to_f64().1 - l.p1.y).abs() < 1e-12, "chain follows level")
            }
            other => panic!("line tool still active, got {other:?}"),
        }
    }

    #[test]
    fn near_axis_drawn_line_is_leveled_and_constrained_vertical() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        a.run_command("LINE");
        let (s1x, s1y) = a.view.world_to_screen(2.0, 1.0);
        a.canvas_click(s1x, s1y);
        let (s2x, s2y) = a.view.world_to_screen(2.04, 7.0);
        a.canvas_click(s2x, s2y);
        let id = *a.document.order.last().unwrap();
        let l = line_of(&a, id);
        assert!((l.p1.x - l.p0.x).abs() < 1e-12, "line snapped plumb: {l:?}");
        assert!(
            a.document
                .constraints
                .iter()
                .any(|c| c.kind == oxidraft_document::ConstraintKind::Vertical && c.a == id),
            "vertical constraint recorded"
        );
    }

    #[test]
    fn typed_near_axis_end_is_not_leveled() {
        let mut a = app();
        a.snap_on = false;
        a.run_command("LINE");
        a.place_tool_point(Point2d::from_f64(0.0, 0.0));
        a.place_tool_point(Point2d::from_f64(8.0, 0.04));
        let id = *a.document.order.last().unwrap();
        let l = line_of(&a, id);
        assert!((l.p1.y - 0.04).abs() < 1e-12, "typed end untouched: {l:?}");
        assert!(
            user_constraints(&a).is_empty(),
            "no constraint on a deliberately off-axis typed line"
        );
    }

    #[test]
    fn exact_axis_lines_record_constraints_without_moving() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.run_command("LINE");
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 4.0)] {
            a.place_tool_point(Point2d::from_f64(x, y));
        }
        let l1 = a.document.order[1];
        let l2 = a.document.order[2];
        assert!(
            a.document
                .constraints
                .iter()
                .any(|c| c.kind == oxidraft_document::ConstraintKind::Horizontal && c.a == l1),
            "exact horizontal typed line recorded"
        );
        assert!(
            a.document
                .constraints
                .iter()
                .any(|c| c.kind == oxidraft_document::ConstraintKind::Vertical && c.a == l2),
            "exact vertical typed line recorded"
        );
        let s1 = line_of(&a, l1);
        assert_eq!((s1.p0.x, s1.p0.y, s1.p1.x, s1.p1.y), (0.0, 0.0, 10.0, 0.0));
    }

    #[test]
    fn endpoint_snap_infers_coincident_on_drawn_line() {
        let mut a = app();
        a.snap_on = true;
        a.infer_constraints = true;
        let base = a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 0),
        ))));
        a.run_command("LINE");
        let (sx, sy) = a.view.world_to_screen(10.0, 0.0);
        a.canvas_click(sx, sy);
        let (sx2, sy2) = a.view.world_to_screen(15.0, 5.0);
        a.canvas_click(sx2, sy2);
        let new_id = *a.document.order.last().unwrap();
        let uc = user_constraints(&a);
        assert_eq!(uc.len(), 1, "snap coincidence recorded");
        let c = uc[0];
        assert_eq!((c.a, c.b, c.pts), (base, Some(new_id), Some((1, 0))));
    }

    /// Radius-5 arc about the origin from −45° to +45°. Returns the id,
    /// the +45° endpoint, and the unit tangent direction there — chosen
    /// deliberately off-axis so tangent inference can't be confused with
    /// h/v inference.
    fn quarter_arc(a: &mut AppState) -> (EntityId, (f64, f64), (f64, f64)) {
        let q = std::f64::consts::FRAC_PI_4;
        let arc = oxidraft_geometry::CircularArc::new(pt(0, 0), 5.0, -q, q);
        let end = arc.end_point();
        let id = a.add_entity(EntityKind::Curve(Curve::Arc(arc)));
        (id, end, (-q.sin(), q.cos()))
    }

    #[test]
    fn line_drawn_off_an_arc_endpoint_snaps_tangent_and_records() {
        let mut a = app();
        a.snap_on = true;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        let (arc_id, att, t) = quarter_arc(&mut a);
        a.run_command("LINE");
        let (sx, sy) = a.view.world_to_screen(att.0, att.1);
        a.canvas_click(sx, sy);
        // 6 units out along the tangent, pushed 0.04 (2 px) off the ray:
        // inside the 3 px slack, so the segment rotates exactly tangent.
        let click = (
            att.0 + 6.0 * t.0 - 0.04 * t.1,
            att.1 + 6.0 * t.1 + 0.04 * t.0,
        );
        let (sx2, sy2) = a.view.world_to_screen(click.0, click.1);
        a.canvas_click(sx2, sy2);
        let line_id = *a.document.order.last().unwrap();
        let l = line_of(&a, line_id);
        let cross = t.0 * (l.p1.y - l.p0.y) - t.1 * (l.p1.x - l.p0.x);
        assert!(
            cross.abs() < 1e-9,
            "line rotated onto the tangent ray: {cross}"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Tangent
                    && (c.a, c.b) == (arc_id, Some(line_id))
            }),
            "tangent constraint recorded"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Coincident
                    && (c.a, c.b, c.pts) == (arc_id, Some(line_id), Some((1, 0)))
            }),
            "weld to the arc endpoint recorded"
        );
        // The chain continues from the rotated end, not the raw click.
        match &a.tool {
            Tool::Line { last: Some(p) } => {
                assert!(
                    (p.x - l.p1.x).abs() < 1e-12 && (p.y - l.p1.y).abs() < 1e-12,
                    "chain follows the rotated end"
                );
            }
            other => panic!("line tool still active, got {other:?}"),
        }
    }

    #[test]
    fn inferred_tangency_survives_a_grip_drag() {
        let mut a = app();
        a.snap_on = true;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        let (_, att, t) = quarter_arc(&mut a);
        a.run_command("LINE");
        let (sx, sy) = a.view.world_to_screen(att.0, att.1);
        a.canvas_click(sx, sy);
        let click = (att.0 + 6.0 * t.0, att.1 + 6.0 * t.1);
        let (sx2, sy2) = a.view.world_to_screen(click.0, click.1);
        a.canvas_click(sx2, sy2);
        let line_id = *a.document.order.last().unwrap();
        a.run_command("");

        // Swing the free end; the arc must follow, staying tangent.
        let grips = oxidraft_cad::grips_for(&a.document.get(line_id).unwrap().kind);
        let grip = *grips
            .iter()
            .find(|g| {
                matches!(g.role, oxidraft_cad::GripRole::Endpoint(_))
                    && (g.world.x - att.0).hypot(g.world.y - att.1) > 1.0
            })
            .expect("free endpoint grip");
        a.begin_grip_drag(line_id, grip);
        a.apply_grip_drag((-2.0, 6.0));
        a.end_grip_drag();

        let l = line_of(&a, line_id);
        let arc = a
            .document
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::Curve(Curve::Arc(arc)) => Some(*arc),
                _ => None,
            })
            .expect("arc still present");
        let (ux, uy) = (l.p1.x - l.p0.x, l.p1.y - l.p0.y);
        let d = (ux * (arc.center.y - l.p0.y) - uy * (arc.center.x - l.p0.x)) / ux.hypot(uy);
        assert!(
            (d.abs() - arc.radius).abs() < 1e-6,
            "arc re-solved tangent to the dragged line: gap {}",
            d.abs() - arc.radius
        );
    }

    #[test]
    fn typed_near_tangent_end_is_not_rotated() {
        let mut a = app();
        a.snap_on = true;
        a.polar_on = false;
        a.track_on = false;
        let (_, att, t) = quarter_arc(&mut a);
        a.run_command("LINE");
        let (sx, sy) = a.view.world_to_screen(att.0, att.1);
        a.canvas_click(sx, sy);
        let typed = (
            att.0 + 6.0 * t.0 - 0.04 * t.1,
            att.1 + 6.0 * t.1 + 0.04 * t.0,
        );
        a.place_tool_point(Point2d::from_f64(typed.0, typed.1));
        let line_id = *a.document.order.last().unwrap();
        let l = line_of(&a, line_id);
        assert!(
            (l.p1.x - typed.0).abs() < 1e-12 && (l.p1.y - typed.1).abs() < 1e-12,
            "typed end untouched: {l:?}"
        );
        assert!(
            !a.document
                .constraints
                .iter()
                .any(|c| c.kind == oxidraft_document::ConstraintKind::Tangent),
            "no tangent on a deliberately off-tangent typed line"
        );
    }

    #[test]
    fn typed_exact_tangent_records_without_moving() {
        let mut a = app();
        a.snap_on = true;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        let (arc_id, att, t) = quarter_arc(&mut a);
        a.run_command("LINE");
        let (sx, sy) = a.view.world_to_screen(att.0, att.1);
        a.canvas_click(sx, sy);
        let typed = (att.0 + 6.0 * t.0, att.1 + 6.0 * t.1);
        a.place_tool_point(Point2d::from_f64(typed.0, typed.1));
        let line_id = *a.document.order.last().unwrap();
        let l = line_of(&a, line_id);
        assert!(
            (l.p1.x - typed.0).abs() < 1e-12 && (l.p1.y - typed.1).abs() < 1e-12,
            "exact end untouched: {l:?}"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Tangent
                    && (c.a, c.b) == (arc_id, Some(line_id))
            }),
            "exact tangency recorded on a pinned end"
        );
    }

    #[test]
    fn line_drawn_into_an_arc_endpoint_rotates_its_free_start() {
        let mut a = app();
        a.snap_on = true;
        a.infer_constraints = true;
        a.polar_on = false;
        a.track_on = false;
        let (arc_id, att, t) = quarter_arc(&mut a);
        a.run_command("LINE");
        // Freehand start near the tangent ray, then end snapped onto the
        // arc endpoint: the start is the only movable end.
        let start = (
            att.0 + 6.0 * t.0 - 0.04 * t.1,
            att.1 + 6.0 * t.1 + 0.04 * t.0,
        );
        let (sx, sy) = a.view.world_to_screen(start.0, start.1);
        a.canvas_click(sx, sy);
        let (sx2, sy2) = a.view.world_to_screen(att.0, att.1);
        a.canvas_click(sx2, sy2);
        let line_id = *a.document.order.last().unwrap();
        let l = line_of(&a, line_id);
        assert!(
            (l.p1.x - att.0).abs() < 1e-12 && (l.p1.y - att.1).abs() < 1e-12,
            "snapped end stays on the arc endpoint: {l:?}"
        );
        let cross = t.0 * (l.p1.y - l.p0.y) - t.1 * (l.p1.x - l.p0.x);
        assert!(
            cross.abs() < 1e-9,
            "free start rotated onto the ray: {cross}"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Tangent
                    && (c.a, c.b) == (arc_id, Some(line_id))
            }),
            "tangent constraint recorded"
        );
    }

    #[test]
    fn arc_drawn_off_a_line_endpoint_pulls_tangent_and_welds() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        // Horizontal line ending at the origin; endpoint 0 is the shared corner.
        let line_id = a.add_entity(line(0, 0, -5, 0));
        // Arc through the origin with its centre nudged just off the y-axis, so
        // it leaves the origin a hair off horizontal — inside the 3 px slack
        // but not yet tangent (centre-to-line gap ~1.6e-4 before solving).
        let (cx, cy) = (0.04_f64, 5.0_f64);
        let r = (cx * cx + cy * cy).sqrt();
        let start_angle = (0.0 - cy).atan2(0.0 - cx);
        let arc =
            oxidraft_geometry::CircularArc::new(Point2d::from_f64(cx, cy), r, start_angle, 0.0);
        let arc_id = a.add_entity(EntityKind::Curve(Curve::Arc(arc)));
        a.infer_arc_onset_tangency(arc_id);

        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Tangent
                    && (c.a, c.b) == (arc_id, Some(line_id))
            }),
            "tangent inferred between the drawn arc and the line"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Coincident
                    && c.a == arc_id
                    && c.b == Some(line_id)
            }),
            "shared corner welded coincident"
        );
        let solved = a
            .document
            .get(arc_id)
            .and_then(|e| match &e.kind {
                EntityKind::Curve(Curve::Arc(arc)) => Some(*arc),
                _ => None,
            })
            .expect("arc still present");
        let l = line_of(&a, line_id);
        let (ux, uy) = (l.p1.x - l.p0.x, l.p1.y - l.p0.y);
        let d = (ux * (solved.center.y - l.p0.y) - uy * (solved.center.x - l.p0.x)).abs()
            / ux.hypot(uy);
        assert!(
            (d - solved.radius).abs() < 1e-6,
            "arc pulled exactly tangent to the line: gap {}",
            d - solved.radius
        );
    }

    #[test]
    fn arc_tool_off_a_line_endpoint_infers_tangency() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        // Horizontal line whose endpoint 1 lands on the origin.
        a.run_command("LINE");
        a.place_tool_point(pt(-5, 0));
        a.place_tool_point(pt(0, 0));
        let line_id = *a.document.order.last().unwrap();
        // Three-point arc off the origin, exactly tangent to the line: all
        // three points sit on the circle centred at (0, 5), so it leaves the
        // origin horizontal.
        a.run_command("ARC");
        a.place_tool_point(pt(0, 0));
        a.place_tool_point(Point2d::from_f64(3.5355339, 1.4644661));
        a.place_tool_point(pt(5, 5));
        let arc_id = *a.document.order.last().unwrap();
        assert!(
            matches!(
                a.document.get(arc_id).map(|e| &e.kind),
                Some(EntityKind::Curve(Curve::Arc(_)))
            ),
            "the three points made an arc"
        );
        assert!(
            a.document.constraints.iter().any(|c| {
                c.kind == oxidraft_document::ConstraintKind::Tangent
                    && (c.a, c.b) == (arc_id, Some(line_id))
            }),
            "arc-onset tangency inferred through the Arc tool"
        );
    }

    #[test]
    fn arc_onset_tangency_respects_the_infer_toggle() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = false;
        let _line_id = a.add_entity(line(0, 0, -5, 0));
        let arc = oxidraft_geometry::CircularArc::new(
            Point2d::from_f64(0.0, 5.0),
            5.0,
            -std::f64::consts::FRAC_PI_2,
            0.0,
        );
        let arc_id = a.add_entity(EntityKind::Curve(Curve::Arc(arc)));
        a.infer_arc_onset_tangency(arc_id);
        assert!(
            user_constraints(&a).is_empty(),
            "auto-constrain off: no tangency inferred"
        );
    }

    #[test]
    fn radcon_command_drives_the_selected_circle() {
        let mut a = app();
        a.snap_on = false;
        let c = a.add_entity(EntityKind::Curve(Curve::Arc(
            oxidraft_geometry::CircularArc::new(pt(0, 0), 2.0, 0.0, std::f64::consts::TAU),
        )));
        a.selection = vec![c];
        a.run_command("RADCON 3.5");
        let radius_of = |a: &AppState| match a.document.get(c).unwrap().as_curve().unwrap() {
            Curve::Arc(arc) => arc.radius,
            other => panic!("expected arc, got {other:?}"),
        };
        assert!((radius_of(&a) - 3.5).abs() < 1e-7, "circle resized");
        let uc = user_constraints(&a);
        assert_eq!(uc.len(), 1);
        assert_eq!(uc[0].val, Some(3.5));
        a.undo();
        assert!((radius_of(&a) - 2.0).abs() < 1e-9, "undo restores the size");
        assert!(user_constraints(&a).is_empty(), "and drops the record");
    }

    #[test]
    fn inference_respects_the_toggle() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = false;
        a.run_command("LINE");
        for (x, y) in [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0)] {
            let (sx, sy) = a.view.world_to_screen(x, y);
            a.canvas_click(sx, sy);
        }
        assert!(user_constraints(&a).is_empty(), "toggle off, no welds");
    }

    #[test]
    fn moving_a_welded_line_drags_its_neighbour() {
        let mut a = app();
        a.snap_on = false;
        a.infer_constraints = true;
        a.run_command("LINE");
        for (x, y) in [(0.0, 0.0), (8.0, 0.0), (8.0, 6.0)] {
            let (sx, sy) = a.view.world_to_screen(x, y);
            a.canvas_click(sx, sy);
        }
        let l1 = a.document.order[1];
        let l2 = a.document.order[2];
        a.run_command("");
        a.selection = vec![l1];
        // Move l1 by (2, 1) through the modify-tool path.
        let t = oxidraft_geometry::Transform2d::translation(2.0, 1.0);
        a.apply_tool_event(ToolEvent::Transform { ids: vec![l1], t });
        let s1 = line_of(&a, l1);
        let s2 = line_of(&a, l2);
        assert!(
            (s1.p1.x - 10.0).abs() < 1e-6 && (s1.p1.y - 1.0).abs() < 1e-6,
            "l1 moved: {s1:?}"
        );
        assert!(
            (s2.p0.x - 10.0).abs() < 1e-6 && (s2.p0.y - 1.0).abs() < 1e-6,
            "welded neighbour reattached: {s2:?}"
        );
    }

    #[test]
    fn divide_and_measure_place_points_on_the_selection() {
        let mut a = app();
        a.run_command("LINE");
        a.canvas_click(400.0, 300.0); // world (0,0)
        a.canvas_click(650.0, 300.0); // world (5,0) at zoom 50
        a.run_command("");
        let line_id = *a.document.order.last().unwrap();
        a.selection = vec![line_id];

        let before = a.document.len();
        a.run_command("DIVIDE 5");
        assert_eq!(a.document.len(), before + 4, "4 division points");
        a.execute(Command::Undo);
        assert_eq!(a.document.len(), before, "divide is one undo step");

        a.selection = vec![line_id];
        a.run_command("MEASURE 2");
        assert_eq!(a.document.len(), before + 2, "points at 2 and 4");

        // Bad forms log usage instead of touching the document or history.
        let depth = a.history.undo_depth();
        a.run_command("DIVIDE 1");
        a.run_command("MEASURE 0");
        a.selection.clear();
        a.run_command("DIVIDE 4");
        assert_eq!(
            a.history.undo_depth(),
            depth,
            "declined commands snapshot nothing"
        );
    }

    #[test]
    fn plot_window_pick_stores_the_rect_and_reopens_the_dialog() {
        let mut a = AppState::new(800.0, 600.0);
        a.snap_on = false;
        a.grid_snap_on = false;
        a.tool = Tool::PlotWindow { first: None };
        // Screen (300,350) is world (−2,−1); (500,250) is world (2,1).
        a.canvas_click(300.0, 350.0);
        a.canvas_click(500.0, 250.0);
        let (x0, y0, x1, y1) = a.plot_window.expect("window stored");
        assert!(
            (x0 + 2.0).abs() < 1e-9
                && (y0 + 1.0).abs() < 1e-9
                && (x1 - 2.0).abs() < 1e-9
                && (y1 - 1.0).abs() < 1e-9,
            "corners sorted: ({x0},{y0})..({x1},{y1})"
        );
        assert!(
            a.plot_dialog_open && a.plot_window_mode,
            "the dialog reopens in Window mode after the pick"
        );
        assert!(matches!(a.tool, Tool::Select));

        // A zero-area pick declines the window but still reopens.
        a.plot_dialog_open = false;
        a.plot_window = None;
        a.tool = Tool::PlotWindow { first: None };
        a.canvas_click(400.0, 300.0);
        a.canvas_click(400.0, 300.0);
        assert_eq!(a.plot_window, None, "no area, no window");
        assert!(a.plot_dialog_open && a.plot_window_mode);
    }

    #[test]
    fn ortho_mode_constrains_cursor_to_axis() {
        let mut a = app();
        a.snap_on = false;
        a.ortho_on = true;

        a.run_command("LINE");
        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);

        let (s2x, s2y) = a.view.world_to_screen(8.0, 3.0);
        a.pointer_moved(s2x, s2y);
        assert!((a.cursor_world.0 - 8.0).abs() < 1e-4);
        assert!(a.cursor_world.1.abs() < 1e-4);

        let (s3x, s3y) = a.view.world_to_screen(2.0, 9.0);
        a.pointer_moved(s3x, s3y);
        assert!(a.cursor_world.0.abs() < 1e-4);
        assert!((a.cursor_world.1 - 9.0).abs() < 1e-4);
    }

    #[test]
    fn perpendicular_snapping_triggers_anywhere_near_line() {
        let mut a = app();
        a.add_entity(EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(
            pt(0, 0),
            pt(10, 0),
        ))));
        a.snap.enabled = vec![oxidraft_cad::SnapKind::Perpendicular];
        a.snap_on = true;

        a.run_command("LINE");
        let (s1x, s1y) = a.view.world_to_screen(5.0, 5.0);
        a.canvas_click(s1x, s1y);

        let (s2x, s2y) = a.view.world_to_screen(5.3, 0.1);
        a.pointer_moved(s2x, s2y);

        assert!(a.active_snap.is_some());
        let sp = a.active_snap.as_ref().unwrap();
        assert_eq!(sp.kind, oxidraft_cad::SnapKind::Perpendicular);
        assert!((sp.pos.0 - 5.0).abs() < 1e-4);
        assert!(sp.pos.1.abs() < 1e-4);
    }

    #[test]
    fn direct_distance_entry_projects_along_cursor() {
        let mut a = app();
        a.snap_on = false;
        a.run_command("LINE");

        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);

        let (s2x, s2y) = a.view.world_to_screen(3.0, 4.0);
        a.pointer_moved(s2x, s2y);

        a.run_command("10.0");

        assert_eq!(a.document.len(), 2);
        let first = a.document.iter().find(|e| e.id != a.origin_id).unwrap();
        if let EntityKind::Curve(Curve::Line(l)) = &first.kind {
            assert!((l.p0.x - 0.0).abs() < 1e-4);
            assert!((l.p0.y - 0.0).abs() < 1e-4);
            assert!((l.p1.x - 6.0).abs() < 1e-4);
            assert!((l.p1.y - 8.0).abs() < 1e-4);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn typed_coordinates_build_a_line() {
        let mut a = app();
        a.snap_on = false;
        a.run_command("LINE");
        a.run_command("0,0");
        a.run_command("@10,0");

        assert_eq!(a.document.len(), 2);
        let line = a.document.iter().find(|e| e.id != a.origin_id).unwrap();
        if let EntityKind::Curve(Curve::Line(l)) = &line.kind {
            assert!((l.p0.x).abs() < 1e-9 && (l.p0.y).abs() < 1e-9);
            assert!((l.p1.x - 10.0).abs() < 1e-9 && (l.p1.y).abs() < 1e-9);
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn relative_polar_coordinate_places_point() {
        let mut a = app();
        a.snap_on = false;
        a.run_command("LINE");
        a.run_command("0,0");
        a.run_command("@5<90");

        let line = a.document.iter().find(|e| e.id != a.origin_id).unwrap();
        if let EntityKind::Curve(Curve::Line(l)) = &line.kind {
            assert!((l.p1.x).abs() < 1e-6, "x should be ~0, got {}", l.p1.x);
            assert!(
                (l.p1.y - 5.0).abs() < 1e-6,
                "y should be ~5, got {}",
                l.p1.y
            );
        } else {
            panic!("expected line");
        }
    }

    #[test]
    fn right_click_repeat_reactivates_last_command() {
        let mut a = app();
        a.run_command("CIRCLE");
        assert!(matches!(a.tool, Tool::Circle { .. }));
        assert_eq!(a.last_command.as_deref(), Some("CIRCLE"));
        a.run_command("");
        assert!(matches!(a.tool, Tool::Select));
        a.repeat_last_command();
        assert!(matches!(a.tool, Tool::Circle { .. }));
    }

    #[test]
    fn polygon_command_allows_side_update() {
        let mut a = app();
        a.run_command("POLYGON");
        assert!(matches!(
            a.tool,
            Tool::Polygon {
                center: None,
                radius_point: None,
                sides: None
            }
        ));

        a.run_command("6");
        assert!(matches!(
            a.tool,
            Tool::Polygon {
                center: None,
                radius_point: None,
                sides: Some(6)
            }
        ));

        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);

        let (s2x, s2y) = a.view.world_to_screen(10.0, 0.0);
        a.canvas_click(s2x, s2y);

        // Both clicks placed (center + radius), but the side-count popup is
        // now pending confirmation — nothing is created yet.
        assert_eq!(a.document.len(), 1);
        assert!(matches!(
            a.tool,
            Tool::Polygon {
                center: Some(_),
                radius_point: Some(_),
                sides: Some(6)
            }
        ));

        a.confirm_pending_polygon();
        // Origin + six individual welded side lines.
        assert_eq!(a.document.len(), 7);
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 6, "every hexagon corner is welded");
        assert!(matches!(
            a.tool,
            Tool::Polygon {
                center: None,
                radius_point: None,
                ..
            }
        ));
    }

    #[test]
    fn polygon_cancel_pending_drops_without_committing() {
        let mut a = app();
        a.run_command("POLYGON");
        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);
        let (s2x, s2y) = a.view.world_to_screen(10.0, 0.0);
        a.canvas_click(s2x, s2y);
        assert_eq!(a.document.len(), 1);

        a.cancel_pending_polygon();
        assert_eq!(a.document.len(), 1, "cancel must not create anything");
        assert!(matches!(
            a.tool,
            Tool::Polygon {
                center: None,
                radius_point: None,
                sides: Some(6)
            }
        ));
    }

    #[test]
    fn polyline_command_commits_on_empty_command() {
        let mut a = app();
        a.run_command("PL");
        assert!(matches!(a.tool, Tool::Polyline { .. }));

        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);
        let (s2x, s2y) = a.view.world_to_screen(5.0, 5.0);
        a.canvas_click(s2x, s2y);
        let (s3x, s3y) = a.view.world_to_screen(10.0, 0.0);
        a.canvas_click(s3x, s3y);

        a.run_command("");
        assert!(matches!(a.tool, Tool::Select));
        // Origin + two individual welded lines.
        assert_eq!(a.document.len(), 3);
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 1, "the open chain's shared corner is welded");
    }

    #[test]
    fn cv_spline_command_commits_to_nurbs() {
        let mut a = app();
        a.run_command("SPLINE");
        assert!(matches!(a.tool, Tool::Spline { .. }));

        for (wx, wy) in [(0.0, 0.0), (5.0, 8.0), (10.0, -4.0), (15.0, 0.0)] {
            let (sx, sy) = a.view.world_to_screen(wx, wy);
            a.canvas_click(sx, sy);
        }
        a.run_command("");
        assert!(matches!(a.tool, Tool::Select));
        assert_eq!(a.document.len(), 2);

        let entity = a.document.iter().find(|e| e.id != a.origin_id).unwrap();
        match &entity.kind {
            EntityKind::Curve(Curve::Nurbs(nc)) => assert_eq!(nc.control.len(), 4),
            other => panic!("expected a NURBS curve, got {:?}", other),
        }
    }

    #[test]
    fn nurbs_grip_edit_moves_control_and_weight() {
        let mut a = app();
        let nc = oxidraft_geometry::NurbsCurve::uniform(vec![
            Point2d::from_i64(0, 0),
            Point2d::from_i64(2, 4),
            Point2d::from_i64(6, 4),
            Point2d::from_i64(8, 0),
            Point2d::from_i64(10, 4),
        ]);
        let id = a.add_entity(EntityKind::Curve(Curve::Nurbs(nc)));
        a.selection = vec![id];

        let (sid, control, weights) = a.selected_nurbs().expect("a NURBS is selected");
        assert_eq!(sid, id);
        assert_eq!(control.len(), 5);
        assert!(weights.iter().all(|&w| w == 1.0));

        a.begin_edit();
        a.set_nurbs_control(id, 2, Point2d::from_f64(6.0, 9.0));
        let weight_at = |a: &AppState, i: usize| {
            if let EntityKind::Curve(Curve::Nurbs(nc)) = &a.document.get(id).unwrap().kind {
                (nc.control[i], nc.weights[i])
            } else {
                panic!("expected NURBS")
            }
        };
        assert_eq!(weight_at(&a, 2).0, Point2d::from_f64(6.0, 9.0));
        assert!(a.adjust_nurbs_weight(id, 2, 5.0));
        assert!((weight_at(&a, 2).1 - 5.0).abs() < 1e-9);
        a.adjust_nurbs_weight(id, 2, 100.0);
        assert!(weight_at(&a, 2).1 <= 20.0 + 1e-9);
        a.undo();
        assert!(
            (weight_at(&a, 2).1 - 5.0).abs() < 1e-9,
            "undo restores the prior weight"
        );
    }

    #[test]
    fn polyline_command_closes_on_c_command() {
        let mut a = app();
        a.run_command("PL");

        let (s1x, s1y) = a.view.world_to_screen(0.0, 0.0);
        a.canvas_click(s1x, s1y);
        let (s2x, s2y) = a.view.world_to_screen(5.0, 5.0);
        a.canvas_click(s2x, s2y);
        let (s3x, s3y) = a.view.world_to_screen(10.0, 0.0);
        a.canvas_click(s3x, s3y);

        a.run_command("c");
        assert!(matches!(a.tool, Tool::Select));
        // Origin + three individual lines (two drawn + the closer), with
        // every corner of the closed chain welded.
        assert_eq!(a.document.len(), 4);
        let lines = a
            .document
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Curve(Curve::Line(_))))
            .count();
        assert_eq!(lines, 3);
        let welds = a
            .document
            .constraints
            .iter()
            .filter(|c| c.kind == oxidraft_document::ConstraintKind::Coincident)
            .count();
        assert_eq!(welds, 3, "closed chain welds all three corners");
    }

    #[test]
    fn fixed_origin_test() {
        let mut a = app();
        if let Some(EntityKind::Point(p)) = a.document.get(a.origin_id).map(|e| &e.kind) {
            assert_eq!(p.to_f64(), (0.0, 0.0));
        } else {
            panic!("expected origin point");
        }

        a.toggle_selection(a.origin_id);
        assert!(!a.selection.contains(&a.origin_id));

        a.selection = vec![a.origin_id];
        a.erase_selection();
        assert!(a.document.get(a.origin_id).is_some());

        let t = oxidraft_geometry::Transform2d::translation(10.0, 10.0);
        let ev = ToolEvent::Transform {
            ids: vec![a.origin_id],
            t,
        };
        a.apply_tool_event(ev);
        if let Some(EntityKind::Point(p)) = a.document.get(a.origin_id).map(|e| &e.kind) {
            assert_eq!(p.to_f64(), (0.0, 0.0));
        } else {
            panic!("expected origin point");
        }
    }

    #[test]
    fn text_tool_places_text_entity() {
        let mut a = app();
        a.run_command("TEXT");
        assert!(matches!(a.tool, Tool::Text { anchor: None, .. }));
        let (sx, sy) = a.view.world_to_screen(2.0, 3.0);
        a.canvas_click(sx, sy);
        assert!(matches!(
            a.tool,
            Tool::Text {
                anchor: Some(_),
                ..
            }
        ));
        a.run_command("Hello\\nWorld");
        assert!(matches!(a.tool, Tool::Select));
        let content = a
            .document
            .iter()
            .find_map(|e| match &e.kind {
                EntityKind::Text { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("a Text entity should be created");
        assert_eq!(
            content, "Hello\nWorld",
            "single unified tool handles multi-line via \\n"
        );
    }

    #[test]
    fn reconstrain_tangency_tolerates_deleted_target() {
        use oxidraft_document::TangentRef;
        use oxidraft_geometry::CircularArc;
        let mut a = app();
        let t1 = a.document.add(line(0, 0, 10, 0));
        let t2 = a.document.add(line(0, 10, 10, 10));
        let arc = a
            .document
            .add(EntityKind::Curve(Curve::Arc(CircularArc::new(
                pt(5, 5),
                1.0,
                0.0,
                std::f64::consts::TAU,
            ))));
        if let Some(e) = a.document.get_mut(arc) {
            e.tangents = vec![
                TangentRef {
                    target: t1,
                    near: pt(5, 0),
                },
                TangentRef {
                    target: t2,
                    near: pt(5, 10),
                },
            ];
        }
        // Drop one tangent target, leaving a dangling reference.
        a.document.remove(t2);
        // Must not panic, and must leave the arc untouched (cannot re-solve).
        a.reconstrain_tangency(arc);
        assert!(matches!(
            a.document.get(arc).and_then(|e| e.as_curve()),
            Some(Curve::Arc(_))
        ));
    }
}
