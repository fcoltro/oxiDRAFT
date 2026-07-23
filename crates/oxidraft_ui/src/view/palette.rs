//! The command palette: a searchable, fuzzy-filtered overlay (opened by a
//! keyboard shortcut) listing every tool and toggle by name, hint, and
//! keywords, so any command is reachable without memorizing its keybinding.

use super::UiState;
use crate::icons::Icon;
use crate::state::AppState;
use crate::theme;
use egui::{Context, Key};

enum Action {
    Cmd(&'static str),
    ToggleGrid,
    ToggleSnap,
    ToggleGridSnap,
    ToggleOrtho,
    TogglePolar,
    ToggleTrack,
    ToggleDyn,
}

struct Entry {
    name: &'static str,
    hint: &'static str,
    keywords: &'static str,
    group: &'static str,
    icon: Icon,
    action: Action,
}

const ENTRIES: &[Entry] = &[
    Entry {
        name: "Select",
        hint: "Esc",
        keywords: "pick arrow",
        group: "Tools",
        icon: Icon::Select,
        action: Action::Cmd("SELECT"),
    },
    Entry {
        name: "Line",
        hint: "L",
        keywords: "segment draw",
        group: "Tools",
        icon: Icon::Line,
        action: Action::Cmd("LINE"),
    },
    Entry {
        name: "Polyline",
        hint: "P",
        keywords: "draw connected",
        group: "Tools",
        icon: Icon::Polyline,
        action: Action::Cmd("POLYLINE"),
    },
    Entry {
        name: "Circle",
        hint: "C",
        keywords: "draw round",
        group: "Tools",
        icon: Icon::Circle,
        action: Action::Cmd("CIRCLE"),
    },
    Entry {
        name: "Ellipse",
        hint: "E",
        keywords: "draw oval",
        group: "Tools",
        icon: Icon::Ellipse,
        action: Action::Cmd("ELLIPSE"),
    },
    Entry {
        name: "Arc (3 points)",
        hint: "A",
        keywords: "draw curve",
        group: "Tools",
        icon: Icon::Arc,
        action: Action::Cmd("ARC"),
    },
    Entry {
        name: "Rectangle",
        hint: "R",
        keywords: "draw box square",
        group: "Tools",
        icon: Icon::Rectangle,
        action: Action::Cmd("RECTANGLE"),
    },
    Entry {
        name: "Polygon",
        hint: "G",
        keywords: "draw hexagon sides",
        group: "Tools",
        icon: Icon::Polygon,
        action: Action::Cmd("POLYGON"),
    },
    Entry {
        name: "Spline",
        hint: "S",
        keywords: "draw bezier curve",
        group: "Tools",
        icon: Icon::Spline,
        action: Action::Cmd("SPLINE"),
    },
    Entry {
        name: "Text",
        hint: "T",
        keywords: "draw label annotate",
        group: "Tools",
        icon: Icon::Text,
        action: Action::Cmd("TEXT"),
    },
    Entry {
        name: "Move",
        hint: "Shift M",
        keywords: "modify translate",
        group: "Modify",
        icon: Icon::Move,
        action: Action::Cmd("MOVE"),
    },
    Entry {
        name: "Copy",
        hint: "Shift C",
        keywords: "modify duplicate",
        group: "Modify",
        icon: Icon::Copy,
        action: Action::Cmd("COPY"),
    },
    Entry {
        name: "Rotate",
        hint: "Shift R",
        keywords: "modify turn angle",
        group: "Modify",
        icon: Icon::Rotate,
        action: Action::Cmd("ROTATE"),
    },
    Entry {
        name: "Scale",
        hint: "Shift A",
        keywords: "modify resize",
        group: "Modify",
        icon: Icon::Scale,
        action: Action::Cmd("SCALE"),
    },
    Entry {
        name: "Mirror",
        hint: "Shift I",
        keywords: "modify reflect flip",
        group: "Modify",
        icon: Icon::Mirror,
        action: Action::Cmd("MIRROR"),
    },
    Entry {
        name: "Offset",
        hint: "Shift O",
        keywords: "modify parallel",
        group: "Modify",
        icon: Icon::Offset,
        action: Action::Cmd("OFFSET"),
    },
    Entry {
        name: "Trim",
        hint: "Shift T",
        keywords: "modify cut",
        group: "Modify",
        icon: Icon::Trim,
        action: Action::Cmd("TRIM"),
    },
    Entry {
        name: "Extend",
        hint: "Shift E",
        keywords: "modify lengthen",
        group: "Modify",
        icon: Icon::Extend,
        action: Action::Cmd("EXTEND"),
    },
    Entry {
        name: "Fillet",
        hint: "Shift F",
        keywords: "modify round corner radius",
        group: "Modify",
        icon: Icon::Fillet,
        action: Action::Cmd("FILLET"),
    },
    Entry {
        name: "Chamfer",
        hint: "Shift H",
        keywords: "modify bevel corner",
        group: "Modify",
        icon: Icon::Chamfer,
        action: Action::Cmd("CHAMFER"),
    },
    Entry {
        name: "Blend",
        hint: "Shift B",
        keywords: "modify connect spline continuity g0 g1 g2 g3 tangent curvature join smooth",
        group: "Modify",
        icon: Icon::Blend,
        action: Action::Cmd("BLEND"),
    },
    Entry {
        name: "Stretch",
        hint: "Shift S",
        keywords: "modify deform window",
        group: "Modify",
        icon: Icon::Stretch,
        action: Action::Cmd("STRETCH"),
    },
    Entry {
        name: "Disjoint",
        hint: "Shift X",
        keywords: "explode ungroup separate break apart",
        group: "Modify",
        icon: Icon::Explode,
        action: Action::Cmd("DISJOINT"),
    },
    Entry {
        name: "Join",
        hint: "Shift J",
        keywords: "modify merge connect weld combine",
        group: "Modify",
        icon: Icon::Join,
        action: Action::Cmd("JOIN"),
    },
    Entry {
        name: "Hatch",
        hint: "H",
        keywords: "fill region solid boundary area",
        group: "Modify",
        icon: Icon::Hatch,
        action: Action::Cmd("HATCH"),
    },
    Entry {
        name: "Erase",
        hint: "Del",
        keywords: "delete remove",
        group: "Modify",
        icon: Icon::Delete,
        action: Action::Cmd("ERASE"),
    },
    Entry {
        name: "Zoom Extents",
        hint: "Z",
        keywords: "fit view all frame",
        group: "View",
        icon: Icon::ZoomFit,
        action: Action::Cmd("ZOOM E"),
    },
    Entry {
        name: "Select All",
        hint: "ALL",
        keywords: "everything",
        group: "View",
        icon: Icon::Select,
        action: Action::Cmd("ALL"),
    },
    Entry {
        name: "Undo",
        hint: "Ctrl Z",
        keywords: "back revert",
        group: "View",
        icon: Icon::Undo,
        action: Action::Cmd("UNDO"),
    },
    Entry {
        name: "Redo",
        hint: "Ctrl Y",
        keywords: "forward again",
        group: "View",
        icon: Icon::Redo,
        action: Action::Cmd("REDO"),
    },
    Entry {
        name: "Toggle Object Snap",
        hint: "F7",
        keywords: "snap osnap endpoint midpoint",
        group: "View",
        icon: Icon::Select,
        action: Action::ToggleSnap,
    },
    Entry {
        name: "Toggle Grid",
        hint: "F8",
        keywords: "view background lines",
        group: "View",
        icon: Icon::ZoomFit,
        action: Action::ToggleGrid,
    },
    Entry {
        name: "Toggle Snap to Grid",
        hint: "F9",
        keywords: "gsnap grid snap step",
        group: "View",
        icon: Icon::ZoomFit,
        action: Action::ToggleGridSnap,
    },
    Entry {
        name: "Toggle Guides (Polar Tracking)",
        hint: "F10",
        keywords: "guides polar angle 45",
        group: "View",
        icon: Icon::Pan,
        action: Action::TogglePolar,
    },
    Entry {
        name: "Toggle Track (Extension Tracking)",
        hint: "F11",
        keywords: "track extension guide colinear axis",
        group: "View",
        icon: Icon::Pan,
        action: Action::ToggleTrack,
    },
    Entry {
        name: "Toggle Dynamic Input",
        hint: "F12",
        keywords: "dyn hud length angle",
        group: "View",
        icon: Icon::Pan,
        action: Action::ToggleDyn,
    },
    Entry {
        name: "Toggle Ortho",
        hint: "",
        keywords: "horizontal vertical lock",
        group: "View",
        icon: Icon::Pan,
        action: Action::ToggleOrtho,
    },
];

const GROUP_ORDER: [&str; 3] = ["Tools", "Modify", "View"];

fn score(entry: &Entry, q: &str) -> Option<u8> {
    if q.is_empty() {
        return Some(3);
    }
    let name = entry.name.to_ascii_lowercase();
    if name.starts_with(q) {
        return Some(0);
    }
    if name.split_whitespace().any(|w| w.starts_with(q)) {
        return Some(1);
    }
    if name.contains(q) {
        return Some(2);
    }
    if entry.hint.to_ascii_lowercase() == q {
        return Some(0);
    }
    if entry.keywords.split_whitespace().any(|w| w.starts_with(q)) {
        return Some(2);
    }
    None
}

fn run_entry(app: &mut AppState, e: &Entry) {
    match e.action {
        Action::Cmd(c) => app.run_command(c),
        Action::ToggleGrid => app.grid_on = !app.grid_on,
        Action::ToggleSnap => app.snap_on = !app.snap_on,
        Action::ToggleGridSnap => app.grid_snap_on = !app.grid_snap_on,
        Action::ToggleOrtho => {
            app.ortho_on = !app.ortho_on;
            if app.ortho_on {
                app.polar_on = false;
            }
        }
        Action::TogglePolar => {
            app.polar_on = !app.polar_on;
            if app.polar_on {
                app.ortho_on = false;
            }
        }
        Action::ToggleTrack => app.track_on = !app.track_on,
        Action::ToggleDyn => app.dyn_on = !app.dyn_on,
    }
}

fn keycap(ui: &mut egui::Ui, text: &str) {
    let galley = ui.painter().layout_no_wrap(
        text.to_string(),
        egui::FontId::monospace(11.0),
        theme::TEXT_DIM,
    );
    let w = galley.size().x + 12.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 18.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        5.0,
        theme::WIDGET_BG,
        egui::Stroke::new(1.0, theme::OUTLINE),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(11.0),
        theme::TEXT_DIM,
    );
}

fn command_row(ui: &mut egui::Ui, e: &Entry, selected: bool) -> egui::Response {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(rect, 10.0, theme::WIDGET_HOVER);
    }
    let icon_box = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 8.0, rect.center().y - 15.0),
        egui::vec2(30.0, 30.0),
    );
    ui.painter().rect(
        icon_box,
        8.0,
        theme::WIDGET_BG,
        egui::Stroke::new(1.0, theme::OUTLINE),
        egui::StrokeKind::Inside,
    );
    crate::icons::paint_icon(
        &ui.painter_at(rect),
        ui.ctx(),
        e.icon,
        icon_box.shrink(7.0),
        egui::Color32::from_rgb(210, 224, 244),
    );
    ui.painter().text(
        egui::pos2(icon_box.right() + 12.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        e.name,
        egui::FontId::proportional(14.0),
        theme::TEXT,
    );
    if !e.hint.is_empty() {
        let mut keys: Vec<&str> = e.hint.split_whitespace().collect();
        keys.reverse();
        let mut x = rect.right() - 12.0;
        for k in keys {
            let galley = ui.painter().layout_no_wrap(
                k.to_string(),
                egui::FontId::monospace(11.0),
                theme::TEXT_DIM,
            );
            let w = galley.size().x + 12.0;
            let kr = egui::Rect::from_min_size(
                egui::pos2(x - w, rect.center().y - 9.0),
                egui::vec2(w, 18.0),
            );
            ui.painter().rect(
                kr,
                5.0,
                theme::WIDGET_BG,
                egui::Stroke::new(1.0, theme::OUTLINE),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                kr.center(),
                egui::Align2::CENTER_CENTER,
                k,
                egui::FontId::monospace(11.0),
                theme::TEXT_DIM,
            );
            x -= w + 5.0;
        }
    }
    resp
}

pub(super) fn command_bar(
    ctx: &Context,
    app: &mut AppState,
    ui_state: &mut UiState,
    canvas_rect: egui::Rect,
) -> bool {
    let bar_id = egui::Id::new("command_line_input");
    let open_id = egui::Id::new("palette_open_state");
    let mut open = ctx.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));

    let menu_request = ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("open_palette"))
            .unwrap_or(false)
    });
    if menu_request {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_palette"), false));
    }
    let mut focus_request = false;
    let kbd_open = ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::F));
    if kbd_open || menu_request {
        open = true;
        focus_request = true;
        ui_state.command_input.clear();
        ui_state.palette_index = 0;
        ui_state.palette_nav = false;
    }

    if !open {
        ctx.data_mut(|d| d.insert_temp(open_id, false));
        return false;
    }

    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
        ctx.memory_mut(|m| m.surrender_focus(bar_id));
        ctx.data_mut(|d| d.insert_temp(open_id, false));
        return false;
    }

    let focused = ctx.memory(|m| m.has_focus(bar_id));
    let q = ui_state.command_input.trim().to_ascii_lowercase();

    let visible: Vec<&Entry> = if q.is_empty() {
        let mut v = Vec::new();
        for g in GROUP_ORDER {
            v.extend(ENTRIES.iter().filter(|e| e.group == g));
        }
        v
    } else {
        let mut scored: Vec<(&Entry, u8)> = ENTRIES
            .iter()
            .filter_map(|e| score(e, &q).map(|s| (e, s)))
            .collect();
        scored.sort_by_key(|&(_, s)| s);
        scored.into_iter().map(|(e, _)| e).collect()
    };

    let mut index = ui_state.palette_index.min(visible.len().saturating_sub(1));
    let mut nav = ui_state.palette_nav;
    if focused && !visible.is_empty() {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
            index = (index + 1) % visible.len();
            nav = true;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
            index = (index + visible.len() - 1) % visible.len();
            nav = true;
        }
    }

    let mut run_idx: Option<usize> = None;
    let mut run_raw = false;
    let mut close_requested = false;

    let screen = ctx.content_rect();
    let backdrop = egui::Area::new(egui::Id::new("palette_backdrop"))
        .order(egui::Order::Tooltip)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.painter()
                .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(140));
            ui.allocate_rect(screen, egui::Sense::click())
        });
    if backdrop.inner.clicked() {
        close_requested = true;
    }

    let width = 600.0_f32.min(canvas_rect.width() - 48.0);
    let pos = egui::pos2(
        canvas_rect.center().x - width / 2.0,
        screen.top() + screen.height() * 0.13,
    );
    let card = egui::Area::new(egui::Id::new("command_palette"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_width(width);
            theme::glass(theme::tok::R_LG)
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.set_width(width);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(16, 14))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(
                                    egui::vec2(20.0, 20.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_stroke(
                                    r.center() - egui::vec2(2.0, 2.0),
                                    6.0,
                                    egui::Stroke::new(1.6, theme::TEXT_DIM),
                                );
                                ui.painter().line_segment(
                                    [
                                        r.center() + egui::vec2(2.5, 2.5),
                                        r.center() + egui::vec2(7.0, 7.0),
                                    ],
                                    egui::Stroke::new(1.6, theme::TEXT_DIM),
                                );
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut ui_state.command_input)
                                        .id(bar_id)
                                        .frame(egui::Frame::NONE)
                                        .hint_text("Search tools, modify operations, views\u{2026}")
                                        .desired_width(f32::INFINITY)
                                        .margin(egui::Margin::ZERO),
                                );
                                if focus_request {
                                    resp.request_focus();
                                }
                                if resp.changed() {
                                    index = 0;
                                    nav = false;
                                }
                                let enter = (resp.lost_focus() || resp.has_focus())
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if enter {
                                    if !visible.is_empty() {
                                        run_idx = Some(index.min(visible.len() - 1));
                                    } else {
                                        run_raw = true;
                                    }
                                }
                                keycap(ui, "ESC");
                            });
                        });
                    super::chrome::divider_h(ui);

                    egui::ScrollArea::vertical()
                        .max_height(screen.height() * 0.46)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::same(8))
                                .show(ui, |ui| {
                                    ui.set_width(width - 16.0);
                                    if visible.is_empty() {
                                        ui.add_space(24.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "No commands match “{q}”"
                                                ))
                                                .color(theme::TEXT_DIM),
                                            );
                                        });
                                        ui.add_space(24.0);
                                    }
                                    let mut last_group = "";
                                    for (i, e) in visible.iter().enumerate() {
                                        if q.is_empty() && e.group != last_group {
                                            last_group = e.group;
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(e.group.to_uppercase())
                                                    .font(crate::fonts::strong_font_id(
                                                        ui.ctx(),
                                                        10.0,
                                                    ))
                                                    .color(theme::TEXT_DIM),
                                            );
                                            ui.add_space(2.0);
                                        }
                                        let selected = nav && i == index;
                                        let r = command_row(ui, e, selected);
                                        if r.clicked() {
                                            run_idx = Some(i);
                                        }
                                        if r.hovered() {
                                            index = i;
                                        }
                                    }
                                });
                        });

                    super::chrome::divider_h(ui);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(16, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                keycap(ui, "↑↓");
                                ui.label(
                                    egui::RichText::new("navigate")
                                        .size(12.0)
                                        .color(theme::TEXT_DIM),
                                );
                                ui.add_space(8.0);
                                keycap(ui, "↵");
                                ui.label(
                                    egui::RichText::new("select")
                                        .size(12.0)
                                        .color(theme::TEXT_DIM),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(concat!(
                                                "oxiDRAFT · v",
                                                env!("CARGO_PKG_VERSION")
                                            ))
                                            .monospace()
                                            .size(11.0)
                                            .color(theme::TEXT_DIM),
                                        );
                                    },
                                );
                            });
                        });
                });
        });
    ctx.move_to_top(card.response.layer_id);

    if let Some(i) = run_idx {
        if i < visible.len() {
            run_entry(app, visible[i]);
            ui_state.command_input.clear();
            ctx.memory_mut(|m| m.surrender_focus(bar_id));
        }
        index = 0;
        nav = false;
        open = false;
    } else if run_raw {
        let text = std::mem::take(&mut ui_state.command_input);
        app.run_command(text.trim());
        ctx.memory_mut(|m| m.surrender_focus(bar_id));
        index = 0;
        nav = false;
        open = false;
    } else if close_requested {
        ctx.memory_mut(|m| m.surrender_focus(bar_id));
        open = false;
    }

    ui_state.palette_index = index;
    ui_state.palette_nav = nav;
    ctx.data_mut(|d| d.insert_temp(open_id, open));
    open
}
