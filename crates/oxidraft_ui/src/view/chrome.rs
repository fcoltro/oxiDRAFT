use super::UiState;
use crate::command::Command;
use crate::state::AppState;
use crate::tools::Tool;
use egui::{Color32, Context};
use oxidraft_document::{EntityKind, Layer};
use oxidraft_geometry::{Curve, Point2d};
use rfd::FileDialog;

pub(super) fn handle_shortcuts(ctx: &Context, app: &mut AppState, ui_state: &mut UiState) {
    let title = app.window_title();
    if ui_state.last_title != title {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        ui_state.last_title = title;
    }
    let (ctrl, shift) = ctx.input(|i| (i.modifiers.ctrl, i.modifiers.shift));
    let s_key = ctx.input(|i| i.key_pressed(egui::Key::S));
    let n_key = ctx.input(|i| i.key_pressed(egui::Key::N));
    let o_key = ctx.input(|i| i.key_pressed(egui::Key::O));
    if ctrl && n_key && maybe_save(app) {
        app.new_document();
    }
    if ctrl && o_key && maybe_save(app) {
        file_open(app);
    }
    let save_as_key = ctrl && shift && s_key;
    let save_key = ctrl && !shift && s_key;
    if save_as_key || (save_key && !app.save_file()) {
        file_save_as(app);
    }
    let typing = ctx.memory(|m| m.focused().is_some());
    if !typing {
        let z = ctx.input(|i| i.key_pressed(egui::Key::Z));
        let y = ctx.input(|i| i.key_pressed(egui::Key::Y));
        if ctrl && ((z && shift) || y) {
            app.redo();
        } else if ctrl && z {
            app.undo();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::A)) {
            app.execute(Command::SelectAll);
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::C)) {
            app.clipboard_copy();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::X)) {
            app.clipboard_cut();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::V)) {
            app.clipboard_paste();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            app.erase_selection();
        }
    }
}

pub(super) fn top_bar(ctx: &Context, app: &mut AppState, canvas_rect: egui::Rect) {
    let margin = 12.0;
    let pos = canvas_rect.left_top() + egui::vec2(margin, margin);
    let width = canvas_rect.width() - 2.0 * margin;
    egui::Area::new(egui::Id::new("top_bar_pill"))
        .fixed_pos(pos)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(width);
            crate::theme::glass(crate::theme::tok::R_LG)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.set_width(width - 20.0);
                    ui.set_height(34.0);
                    ui.horizontal_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(app.document_label())
                                .size(13.0)
                                .color(crate::theme::TEXT),
                        );
                        {
                            let (dot_color, status) = if app.current_file_path.is_none() {
                                (crate::theme::STATUS_RED, "Not saved yet")
                            } else if app.is_dirty() {
                                (crate::theme::STATUS_AMBER, "Unsaved changes")
                            } else {
                                (crate::theme::STATUS_GREEN, "All changes saved")
                            };
                            let (rect, resp) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 3.0, dot_color);
                            resp.on_hover_text(status);
                        }
                        ui.add_space(6.0);
                        ui.scope(|ui| {
                            ui.style_mut().visuals.override_text_color =
                                Some(Color32::from_rgb(203, 212, 226));
                            menu_items(ui, app);
                        });
                        ui.add_space(2.0);
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.add_enabled_ui(app.history.can_undo(), |ui| {
                                if crate::icons::icon_button_sized(
                                    ui,
                                    crate::icons::Icon::Undo,
                                    "Undo  (Ctrl+Z)",
                                    false,
                                    30.0,
                                )
                                .clicked()
                                {
                                    app.undo();
                                }
                            });
                            ui.add_enabled_ui(app.history.can_redo(), |ui| {
                                if crate::icons::icon_button_sized(
                                    ui,
                                    crate::icons::Icon::Redo,
                                    "Redo  (Ctrl+Y)",
                                    false,
                                    30.0,
                                )
                                .clicked()
                                {
                                    app.redo();
                                }
                            });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(2.0);
                            if ui
                                .add(export_button())
                                .on_hover_text("Export DXF / SVG")
                                .clicked()
                            {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(egui::Id::new("open_export"), true)
                                });
                            }
                            ui.add_space(8.0);
                            if ui.add(search_button()).clicked() {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(egui::Id::new("open_palette"), true)
                                });
                            }
                        });
                    });
                });
        });
    export_menu(ctx, app);
}

fn search_button() -> impl egui::Widget {
    move |ui: &mut egui::Ui| {
        let desired = egui::vec2(264.0, 32.0);
        let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
        let hov = resp.hovered();
        let fill = if hov {
            crate::theme::WIDGET_HOVER
        } else {
            crate::theme::WIDGET_BG
        };
        let p = ui.painter();
        p.rect(
            rect,
            9.0,
            fill,
            egui::Stroke::new(1.0, crate::theme::OUTLINE),
            egui::StrokeKind::Inside,
        );
        p.circle_stroke(
            egui::pos2(rect.left() + 15.0, rect.center().y),
            4.5,
            egui::Stroke::new(1.4, crate::theme::TEXT_DIM),
        );
        p.text(
            egui::pos2(rect.left() + 28.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Search or run a command",
            egui::FontId::proportional(12.5),
            crate::theme::TEXT_DIM,
        );
        let cap = |p: &egui::Painter, right: f32, text: &str| -> f32 {
            let galley = p.layout_no_wrap(
                text.to_string(),
                egui::FontId::monospace(10.0),
                crate::theme::TEXT_DIM,
            );
            let w = galley.size().x + 10.0;
            let kr = egui::Rect::from_min_size(
                egui::pos2(right - w, rect.center().y - 9.0),
                egui::vec2(w, 18.0),
            );
            p.rect(
                kr,
                5.0,
                crate::theme::WIDGET_BG,
                egui::Stroke::new(1.0, crate::theme::OUTLINE),
                egui::StrokeKind::Inside,
            );
            p.text(
                kr.center(),
                egui::Align2::CENTER_CENTER,
                text,
                egui::FontId::monospace(10.0),
                crate::theme::TEXT_DIM,
            );
            kr.left()
        };
        let mut right = rect.right() - 10.0;
        right = cap(p, right, "F") - 4.0;
        cap(p, right, "Ctrl");
        resp
    }
}

fn export_button() -> impl egui::Widget {
    move |ui: &mut egui::Ui| {
        let desired = egui::vec2(86.0, 30.0);
        let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());
        let fill = if resp.hovered() {
            crate::theme::ACCENT_BRIGHT
        } else {
            crate::theme::ACCENT
        };
        let p = ui.painter();
        p.rect_filled(rect, 9.0, fill);
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "Export",
            egui::FontId::proportional(12.5),
            Color32::WHITE,
        );
        resp
    }
}

fn export_menu(ctx: &Context, app: &mut AppState) {
    if !ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("open_export"))
            .unwrap_or(false)
    }) {
        return;
    }
    let mut open = true;
    egui::Window::new("Export")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(220.0);
            if ui.button("Export DXF…").clicked() {
                if let Some(path) = FileDialog::new().add_filter("DXF", &["dxf"]).save_file() {
                    let content = oxidraft_io::export_dxf(&app.document);
                    if let Err(e) = oxidraft_io::write_atomic(&path, content.as_bytes()) {
                        app.command_log.push(format!("DXF export failed: {e}"));
                    }
                }
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_export"), false));
            }
            if ui.button("Export SVG…").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("SVG image", &["svg"])
                    .save_file()
                {
                    let content = oxidraft_io::export_svg(&app.document);
                    if let Err(e) = oxidraft_io::write_atomic(&path, content.as_bytes()) {
                        app.command_log.push(format!("SVG export failed: {e}"));
                    }
                }
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_export"), false));
            }
            ui.separator();
            if ui.button("Plot…").clicked() {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_export"), false));
                app.plot_dialog_open = true;
            }
        });
    if !open {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_export"), false));
    }
}

const PLOT_PRESET_ID: &str = "plot_paper_preset";

const PLOT_CUSTOM_W_ID: &str = "plot_custom_w_mm";

const PLOT_CUSTOM_H_ID: &str = "plot_custom_h_mm";

const PLOT_LANDSCAPE_ID: &str = "plot_landscape";

pub(super) fn plot_dialog(ctx: &Context, app: &mut AppState) {
    if !app.plot_dialog_open {
        return;
    }
    let preset_id = egui::Id::new(PLOT_PRESET_ID);
    let custom_w_id = egui::Id::new(PLOT_CUSTOM_W_ID);
    let custom_h_id = egui::Id::new(PLOT_CUSTOM_H_ID);
    let landscape_id = egui::Id::new(PLOT_LANDSCAPE_ID);
    let mut window_mode = app.plot_window_mode;
    let mut preset = ctx.data(|d| d.get_temp::<usize>(preset_id)).unwrap_or(0);
    let mut custom_w = ctx
        .data(|d| d.get_temp::<f64>(custom_w_id))
        .unwrap_or(210.0);
    let mut custom_h = ctx
        .data(|d| d.get_temp::<f64>(custom_h_id))
        .unwrap_or(297.0);
    let mut landscape = ctx
        .data(|d| d.get_temp::<bool>(landscape_id))
        .unwrap_or(false);
    let is_custom = preset == oxidraft_io::PAPER_PRESETS.len();
    let mut open = true;
    let mut close_after_plot = false;
    let mut start_pick = false;
    egui::Window::new("Plot")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_width(260.0);
            ui.label(
                egui::RichText::new("Renders the drawing onto one PDF page, fit to the paper.")
                    .size(11.5)
                    .color(crate::theme::TEXT_DIM),
            );
            ui.add_space(8.0);
            setting_row(ui, "Plot area", |ui| {
                ui.selectable_value(&mut window_mode, false, "Extents");
                ui.selectable_value(&mut window_mode, true, "Window");
            });
            if window_mode {
                setting_row(ui, "Window", |ui| {
                    match app.plot_window {
                        Some((x0, y0, x1, y1)) => {
                            ui.label(
                                egui::RichText::new(format!("{:.1} × {:.1}", x1 - x0, y1 - y0))
                                    .size(11.5),
                            );
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("not set")
                                    .size(11.5)
                                    .color(crate::theme::TEXT_DIM),
                            );
                        }
                    }
                    if ui
                        .button("Pick ⌖")
                        .on_hover_text("Pick two corners on the canvas; the dialog reopens after")
                        .clicked()
                    {
                        app.tool = crate::tools::Tool::PlotWindow { first: None };
                        app.selection.clear();
                        start_pick = true;
                    }
                });
            }
            setting_row(ui, "Paper size", |ui| {
                let selected_text = if is_custom {
                    "Custom".to_string()
                } else {
                    oxidraft_io::PAPER_PRESETS[preset].0.to_string()
                };
                egui::ComboBox::from_id_salt("plot_paper_combo")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in oxidraft_io::PAPER_PRESETS.iter().enumerate() {
                            ui.selectable_value(&mut preset, i, *name);
                        }
                        ui.selectable_value(
                            &mut preset,
                            oxidraft_io::PAPER_PRESETS.len(),
                            "Custom",
                        );
                    });
            });
            if is_custom {
                setting_row(ui, "Width (mm)", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut custom_w)
                            .speed(1.0)
                            .range(10.0..=5000.0)
                            .max_decimals(1),
                    );
                });
                setting_row(ui, "Height (mm)", |ui| {
                    ui.add(
                        egui::DragValue::new(&mut custom_h)
                            .speed(1.0)
                            .range(10.0..=5000.0)
                            .max_decimals(1),
                    );
                });
            }
            setting_row(ui, "Orientation", |ui| {
                ui.selectable_value(&mut landscape, false, "Portrait");
                ui.selectable_value(&mut landscape, true, "Landscape");
            });
            ui.add_space(10.0);
            let ready = !window_mode || app.plot_window.is_some();
            let plot_clicked = ui
                .add_enabled(ready, egui::Button::new("Plot to PDF…"))
                .on_disabled_hover_text("Pick a plot window first")
                .clicked();
            if plot_clicked {
                let paper = if is_custom {
                    oxidraft_io::PaperSize::new(custom_w, custom_h)
                } else {
                    oxidraft_io::PAPER_PRESETS[preset].1
                };
                let paper = if landscape {
                    paper.landscape()
                } else {
                    paper.portrait()
                };
                let window = window_mode
                    .then(|| {
                        app.plot_window
                            .map(|(x0, y0, x1, y1)| oxidraft_io::PlotWindow { x0, y0, x1, y1 })
                    })
                    .flatten();
                if let Some(path) = FileDialog::new().add_filter("PDF", &["pdf"]).save_file() {
                    match oxidraft_io::export_pdf_window(&app.document, paper, window) {
                        Ok(bytes) => {
                            if let Err(e) = oxidraft_io::write_atomic(&path, &bytes) {
                                app.command_log.push(format!("Plot failed: {e}"));
                            } else {
                                app.command_log.push("Plotted to PDF".to_string());
                            }
                        }
                        Err(e) => app.command_log.push(format!("Plot failed: {e}")),
                    }
                }
                close_after_plot = true;
            }
        });
    ctx.data_mut(|d| {
        d.insert_temp(preset_id, preset);
        d.insert_temp(custom_w_id, custom_w);
        d.insert_temp(custom_h_id, custom_h);
        d.insert_temp(landscape_id, landscape);
    });
    app.plot_window_mode = window_mode;
    if !open || close_after_plot || start_pick {
        app.plot_dialog_open = false;
    }
}

fn menu_items(ui: &mut egui::Ui, app: &mut AppState) {
    ui.spacing_mut().item_spacing.x = 12.0;
    ui.menu_button("File", |ui| {
        if ui
            .add(egui::Button::new("New").shortcut_text("Ctrl+N"))
            .clicked()
        {
            if maybe_save(app) {
                app.new_document();
            }
            ui.close();
        }
        if ui
            .add(egui::Button::new("Open…").shortcut_text("Ctrl+O"))
            .clicked()
        {
            if maybe_save(app) {
                file_open(app);
            }
            ui.close();
        }
        if ui
            .add(egui::Button::new("Save").shortcut_text("Ctrl+S"))
            .clicked()
        {
            if !app.save_file() {
                file_save_as(app);
            }
            ui.close();
        }
        if ui
            .add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S"))
            .clicked()
        {
            file_save_as(app);
            ui.close();
        }
        ui.separator();
        if ui.button("Export DXF…").clicked() {
            if let Some(path) = FileDialog::new().add_filter("DXF", &["dxf"]).save_file() {
                let content = oxidraft_io::export_dxf(&app.document);
                if let Err(e) = oxidraft_io::write_atomic(&path, content.as_bytes()) {
                    app.command_log.push(format!("DXF export failed: {e}"));
                }
            }
            ui.close();
        }
        if ui.button("Export SVG…").clicked() {
            if let Some(path) = FileDialog::new()
                .add_filter("SVG image", &["svg"])
                .save_file()
            {
                let content = oxidraft_io::export_svg(&app.document);
                if let Err(e) = oxidraft_io::write_atomic(&path, content.as_bytes()) {
                    app.command_log.push(format!("SVG export failed: {e}"));
                }
            }
            ui.close();
        }
        ui.separator();
        if ui.button("Plot…").clicked() {
            app.plot_dialog_open = true;
            ui.close();
        }
    });
    ui.menu_button("Edit", |ui| {
        if ui
            .add_enabled(
                app.history.can_undo(),
                egui::Button::new("Undo").shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            app.undo();
        }
        if ui
            .add_enabled(
                app.history.can_redo(),
                egui::Button::new("Redo").shortcut_text("Ctrl+Y"),
            )
            .clicked()
        {
            app.redo();
        }
        ui.separator();
        let has_sel = app.has_selection();
        if ui
            .add_enabled(has_sel, egui::Button::new("Cut").shortcut_text("Ctrl+X"))
            .clicked()
        {
            app.clipboard_cut();
            ui.close();
        }
        if ui
            .add_enabled(has_sel, egui::Button::new("Copy").shortcut_text("Ctrl+C"))
            .clicked()
        {
            app.clipboard_copy();
            ui.close();
        }
        if ui
            .add_enabled(
                !app.clipboard.is_empty(),
                egui::Button::new("Paste").shortcut_text("Ctrl+V"),
            )
            .on_hover_text("Paste at the cursor")
            .clicked()
        {
            app.clipboard_paste();
            ui.close();
        }
        ui.separator();
        if ui
            .add_enabled(has_sel, egui::Button::new("Erase").shortcut_text("Del"))
            .clicked()
        {
            app.erase_selection();
        }
        if ui
            .add(egui::Button::new("Select All").shortcut_text("Ctrl+A"))
            .clicked()
        {
            app.execute(Command::SelectAll);
        }
        ui.separator();
        if ui
            .add(egui::Button::new("Command Palette…").shortcut_text("Ctrl+F"))
            .clicked()
        {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("open_palette"), true));
        }
        ui.separator();
        if ui.button("Settings…").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("open_settings"), true));
            ui.close();
        }
    });
    ui.menu_button("View", |ui| {
        if ui
            .add(egui::Button::new("Zoom Extents").shortcut_text("Z"))
            .clicked()
        {
            app.zoom_extents();
        }
        ui.separator();
        ui.checkbox(&mut app.snap_on, "Object Snap  (F7)");
        ui.checkbox(&mut app.grid_on, "Grid  (F8)");
        ui.checkbox(&mut app.grid_snap_on, "Snap to Grid  (F9)");
        let mut polar = app.polar_on;
        if ui
            .checkbox(&mut polar, "Guides — Polar Tracking  (F10)")
            .changed()
        {
            app.polar_on = polar;
            if polar {
                app.ortho_on = false;
            }
        }
        ui.checkbox(&mut app.track_on, "Track — Extension Tracking  (F11)");
        ui.checkbox(&mut app.dyn_on, "Dynamic Input  (F12)");
        ui.separator();
        ui.checkbox(&mut app.comb_on, "Curvature Comb");
        ui.separator();
        if ui.button("Reset Tool Options").clicked() {
            app.apply_prefs(&crate::state::UiPrefs::default());
            ui.close();
        }
    });
    ui.menu_button("Draw", |ui| {
        tool_menu_item(ui, app, "Select", Tool::Select);
        ui.separator();
        tool_menu_item(ui, app, "Line", Tool::Line { last: None });
        tool_menu_item(ui, app, "Tangent Line", Tool::TangentLine { first: None });
        tool_menu_item(ui, app, "Circle", Tool::Circle { center: None });
        ui.menu_button("Circle", |ui| {
            tool_menu_item(ui, app, "Center, Radius", Tool::Circle { center: None });
            tool_menu_item(
                ui,
                app,
                "2 Points (diameter)",
                Tool::CircleTwoPoint { first: None },
            );
            tool_menu_item(ui, app, "3 Points", Tool::CircleThreePoint { pts: vec![] });
            tool_menu_item(
                ui,
                app,
                "Tan, Tan, Radius",
                Tool::CircleTtr {
                    radius: 1.0,
                    first: None,
                },
            );
            tool_menu_item(ui, app, "Tan, Tan, Tan", Tool::CircleTtt { picks: vec![] });
        });
        tool_menu_item(
            ui,
            app,
            "Ellipse",
            Tool::Ellipse {
                center: None,
                axis_end: None,
            },
        );
        tool_menu_item(ui, app, "Arc", Tool::Arc3 { pts: vec![] });
        ui.menu_button("Arc", |ui| {
            tool_menu_item(ui, app, "3 Points", Tool::Arc3 { pts: vec![] });
            tool_menu_item(
                ui,
                app,
                "Start, Center, End",
                Tool::ArcStartCenterEnd {
                    start: None,
                    center: None,
                },
            );
            tool_menu_item(
                ui,
                app,
                "Center, Start, End",
                Tool::ArcCenterStartEnd {
                    center: None,
                    start: None,
                },
            );
        });
        tool_menu_item(ui, app, "Rectangle", Tool::Rectangle { first: None });
        tool_menu_item(
            ui,
            app,
            "Polygon",
            Tool::Polygon {
                center: None,
                radius_point: None,
                sides: None,
            },
        );
        tool_menu_item(ui, app, "Spline", Tool::Spline { pts: vec![] });
        tool_menu_item(ui, app, "Polyline", Tool::Polyline { pts: vec![] });
        tool_menu_item(
            ui,
            app,
            "Text",
            Tool::Text {
                anchor: None,
                height: 2.5,
            },
        );
        ui.separator();
        ui.menu_button("Dimension", |ui| {
            tool_menu_item(
                ui,
                app,
                "Linear (aligned)",
                Tool::Dimension { p1: None, p2: None },
            );
            tool_menu_item(
                ui,
                app,
                "Angular (2 lines)",
                Tool::DimAngularLines {
                    a: None,
                    geom: None,
                },
            );
            tool_menu_item(
                ui,
                app,
                "Radius",
                Tool::DimRadial {
                    diameter: false,
                    center: None,
                    radius: 0.0,
                },
            );
            tool_menu_item(
                ui,
                app,
                "Diameter",
                Tool::DimRadial {
                    diameter: true,
                    center: None,
                    radius: 0.0,
                },
            );
        });
    });
    ui.menu_button("Modify", |ui| {
        tool_menu_item(
            ui,
            app,
            "Move",
            Tool::Move {
                base: None,
                ids: vec![],
            },
        );
        tool_menu_item(
            ui,
            app,
            "Copy",
            Tool::Copy {
                base: None,
                ids: vec![],
            },
        );
        tool_menu_item(
            ui,
            app,
            "Rotate",
            Tool::Rotate {
                base: None,
                ids: vec![],
            },
        );
        tool_menu_item(
            ui,
            app,
            "Scale",
            Tool::Scale {
                base: None,
                reference: None,
                ids: vec![],
            },
        );
        tool_menu_item(
            ui,
            app,
            "Mirror",
            Tool::Mirror {
                first: None,
                ids: vec![],
            },
        );
        tool_menu_item(
            ui,
            app,
            "Stretch",
            Tool::Stretch {
                c1: None,
                c2: None,
                base: None,
                ids: vec![],
            },
        );
        ui.separator();
        tool_menu_item(
            ui,
            app,
            "Offset",
            Tool::Offset {
                dist: 1.0,
                source: None,
            },
        );
        tool_menu_item(ui, app, "Trim", Tool::Trim);
        tool_menu_item(ui, app, "Extend", Tool::Extend);
        tool_menu_item(
            ui,
            app,
            "Fillet",
            Tool::Fillet {
                radius: 1.0,
                first: None,
            },
        );
        tool_menu_item(
            ui,
            app,
            "Chamfer",
            Tool::Chamfer {
                dist: 1.0,
                first: None,
            },
        );
        tool_menu_item(
            ui,
            app,
            "Blend",
            Tool::Blend {
                continuity: oxidraft_geometry::Continuity::G1,
                tension: 1.0,
                first: None,
                second: None,
            },
        );
        ui.separator();
        ui.menu_button("Constrain", |ui| {
            use oxidraft_cad::ConstraintKind;
            for (label, cmd, kind) in [
                ("Horizontal", "HOR", ConstraintKind::Horizontal),
                ("Vertical", "VER", ConstraintKind::Vertical),
                ("Parallel", "PAR", ConstraintKind::Parallel),
                ("Perpendicular", "PERP", ConstraintKind::Perpendicular),
                ("Equal Length", "EQL", ConstraintKind::EqualLength),
                ("Coincident", "COI", ConstraintKind::Coincident),
                ("Tangent", "TANCON", ConstraintKind::Tangent),
            ] {
                if ui
                    .add(egui::Button::new(label).shortcut_text(cmd))
                    .clicked()
                {
                    app.execute(Command::Constrain(kind));
                    ui.close();
                }
            }
            if ui
                .add(egui::Button::new("Lock Radius").shortcut_text("RADCON"))
                .on_hover_text("Hold the current radius; RADCON <value> drives it to a new one")
                .clicked()
            {
                app.execute(Command::ConstrainRadius(None));
                ui.close();
            }
            if ui
                .add(egui::Button::new("Lock Length").shortcut_text("LENCON"))
                .on_hover_text("Hold the current length; LENCON <value> drives it to a new one")
                .clicked()
            {
                app.execute(Command::ConstrainDistance(None));
                ui.close();
            }
            ui.separator();
            if ui
                .add(egui::Button::new("Remove Constraints").shortcut_text("UNCON"))
                .clicked()
            {
                app.execute(Command::Unconstrain);
                ui.close();
            }
        });
        ui.separator();
        if ui
            .add(egui::Button::new("Disjoint").shortcut_text("Shift+X"))
            .clicked()
        {
            app.explode_selection();
            ui.close();
        }
        if ui
            .add(egui::Button::new("Join").shortcut_text("Shift+J"))
            .clicked()
        {
            app.join_selection();
            ui.close();
        }
        if ui
            .add(egui::Button::new("Hatch").shortcut_text("H"))
            .clicked()
        {
            app.execute(Command::Hatch);
            ui.close();
        }
        ui.separator();
        if ui.button("Line Weight & Type…").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("open_line_props"), true));
            ui.close();
        }
    });
    ui.menu_button("Help", |ui| {
        if ui.button("About oxiDRAFT").clicked() {
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::new("open_about"), true));
            ui.close();
        }
    });
}

pub(super) fn about_window(ctx: &Context, ui_state: &mut UiState) {
    if ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("open_about"))
            .unwrap_or(false)
    }) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_about"), false));
        ui_state.about_open = true;
    }
    if !ui_state.about_open {
        return;
    }
    let backdrop = egui::Area::new(egui::Id::new("about_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(ctx.content_rect().min)
        .show(ctx, |ui| {
            let r = ctx.content_rect();
            ui.painter()
                .rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
            ui.allocate_rect(r, egui::Sense::click())
        });
    let mut close = backdrop.inner.clicked();
    egui::Window::new("about_dialog")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(egui::vec2(360.0, 0.0))
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(14.0);
                if let Some(tex) = crate::icons::logo_texture(ui.ctx()) {
                    let s = tex.size_vec2();
                    let w = 280.0;
                    let size = egui::vec2(w, w * s.y / s.x);
                    ui.image(egui::load::SizedTexture::new(tex.id(), size));
                }
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(concat!("Version ", env!("CARGO_PKG_VERSION")))
                        .size(12.0)
                        .color(crate::theme::TEXT_DIM),
                );
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new("Exact, robust 2D CAD")
                        .size(11.0)
                        .color(crate::theme::TEXT_DIM),
                );
                ui.add_space(14.0);
                if ui.button("Close").clicked() {
                    close = true;
                }
                ui.add_space(6.0);
            });
        });
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui_state.about_open = false;
    }
}

pub(super) fn line_props_dialog(ctx: &Context, app: &mut AppState, ui_state: &mut UiState) {
    if ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("open_line_props"))
            .unwrap_or(false)
    }) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_line_props"), false));
        ui_state.line_props_open = true;
    }
    if !ui_state.line_props_open {
        return;
    }
    let backdrop = egui::Area::new(egui::Id::new("line_props_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(ctx.content_rect().min)
        .show(ctx, |ui| {
            let r = ctx.content_rect();
            ui.painter()
                .rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
            ui.allocate_rect(r, egui::Sense::click())
        });
    let mut close = backdrop.inner.clicked();
    let sel = app.selection.clone();
    egui::Window::new("line_props_dialog")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .fixed_size(egui::vec2(320.0, 0.0))
        .show(ctx, |ui| {
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new("Line Weight & Type")
                    .size(14.0)
                    .strong()
                    .color(crate::theme::TEXT),
            );
            prop_section(ui, "NEW OBJECTS");
            ui.push_id("line_props_new", |ui| {
                let dw = app.default_line_weight.clone();
                appearance_row(ui, "Line weight", lw_label(&dw), None, false, |ui| {
                    for (lbl, val) in lw_options() {
                        if ui.selectable_label(dw == val, lbl).clicked() {
                            app.default_line_weight = val;
                            ui.close();
                        }
                    }
                });
                let dt = app.default_line_type.clone();
                appearance_row(ui, "Line type", lt_label(&dt), None, true, |ui| {
                    for (lbl, val) in lt_options() {
                        if ui.selectable_label(dt == val, lbl).clicked() {
                            app.default_line_type = val;
                            ui.close();
                        }
                    }
                });
            });
            if !sel.is_empty() {
                prop_section(ui, &format!("SELECTION ({})", sel.len()));
                ui.push_id("line_props_sel", |ui| {
                    let sw = app.document.get(sel[0]).map(|e| e.line_weight.clone());
                    let sw_lbl = sw.as_ref().map(lw_label).unwrap_or_else(|| "—".into());
                    appearance_row(ui, "Line weight", sw_lbl, None, false, |ui| {
                        for (lbl, val) in lw_options() {
                            if ui
                                .selectable_label(sw.as_ref() == Some(&val), lbl)
                                .clicked()
                            {
                                app.history.snapshot(&app.document);
                                for &id in &sel {
                                    if let Some(e) = app.document.get_mut(id) {
                                        e.line_weight = val.clone();
                                    }
                                }
                                ui.close();
                            }
                        }
                    });
                    let st = app.document.get(sel[0]).map(|e| e.line_type.clone());
                    let st_lbl = st.as_ref().map(lt_label).unwrap_or_else(|| "—".into());
                    appearance_row(ui, "Line type", st_lbl, None, true, |ui| {
                        for (lbl, val) in lt_options() {
                            if ui
                                .selectable_label(st.as_ref() == Some(&val), lbl)
                                .clicked()
                            {
                                app.history.snapshot(&app.document);
                                for &id in &sel {
                                    if let Some(e) = app.document.get_mut(id) {
                                        e.line_type = val.clone();
                                    }
                                }
                                ui.close();
                            }
                        }
                    });
                });
            }
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
            ui.add_space(6.0);
        });
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui_state.line_props_open = false;
    }
}

pub(super) fn settings_dialog(ctx: &Context, app: &mut AppState, ui_state: &mut UiState) {
    use oxidraft_document::Units;
    if ctx.data(|d| {
        d.get_temp::<bool>(egui::Id::new("open_settings"))
            .unwrap_or(false)
    }) {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("open_settings"), false));
        ui_state.settings_open = true;
    }
    if !ui_state.settings_open {
        return;
    }
    let backdrop = egui::Area::new(egui::Id::new("settings_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(ctx.content_rect().min)
        .show(ctx, |ui| {
            let r = ctx.content_rect();
            ui.painter()
                .rect_filled(r, 0.0, egui::Color32::from_black_alpha(160));
            ui.allocate_rect(r, egui::Sense::click())
        });
    let mut close = backdrop.inner.clicked();
    let screen_h = ctx.content_rect().height();
    egui::Window::new("settings_dialog")
        .title_bar(false)
        .collapsible(false)
        .resizable([false, true])
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_height((screen_h * 0.7).clamp(360.0, 680.0))
        .min_height(300.0)
        .max_height(screen_h - 24.0)
        .show(
            ctx,
            |ui| {
                ui.set_width(416.0);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Settings")
                        .size(16.0)
                        .strong()
                        .color(crate::theme::TEXT),
                );
                ui.label(
                    egui::RichText::new(
                            "Preferences, drawing aids & document defaults — drag the bottom edge to resize",
                        )
                        .size(11.5)
                        .color(crate::theme::TEXT_DIM),
                );
                ui.add_space(8.0);
                settings_rule(ui);
                let footer_h = 60.0;
                let scroll_h = (ui.available_height() - footer_h).max(120.0);
                egui::ScrollArea::vertical()
                    .max_height(scroll_h)
                    .auto_shrink([false, false])
                    .show(
                        ui,
                        |ui| {
                            ui.set_width(ui.available_width());
                            settings_card(
                                ui,
                                "UNITS",
                                |ui| {
                                    setting_row(
                                        ui,
                                        "Drawing units",
                                        |ui| {
                                            egui::ComboBox::from_id_salt("settings_units")
                                                .selected_text(units_label(app.document.settings.units))
                                                .width(200.0)
                                                .show_ui(
                                                    ui,
                                                    |ui| {
                                                        for units in [
                                                            Units::Millimeters,
                                                            Units::Centimeters,
                                                            Units::Meters,
                                                            Units::Kilometers,
                                                            Units::Inches,
                                                            Units::Feet,
                                                            Units::Unitless,
                                                        ] {
                                                            if ui
                                                                .selectable_label(
                                                                    app.document.settings.units == units,
                                                                    units_label(units),
                                                                )
                                                                .clicked() && app.document.settings.units != units
                                                            {
                                                                app.document.settings.units = units;
                                                                app.sync_zoom_limits();
                                                            }
                                                        }
                                                    },
                                                );
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "DRAWING AIDS",
                                |ui| {
                                    ui.columns(
                                        2,
                                        |c| {
                                            c[0].checkbox(&mut app.snap_on, "Object snap");
                                            c[0].checkbox(&mut app.grid_on, "Grid");
                                            c[0].checkbox(&mut app.grid_snap_on, "Snap to grid");
                                            c[0].checkbox(&mut app.track_on, "Extension tracking");
                                            let mut polar = app.polar_on;
                                            if c[1].checkbox(&mut polar, "Polar tracking").changed() {
                                                app.polar_on = polar;
                                                if polar {
                                                    app.ortho_on = false;
                                                }
                                            }
                                            let mut ortho = app.ortho_on;
                                            if c[1].checkbox(&mut ortho, "Ortho").changed() {
                                                app.ortho_on = ortho;
                                                if ortho {
                                                    app.polar_on = false;
                                                }
                                            }
                                            c[1].checkbox(&mut app.dyn_on, "Dynamic input");
                                            c[1]
                                                .checkbox(&mut app.infer_constraints, "Infer constraints")
                                                .on_hover_text(
                                                    "Record coincident constraints when line endpoints \
                                     are drawn with endpoint snaps or chained, and \
                                     horizontal/vertical on near-axis lines",
                                                );
                                            c[1]
                                                .checkbox(&mut app.show_constraints, "Constraint badges")
                                                .on_hover_text(
                                                    "Show glyphs on the canvas next to constrained \
                                     lines and welded corners",
                                                );
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "POINTER & GUIDES",
                                |ui| {
                                    setting_row(
                                        ui,
                                        "Snap sensitivity",
                                        |ui| {
                                            ui.add(
                                                    egui::Slider::new(&mut app.snap_px, 4.0..=24.0)
                                                        .suffix(" px")
                                                        .fixed_decimals(0),
                                                )
                                                .on_hover_text(
                                                    "How close (in screen pixels) the cursor must be to snap",
                                                );
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Polar angle step",
                                        |ui| {
                                            egui::ComboBox::from_id_salt("settings_polar_step")
                                                .selected_text(format!("{}°", app.polar_step as i32))
                                                .width(120.0)
                                                .show_ui(
                                                    ui,
                                                    |ui| {
                                                        for step in [5.0, 10.0, 15.0, 22.5, 30.0, 45.0, 90.0] {
                                                            if ui
                                                                .selectable_label(
                                                                    (app.polar_step - step).abs() < 1e-6,
                                                                    format!("{step}°"),
                                                                )
                                                                .clicked()
                                                            {
                                                                app.polar_step = step;
                                                            }
                                                        }
                                                    },
                                                );
                                        },
                                    );
                                    ui.checkbox(
                                        &mut app.crosshair,
                                        "Full-screen crosshair cursor",
                                    );
                                    setting_row(
                                        ui,
                                        "Pick-box size",
                                        |ui| {
                                            ui.add(
                                                egui::Slider::new(&mut app.pick_box, 6.0..=24.0)
                                                    .suffix(" px")
                                                    .fixed_decimals(0),
                                            );
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "ZOOM",
                                |ui| {
                                    setting_row(
                                        ui,
                                        "Wheel speed",
                                        |ui| {
                                            ui.add(
                                                egui::Slider::new(&mut app.zoom_speed, 0.25..=3.0)
                                                    .fixed_decimals(2),
                                            );
                                        },
                                    );
                                    ui.checkbox(&mut app.zoom_to_cursor, "Zoom toward cursor");
                                    ui.checkbox(&mut app.invert_zoom, "Invert wheel direction");
                                },
                            );
                            settings_card(
                                ui,
                                "DISPLAY",
                                |ui| {
                                    ui.checkbox(&mut app.show_lineweights, "Show line weights");
                                    ui.add_space(2.0);
                                    setting_row(
                                        ui,
                                        "Line weight scale",
                                        |ui| {
                                            ui.add_enabled(
                                                app.show_lineweights,
                                                egui::Slider::new(&mut app.lineweight_scale, 1.0..=12.0)
                                                    .suffix(" px/mm")
                                                    .fixed_decimals(1),
                                            );
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "GRID",
                                |ui| {
                                    ui.checkbox(&mut app.grid_dots, "Dotted grid (vs. lines)");
                                    ui.add_space(2.0);
                                    setting_row(
                                        ui,
                                        "Major line every",
                                        |ui| {
                                            let mut n = app.grid_major_every;
                                            if ui
                                                .add(egui::DragValue::new(&mut n).speed(0.1).range(2..=20))
                                                .changed()
                                            {
                                                app.grid_major_every = n;
                                            }
                                            ui.label(
                                                egui::RichText::new("lines")
                                                    .size(11.0)
                                                    .color(crate::theme::TEXT_DIM),
                                            );
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Minor colour",
                                        |ui| {
                                            let mut c = [
                                                app.grid_minor_rgb.0,
                                                app.grid_minor_rgb.1,
                                                app.grid_minor_rgb.2,
                                            ];
                                            if ui.color_edit_button_srgb(&mut c).changed() {
                                                app.grid_minor_rgb = (c[0], c[1], c[2]);
                                            }
                                            let mut m = [
                                                app.grid_major_rgb.0,
                                                app.grid_major_rgb.1,
                                                app.grid_major_rgb.2,
                                            ];
                                            ui.label(
                                                egui::RichText::new("Major")
                                                    .size(11.0)
                                                    .color(crate::theme::TEXT_DIM),
                                            );
                                            if ui.color_edit_button_srgb(&mut m).changed() {
                                                app.grid_major_rgb = (m[0], m[1], m[2]);
                                            }
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "OBJECT SNAPS",
                                |ui| {
                                    let kinds = oxidraft_cad::SNAP_KINDS;
                                    ui.columns(
                                        2,
                                        |cols| {
                                            for (i, (kind, label)) in kinds.into_iter().enumerate() {
                                                let ui = &mut cols[i % 2];
                                                let mut on = app.snap.enabled.contains(&kind);
                                                if ui.checkbox(&mut on, label).changed() {
                                                    if on {
                                                        app.snap.enabled.push(kind);
                                                    } else {
                                                        app.snap.enabled.retain(|&k| k != kind);
                                                    }
                                                }
                                            }
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "DIMENSIONS",
                                |ui| {
                                    let ds = &mut app.document.settings.dim_style;
                                    setting_row(
                                        ui,
                                        "Text height",
                                        |ui| {
                                            ui.add(
                                                egui::DragValue::new(&mut ds.text_height)
                                                    .speed(0.1)
                                                    .range(0.1..=1000.0)
                                                    .max_decimals(3),
                                            );
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Arrow size",
                                        |ui| {
                                            ui.add(
                                                egui::DragValue::new(&mut ds.arrow_size)
                                                    .speed(0.1)
                                                    .range(0.1..=1000.0)
                                                    .max_decimals(3),
                                            );
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Precision",
                                        |ui| {
                                            let mut prec = ds.precision as u32;
                                            if ui
                                                .add(
                                                    egui::DragValue::new(&mut prec).speed(0.1).range(0..=8),
                                                )
                                                .changed()
                                            {
                                                ds.precision = prec as usize;
                                            }
                                            ui.label(
                                                egui::RichText::new("decimals")
                                                    .size(11.0)
                                                    .color(crate::theme::TEXT_DIM),
                                            );
                                        },
                                    );
                                    setting_row(
                                        ui,
                                        "Font",
                                        |ui| {
                                            font_combo(ui, "settings_dim_font", &mut ds.font);
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "TEXT",
                                |ui| {
                                    setting_row(
                                        ui,
                                        "Default font",
                                        |ui| {
                                            font_combo(ui, "settings_font", &mut app.text_font);
                                        },
                                    );
                                },
                            );
                            settings_card(
                                ui,
                                "CURVATURE COMB",
                                |ui| {
                                    ui.checkbox(&mut app.comb_on, "Show on selected curves");
                                    ui.add_space(4.0);
                                    ui.add_enabled(
                                        app.comb_on,
                                        egui::Slider::new(&mut app.comb_scale, 1.0..=20.0)
                                            .text("Tooth scale"),
                                    );
                                },
                            );
                            ui.add_space(8.0);
                        },
                    );
                settings_rule(ui);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Reset aids").clicked() {
                        app.apply_prefs(&crate::state::UiPrefs::default());
                    }
                    if ui.button("Line weight & type…").clicked() {
                        ui.ctx()
                            .data_mut(|d| {
                                d.insert_temp(egui::Id::new("open_line_props"), true)
                            });
                    }
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let close_btn = egui::Button::new(
                                    egui::RichText::new("Close").color(egui::Color32::WHITE),
                                )
                                .fill(crate::theme::ACCENT);
                            if ui.add(close_btn).clicked() {
                                close = true;
                            }
                        },
                    );
                });
                ui.add_space(4.0);
            },
        );
    if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui_state.settings_open = false;
    }
}

fn settings_rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
    );
}

fn settings_card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(9.0);
    egui::Frame::new()
        .fill(crate::theme::PANEL_BG)
        .stroke(egui::Stroke::new(1.0, crate::theme::OUTLINE))
        .corner_radius(egui::CornerRadius::same(crate::theme::tok::R_MD))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                egui::RichText::new(title)
                    .size(10.5)
                    .strong()
                    .color(crate::theme::TEXT_DIM),
            );
            ui.add_space(9.0);
            body(ui);
        });
}

fn setting_row(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(132.0, 24.0), egui::Sense::hover());
        ui.painter().text(
            egui::pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.5),
            crate::theme::TEXT,
        );
        add(ui);
    });
}

fn units_label(u: oxidraft_document::Units) -> &'static str {
    use oxidraft_document::Units;
    match u {
        Units::Millimeters => "Millimeters (mm)",
        Units::Centimeters => "Centimeters (cm)",
        Units::Meters => "Meters (m)",
        Units::Kilometers => "Kilometers (km)",
        Units::Inches => "Inches (in)",
        Units::Feet => "Feet (ft)",
        Units::Unitless => "Unitless",
    }
}

pub(super) fn font_combo(ui: &mut egui::Ui, salt: &str, font: &mut Option<String>) -> bool {
    let families = crate::fonts::system_families();
    let default_label = match crate::fonts::default_family_label() {
        Some(name) => format!("Default ({name})"),
        None => "Default".to_string(),
    };
    let label = font.clone().unwrap_or_else(|| default_label.clone());
    let mut changed = false;
    egui::ComboBox::from_id_salt(salt)
        .selected_text(label)
        .width(150.0)
        .show_ui(ui, |ui| {
            if ui
                .selectable_label(font.is_none(), &default_label)
                .clicked()
                && font.is_some()
            {
                *font = None;
                changed = true;
            }
            for fam in &families {
                if ui
                    .selectable_label(font.as_deref() == Some(fam), fam)
                    .clicked()
                    && font.as_deref() != Some(fam)
                {
                    *font = Some(fam.clone());
                    changed = true;
                }
            }
        });
    changed
}

fn tool_hotkey(tool: &Tool) -> &'static str {
    match tool {
        Tool::Select => "Esc",
        Tool::Line { .. } => "L",
        Tool::Polyline { .. } => "P",
        Tool::Circle { .. } => "C",
        Tool::Ellipse { .. } => "E",
        Tool::Arc3 { .. } => "A",
        Tool::Rectangle { .. } => "R",
        Tool::Polygon { .. } => "G",
        Tool::Spline { .. } => "S",
        Tool::Text { .. } => "T",
        Tool::Move { .. } => "Shift+M",
        Tool::Copy { .. } => "Shift+C",
        Tool::Rotate { .. } => "Shift+R",
        Tool::Scale { .. } => "Shift+A",
        Tool::Mirror { .. } => "Shift+I",
        Tool::Offset { .. } => "Shift+O",
        Tool::Trim => "Shift+T",
        Tool::Extend => "Shift+E",
        Tool::Fillet { .. } => "Shift+F",
        Tool::Chamfer { .. } => "Shift+H",
        Tool::Blend { .. } => "Shift+B",
        Tool::Stretch { .. } => "Shift+S",
        Tool::Hatch => "H",
        Tool::ArcStartCenterEnd { .. }
        | Tool::ArcCenterStartEnd { .. }
        | Tool::CircleTwoPoint { .. }
        | Tool::CircleThreePoint { .. }
        | Tool::CircleTtr { .. }
        | Tool::CircleTtt { .. }
        | Tool::TangentLine { .. }
        | Tool::Dimension { .. }
        | Tool::DimAngularLines { .. }
        | Tool::DimRadial { .. }
        | Tool::DimConstraint { .. }
        | Tool::Weld { .. }
        | Tool::ConPick { .. }
        | Tool::PlotWindow { .. }
        | Tool::Point => "",
    }
}

fn tool_menu_item(ui: &mut egui::Ui, app: &mut AppState, label: &str, tool: Tool) {
    let hotkey = tool_hotkey(&tool);
    if ui
        .add(egui::Button::new(label).shortcut_text(hotkey))
        .clicked()
    {
        app.execute(Command::Activate(tool));
        ui.close();
    }
}

#[derive(Clone)]
pub(super) enum Act {
    Tool(Tool),
    Cmd(Command),
}

pub(super) fn run_act(app: &mut AppState, act: &Act) {
    match act {
        Act::Tool(t) => app.execute(Command::Activate(t.clone())),
        Act::Cmd(c) => app.execute(c.clone()),
    }
}

pub(super) fn draw_entries() -> Vec<(crate::icons::Icon, &'static str, Act)> {
    use crate::icons::Icon;
    vec![
        (Icon::Select, "Select  (Esc)", Act::Tool(Tool::Select)),
        (Icon::Point, "Point", Act::Tool(Tool::Point)),
        (
            Icon::Line,
            "Line  (L)",
            Act::Tool(Tool::Line { last: None }),
        ),
        (
            Icon::Polyline,
            "Polyline  (P)",
            Act::Tool(Tool::Polyline { pts: vec![] }),
        ),
        (
            Icon::Circle,
            "Circle  (C)",
            Act::Tool(Tool::Circle { center: None }),
        ),
        (
            Icon::Ellipse,
            "Ellipse  (E) — center, axis end, then minor axis",
            Act::Tool(Tool::Ellipse {
                center: None,
                axis_end: None,
            }),
        ),
        (
            Icon::Arc,
            "Arc — 3 points  (A)",
            Act::Tool(Tool::Arc3 { pts: vec![] }),
        ),
        (
            Icon::Rectangle,
            "Rectangle  (R)",
            Act::Tool(Tool::Rectangle { first: None }),
        ),
        (
            Icon::Polygon,
            "Polygon  (G)",
            Act::Tool(Tool::Polygon {
                center: None,
                radius_point: None,
                sides: None,
            }),
        ),
        (
            Icon::Spline,
            "Spline  (S)",
            Act::Tool(Tool::Spline { pts: vec![] }),
        ),
        (
            Icon::Text,
            "Text  (T)",
            Act::Tool(Tool::Text {
                anchor: None,
                height: 2.5,
            }),
        ),
        (
            Icon::Dimension,
            "Dimension — hold for angular / radius / diameter",
            Act::Tool(Tool::Dimension { p1: None, p2: None }),
        ),
        (
            Icon::Blend,
            "Blend  (Shift+B) — pick G0–G3, pick 2 entities",
            Act::Tool(Tool::Blend {
                continuity: oxidraft_geometry::Continuity::G1,
                tension: 1.0,
                first: None,
                second: None,
            }),
        ),
        (
            Icon::Hatch,
            "Hatch  (H) — fill selected boundaries, or click inside an area",
            Act::Cmd(Command::Hatch),
        ),
    ]
}

pub(super) fn modify_entries() -> Vec<(crate::icons::Icon, &'static str, Act)> {
    use crate::icons::Icon;
    vec![
        (
            Icon::Move,
            "Move selection  (Shift+M)",
            Act::Tool(Tool::Move {
                base: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Copy,
            "Copy selection  (Shift+C)",
            Act::Tool(Tool::Copy {
                base: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Rotate,
            "Rotate selection  (Shift+R)",
            Act::Tool(Tool::Rotate {
                base: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Scale,
            "Scale selection  (Shift+A)",
            Act::Tool(Tool::Scale {
                base: None,
                reference: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Mirror,
            "Mirror selection  (Shift+I)",
            Act::Tool(Tool::Mirror {
                first: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Offset,
            "Offset  (Shift+O) — type a distance, click curve, click side",
            Act::Tool(Tool::Offset {
                dist: 1.0,
                source: None,
            }),
        ),
        (
            Icon::Trim,
            "Trim  (Shift+T) — click the piece to cut",
            Act::Tool(Tool::Trim),
        ),
        (
            Icon::Extend,
            "Extend  (Shift+E) — click the end to lengthen",
            Act::Tool(Tool::Extend),
        ),
        (
            Icon::Fillet,
            "Fillet  (Shift+F) — type radius, pick 2 lines",
            Act::Tool(Tool::Fillet {
                radius: 1.0,
                first: None,
            }),
        ),
        (
            Icon::Chamfer,
            "Chamfer  (Shift+H) — type distance, pick 2 lines",
            Act::Tool(Tool::Chamfer {
                dist: 1.0,
                first: None,
            }),
        ),
        (
            Icon::Stretch,
            "Stretch  (Shift+S) — window, then base→destination",
            Act::Tool(Tool::Stretch {
                c1: None,
                c2: None,
                base: None,
                ids: vec![],
            }),
        ),
        (
            Icon::Explode,
            "Disjoint  (Shift+X) — break a polyline/polygon/rectangle into lines",
            Act::Cmd(Command::Explode),
        ),
        (
            Icon::Join,
            "Join  (Shift+J) — merge selected connected curves",
            Act::Cmd(Command::Join),
        ),
    ]
}

pub(super) fn act_needs_selection(act: &Act) -> bool {
    match act {
        Act::Tool(t) => {
            matches!(
                t,
                Tool::Move { .. }
                    | Tool::Copy { .. }
                    | Tool::Rotate { .. }
                    | Tool::Scale { .. }
                    | Tool::Mirror { .. }
                    | Tool::Stretch { .. }
            )
        }
        Act::Cmd(c) => matches!(c, Command::Explode | Command::Join),
    }
}

pub(super) fn group_id(act: &Act) -> Option<u8> {
    match act {
        Act::Tool(Tool::Line { .. }) => Some(0),
        Act::Tool(Tool::Circle { .. }) => Some(1),
        Act::Tool(Tool::Arc3 { .. }) => Some(2),
        Act::Tool(Tool::Dimension { .. }) => Some(3),
        _ => None,
    }
}

pub(super) fn group_entries(id: u8) -> Vec<(crate::icons::Icon, &'static str, Act)> {
    use crate::icons::Icon;
    match id {
        0 => {
            vec![
                (Icon::Line, "Line", Act::Tool(Tool::Line { last: None })),
                (
                    Icon::Line,
                    "Tangent line",
                    Act::Tool(Tool::TangentLine { first: None }),
                ),
            ]
        }
        1 => {
            vec![
                (
                    Icon::Circle,
                    "Center, radius",
                    Act::Tool(Tool::Circle { center: None }),
                ),
                (
                    Icon::Circle2P,
                    "2 points (diameter)",
                    Act::Tool(Tool::CircleTwoPoint { first: None }),
                ),
                (
                    Icon::Circle3P,
                    "3 points",
                    Act::Tool(Tool::CircleThreePoint { pts: vec![] }),
                ),
                (
                    Icon::CircleTtr,
                    "Tangent, tangent, radius",
                    Act::Tool(Tool::CircleTtr {
                        radius: 1.0,
                        first: None,
                    }),
                ),
                (
                    Icon::CircleTtt,
                    "Tangent, tangent, tangent",
                    Act::Tool(Tool::CircleTtt { picks: vec![] }),
                ),
            ]
        }
        2 => {
            vec![
                (Icon::Arc, "3 points", Act::Tool(Tool::Arc3 { pts: vec![] })),
                (
                    Icon::ArcStartCenterEnd,
                    "Start, center, end",
                    Act::Tool(Tool::ArcStartCenterEnd {
                        start: None,
                        center: None,
                    }),
                ),
                (
                    Icon::ArcCenterStartEnd,
                    "Center, start, end",
                    Act::Tool(Tool::ArcCenterStartEnd {
                        center: None,
                        start: None,
                    }),
                ),
            ]
        }
        _ => {
            vec![
                (
                    Icon::Dimension,
                    "Linear (aligned)",
                    Act::Tool(Tool::Dimension { p1: None, p2: None }),
                ),
                (
                    Icon::DimAngle,
                    "Angular (2 lines)",
                    Act::Tool(Tool::DimAngularLines {
                        a: None,
                        geom: None,
                    }),
                ),
                (
                    Icon::DimRadius,
                    "Radius",
                    Act::Tool(Tool::DimRadial {
                        diameter: false,
                        center: None,
                        radius: 0.0,
                    }),
                ),
                (
                    Icon::DimDiameter,
                    "Diameter",
                    Act::Tool(Tool::DimRadial {
                        diameter: true,
                        center: None,
                        radius: 0.0,
                    }),
                ),
            ]
        }
    }
}

pub(super) fn command_toast(ctx: &Context, app: &AppState, canvas_rect: egui::Rect) {
    const HOLD_SECS: f64 = 2.5;

    const FADE_SECS: f64 = 0.6;
    let len_id = egui::Id::new("command_toast_len");
    let shown_id = egui::Id::new("command_toast_shown_at");
    let len = app.command_log.len();
    let now = ctx.input(|i| i.time);
    let prev_len = ctx.data(|d| d.get_temp::<usize>(len_id).unwrap_or(0));
    let is_new_message = len != prev_len;
    if is_new_message {
        ctx.data_mut(|d| {
            d.insert_temp(len_id, len);
            d.insert_temp(shown_id, now);
        });
    }
    let (Some(shown_at), Some(msg)) = (
        ctx.data(|d| d.get_temp::<f64>(shown_id)),
        app.command_log.last(),
    ) else {
        return;
    };
    let age = now - shown_at;
    if age > HOLD_SECS + FADE_SECS {
        return;
    }
    let alpha = if age < HOLD_SECS {
        1.0
    } else {
        1.0 - (age - HOLD_SECS) / FADE_SECS
    };
    let alpha = alpha.clamp(0.0, 1.0) as f32;
    if alpha <= 0.0 {
        return;
    }
    let pill_rect = ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("status_pill_rect")));
    let (anchor_pos, pivot) = match pill_rect {
        Some(pill) if pill.left() - canvas_rect.left() > 60.0 => (
            egui::pos2((canvas_rect.left() + pill.left()) / 2.0, pill.center().y),
            egui::Align2::CENTER_CENTER,
        ),
        _ => {
            let content_rect = ctx.content_rect();
            let top_off = -(canvas_rect.bottom() - content_rect.bottom()) - 16.0 - 42.0;
            (
                egui::pos2(content_rect.center().x, content_rect.bottom() + top_off),
                egui::Align2::CENTER_BOTTOM,
            )
        }
    };
    egui::Area::new(egui::Id::new("command_toast"))
        .fixed_pos(anchor_pos)
        .pivot(pivot)
        .order(egui::Order::Foreground)
        .default_size(egui::vec2(480.0, 32.0))
        .sizing_pass(is_new_message)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            crate::theme::toast_alert(crate::theme::tok::R_MD)
                .inner_margin(egui::Margin::symmetric(12, 7))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(msg)
                            .size(12.0)
                            .color(crate::theme::TEXT),
                    );
                });
        });
    ctx.request_repaint();
}

pub(super) fn status_pill(ctx: &Context, app: &mut AppState, canvas_rect: egui::Rect) {
    let area = egui::Area::new(egui::Id::new("status_pill"))
        .anchor(
            egui::Align2::CENTER_BOTTOM,
            egui::vec2(
                0.0,
                -(canvas_rect.bottom() - ctx.content_rect().bottom()) - 16.0,
            ),
        )
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            crate::theme::glass(crate::theme::tok::R_MD)
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.scope(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            let (cx, cy) = app.cursor_world;
                            let cell = |ui: &mut egui::Ui, text: String| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(56.0, 18.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().text(
                                    egui::pos2(rect.left(), rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    text,
                                    egui::FontId::monospace(12.5),
                                    crate::theme::ACCENT_BRIGHT,
                                );
                            };
                            ui.label(
                                egui::RichText::new("X")
                                    .size(11.0)
                                    .color(crate::theme::TEXT_DIM),
                            );
                            ui.add_space(8.0);
                            cell(ui, format!("{cx:.2}"));
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Y")
                                    .size(11.0)
                                    .color(crate::theme::TEXT_DIM),
                            );
                            ui.add_space(8.0);
                            cell(ui, format!("{cy:.2}"));
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(app.units_label())
                                    .size(11.0)
                                    .color(crate::theme::TEXT_DIM),
                            );
                        });
                        pill_sep(ui);
                        snap_master(ui, app);
                        ui.add_space(6.0);
                        snap_chip(ui, &mut app.grid_on, "Grid");
                        snap_chip(ui, &mut app.grid_snap_on, "GSnap");
                        let mut polar = app.polar_on;
                        if snap_chip(ui, &mut polar, "Guides") {
                            app.polar_on = polar;
                            if app.polar_on {
                                app.ortho_on = false;
                            }
                        }
                        snap_chip(ui, &mut app.track_on, "Track");
                        snap_chip(ui, &mut app.dyn_on, "Dyn");
                        pill_sep(ui);
                        let (wx, wy) = app
                            .view
                            .screen_to_world(app.view.width / 2.0, app.view.height / 2.0);
                        if round_btn(ui, "−", "Zoom out") {
                            app.view.zoom_at(wx, wy, 0.8);
                        }
                        ui.label(
                            egui::RichText::new(format!("{:>3.0}%", app.view.zoom_percent()))
                                .monospace()
                                .color(crate::theme::ACCENT_BRIGHT),
                        );
                        if round_btn(ui, "+", "Zoom in") {
                            app.view.zoom_at(wx, wy, 1.25);
                        }
                        ui.add_space(2.0);
                        if crate::icons::icon_button_sized(
                            ui,
                            crate::icons::Icon::ZoomFit,
                            "Zoom extents — fit the whole drawing",
                            false,
                            40.0,
                        )
                        .clicked()
                        {
                            app.zoom_extents();
                        }
                        pill_sep(ui);
                        unit_dropdown(ui, app);
                    });
                });
        });
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("status_pill_rect"), area.response.rect));
}

fn unit_dropdown(ui: &mut egui::Ui, app: &mut AppState) {
    use oxidraft_document::Units;
    let open_id = egui::Id::new("unit_menu_open");
    let mut open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    let label = app.units_label();
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(12.0),
        crate::theme::TEXT,
    );
    let w = galley.size().x + 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 28.0), egui::Sense::click());
    let fill = if open || resp.hovered() {
        crate::theme::WIDGET_HOVER
    } else {
        crate::theme::WIDGET_BG
    };
    let p = ui.painter();
    p.rect(
        rect,
        8.0,
        fill,
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
        egui::StrokeKind::Inside,
    );
    p.text(
        egui::pos2(rect.left() + 9.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        crate::theme::TEXT,
    );
    let cc = egui::pos2(rect.right() - 11.0, rect.center().y);
    let (dx, dy) = (3.2, 2.2);
    let chev = if open {
        [
            egui::pos2(cc.x - dx, cc.y + dy * 0.6),
            egui::pos2(cc.x, cc.y - dy * 0.9),
            egui::pos2(cc.x + dx, cc.y + dy * 0.6),
        ]
    } else {
        [
            egui::pos2(cc.x - dx, cc.y - dy * 0.6),
            egui::pos2(cc.x, cc.y + dy * 0.9),
            egui::pos2(cc.x + dx, cc.y - dy * 0.6),
        ]
    };
    p.add(egui::Shape::line(
        chev.to_vec(),
        egui::Stroke::new(1.3, crate::theme::TEXT_DIM),
    ));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let popup = egui::Area::new(egui::Id::new("unit_menu_popup"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.center_top() - egui::vec2(0.0, 8.0))
            .pivot(egui::Align2::CENTER_BOTTOM)
            .show(ui.ctx(), |ui| {
                crate::theme::glass(crate::theme::tok::R_MD)
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(150.0);
                        for (name, units) in [
                            ("Millimeters (mm)", Units::Millimeters),
                            ("Centimeters (cm)", Units::Centimeters),
                            ("Meters (m)", Units::Meters),
                            ("Kilometers (km)", Units::Kilometers),
                            ("Inches (in)", Units::Inches),
                            ("Feet (ft)", Units::Feet),
                            ("Unitless", Units::Unitless),
                        ] {
                            let selected = app.document.settings.units == units;
                            if ui.selectable_label(selected, name).clicked() {
                                app.document.settings.units = units;
                                app.sync_zoom_limits();
                                open = false;
                            }
                        }
                    });
            });
        if popup.response.clicked_elsewhere() && !resp.hovered() {
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
}

fn round_btn(ui: &mut egui::Ui, glyph: &str, tip: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    if resp.hovered() {
        ui.painter()
            .rect_filled(rect, 8.0, crate::theme::WIDGET_HOVER);
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(17.0),
        crate::theme::TEXT,
    );
    resp.on_hover_text(tip).clicked()
}

fn snap_master(ui: &mut egui::Ui, app: &mut AppState) {
    let open_id = egui::Id::new("snap_kinds_open");
    let mut open = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    let h = 26.0;
    let saved_sp = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;
    let (srect, sresp) = ui.allocate_exact_size(egui::vec2(48.0, h), egui::Sense::click());
    let (arect, aresp) = ui.allocate_exact_size(egui::vec2(24.0, h), egui::Sense::click());
    ui.spacing_mut().item_spacing.x = saved_sp;
    let on = app.snap_on;
    let union = srect.union(arect);
    let p = ui.painter();
    p.rect(
        union,
        8.0,
        crate::theme::WIDGET_BG,
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
        egui::StrokeKind::Inside,
    );
    if on {
        p.rect_filled(srect.shrink(1.0), 7.0, crate::theme::ACCENT_DIM);
    } else if sresp.hovered() {
        p.rect_filled(srect.shrink(1.0), 7.0, crate::theme::WIDGET_HOVER);
    }
    p.text(
        srect.center(),
        egui::Align2::CENTER_CENTER,
        "SNAP",
        egui::FontId::proportional(11.0),
        if on {
            crate::theme::ACCENT_BRIGHT
        } else {
            crate::theme::TEXT_DIM
        },
    );
    p.vline(
        srect.right(),
        (union.top() + 5.0)..=(union.bottom() - 5.0),
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
    );
    if open || aresp.hovered() {
        p.rect_filled(arect.shrink(1.0), 7.0, crate::theme::WIDGET_HOVER);
    }
    let tc = arect.center();
    let (tdx, tdy) = (3.5, 2.6);
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(tc.x - tdx, tc.y + tdy * 0.6),
            egui::pos2(tc.x + tdx, tc.y + tdy * 0.6),
            egui::pos2(tc.x, tc.y - tdy),
        ],
        if open {
            crate::theme::ACCENT_BRIGHT
        } else {
            crate::theme::TEXT_DIM
        },
        egui::Stroke::NONE,
    ));
    if sresp.clicked() {
        app.snap_on = !app.snap_on;
    }
    if aresp.clicked() {
        open = !open;
    }
    let trigger_hovered = sresp.hovered() || aresp.hovered();
    if open {
        let kinds = oxidraft_cad::SNAP_KINDS;
        let popup = egui::Area::new(egui::Id::new("snap_kinds_popup_area"))
            .order(egui::Order::Foreground)
            .fixed_pos(union.left_top() - egui::vec2(0.0, 8.0))
            .pivot(egui::Align2::LEFT_BOTTOM)
            .show(ui.ctx(), |ui| {
                crate::theme::glass(crate::theme::tok::R_MD)
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_min_width(168.0);
                        ui.label(
                            egui::RichText::new("OBJECT SNAP")
                                .size(10.0)
                                .color(crate::theme::TEXT_DIM)
                                .strong(),
                        );
                        ui.add_space(4.0);
                        for (kind, label) in kinds {
                            let mut enabled = app.snap.enabled.contains(&kind);
                            if ui.checkbox(&mut enabled, label).changed() {
                                if enabled {
                                    if !app.snap.enabled.contains(&kind) {
                                        app.snap.enabled.push(kind);
                                    }
                                } else {
                                    app.snap.enabled.retain(|&k| k != kind);
                                }
                            }
                        }
                    });
            });
        if popup.response.clicked_elsewhere() && !trigger_hovered {
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
}

fn pill_sep(ui: &mut egui::Ui) {
    ui.add_space(3.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
    );
    ui.add_space(3.0);
}

fn snap_chip(ui: &mut egui::Ui, on: &mut bool, label: &str) -> bool {
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(11.5),
        crate::theme::TEXT,
    );
    let w = galley.size().x + 18.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 26.0), egui::Sense::click());
    let (fill, stroke, fg) = if *on {
        (
            crate::theme::ACCENT_DIM,
            egui::Stroke::NONE,
            crate::theme::ACCENT_BRIGHT,
        )
    } else if resp.hovered() {
        (
            crate::theme::WIDGET_HOVER,
            egui::Stroke::new(1.0, crate::theme::OUTLINE),
            crate::theme::TEXT,
        )
    } else {
        (
            crate::theme::WIDGET_BG,
            egui::Stroke::new(1.0, crate::theme::OUTLINE),
            crate::theme::TEXT_DIM,
        )
    };
    let p = ui.painter();
    p.rect(rect, 9.0, fill, stroke, egui::StrokeKind::Inside);
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(11.5),
        fg,
    );
    if resp.clicked() {
        *on = !*on;
        true
    } else {
        false
    }
}

pub(super) fn inspector(ctx: &Context, app: &mut AppState, canvas_rect: egui::Rect) {
    const RIGHT_M: f32 = 12.0;

    const WIDTH: f32 = 292.0;
    let screen = ctx.content_rect();
    let top_off = (canvas_rect.top() - screen.top()) + 76.0;
    let avail_h = (canvas_rect.height() - 76.0 - 80.0).max(160.0);
    egui::Area::new(egui::Id::new("inspector"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-RIGHT_M, top_off))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.set_width(WIDTH);
            crate::theme::glass(crate::theme::tok::R_LG)
                .inner_margin(egui::Margin::same(0))
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.set_height(avail_h);
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: 20,
                            right: 14,
                            top: 12,
                            bottom: 12,
                        })
                        .show(ui, |ui| inspector_header(ui, app));
                    divider_h(ui);
                    let remaining = ui.available_height();
                    egui::ScrollArea::vertical()
                        .max_height(remaining)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin {
                                    left: 20,
                                    right: 14,
                                    top: 10,
                                    bottom: 10,
                                })
                                .show(ui, |ui| {
                                    ui.set_width(WIDTH - 34.0);
                                    selection_properties(ui, app);
                                    ui.add_space(12.0);
                                    layers_section(ui, app);
                                });
                        });
                });
        });
}

pub(super) fn constraint_bar(ctx: &Context, app: &mut AppState, canvas_rect: egui::Rect) {
    let has_sel = !app.selection.is_empty();

    const RIGHT_M: f32 = 12.0;

    const INSPECTOR_W: f32 = 292.0;

    const GAP: f32 = 8.0;

    const BAR_W: f32 = 40.0;
    let screen = ctx.content_rect();
    let top_off = (canvas_rect.top() - screen.top()) + 76.0;
    egui::Area::new(egui::Id::new("constraint_bar"))
        .anchor(
            egui::Align2::RIGHT_TOP,
            egui::vec2(-(RIGHT_M + INSPECTOR_W + GAP), top_off),
        )
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            crate::theme::glass(crate::theme::tok::R_LG)
                .inner_margin(egui::Margin::symmetric(5, 7))
                .show(ui, |ui| {
                    use crate::icons::Icon;
                    use oxidraft_cad::ConstraintKind as K;
                    ui.set_width(30.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    if con_glyph_button(
                        ui,
                        "Smart Dimension (DIMCON) — click a line for length, \
                         a circle/arc for radius, or two lines for angle",
                        Icon::ConLengthLock,
                    )
                    .clicked()
                    {
                        app.execute(Command::Activate(Tool::DimConstraint {
                            first: None,
                            pending: None,
                        }));
                    }
                    bar_divider(ui);
                    ui.set_width(30.0);
                    ui.spacing_mut().item_spacing.y = 3.0;
                    let mut cmd: Option<Command> = None;
                    // Selection-based relations: enabled only when the current
                    // selection is a valid target, with the requirement shown
                    // as a disabled-hover tooltip. Pick-based relations stay
                    // enabled (they open a pick tool regardless of selection).
                    let mut geo = |ui: &mut egui::Ui, icon: Icon, tip: &str, k: K| {
                        let validity =
                            oxidraft_cad::selection_validity(&app.document, &app.selection, k);
                        let pick_based =
                            !crate::tools::con_pick_plan(k).is_empty() || k == K::Coincident;
                        let enabled = pick_based || validity.is_ok();
                        let resp = ui
                            .add_enabled_ui(enabled, |ui| con_glyph_button(ui, tip, icon))
                            .inner;
                        let resp = match validity {
                            Err(why) if !enabled => resp.on_disabled_hover_text(why),
                            _ => resp,
                        };
                        if resp.clicked() {
                            cmd = Some(Command::Constrain(k));
                        }
                    };
                    geo(
                        ui,
                        Icon::ConHorizontal,
                        "Horizontal (HOR) — level the selected line(s)",
                        K::Horizontal,
                    );
                    geo(
                        ui,
                        Icon::ConVertical,
                        "Vertical (VER) — plumb the selected line(s)",
                        K::Vertical,
                    );
                    geo(
                        ui,
                        Icon::ConParallel,
                        "Parallel (PAR) — align the 2nd line to the 1st",
                        K::Parallel,
                    );
                    geo(
                        ui,
                        Icon::ConPerpendicular,
                        "Perpendicular (PERP) — square the 2nd line to the 1st",
                        K::Perpendicular,
                    );
                    geo(
                        ui,
                        Icon::ConParallel,
                        "Collinear (COLL) — lay the 2nd line on the 1st's carrier",
                        K::Collinear,
                    );
                    geo(
                        ui,
                        Icon::ConEqual,
                        "Equal length (EQL) — match the 2nd line to the 1st",
                        K::EqualLength,
                    );
                    geo(
                        ui,
                        Icon::ConTangent,
                        "Tangent (TANCON) — a line and an arc, or two arcs",
                        K::Tangent,
                    );
                    geo(
                        ui,
                        Icon::ConConcentric,
                        "Concentric (CONC) — share the two circles'/arcs' center",
                        K::Concentric,
                    );
                    geo(
                        ui,
                        Icon::ConEqual,
                        "Equal radius (EQR) — match the 2nd circle/arc to the 1st",
                        K::EqualRadius,
                    );
                    bar_divider(ui);
                    // Pick-based relations.
                    geo(
                        ui,
                        Icon::ConCoincident,
                        "Coincident (COI/WELD) — with two lines selected, weld their \
                         nearest endpoints; otherwise pick any two points to weld",
                        K::Coincident,
                    );
                    geo(
                        ui,
                        Icon::ConCoincident,
                        "Midpoint (MID) — hold a picked point at a line's midpoint",
                        K::Midpoint,
                    );
                    geo(
                        ui,
                        Icon::ConCoincident,
                        "Point on line (POL) — hold a picked point on a line",
                        K::PointOnLine,
                    );
                    geo(
                        ui,
                        Icon::ConCoincident,
                        "Point on circle (POC) — hold a picked point on a circle/arc",
                        K::PointOnCircle,
                    );
                    geo(
                        ui,
                        Icon::ConEqual,
                        "Symmetric (SYM) — mirror two picked points about a line",
                        K::Symmetric,
                    );
                    bar_divider(ui);
                    {
                        let block_ok = oxidraft_cad::selection_validity(
                            &app.document,
                            &app.selection,
                            K::Block,
                        );
                        let resp = ui
                            .add_enabled_ui(block_ok.is_ok(), |ui| {
                                con_glyph_button(
                                    ui,
                                    "Block (BLOCK) — lock the selection into a rigid group",
                                    Icon::ConFix,
                                )
                            })
                            .inner;
                        let resp = match block_ok {
                            Err(why) => resp.on_disabled_hover_text(why),
                            _ => resp,
                        };
                        if resp.clicked() {
                            cmd = Some(Command::Constrain(K::Block));
                        }
                    }
                    ui.add_enabled_ui(has_sel, |ui| {
                        if con_glyph_button(
                            ui,
                            "Fix (GCFIX) — pin the selected geometry in place",
                            Icon::ConFix,
                        )
                        .clicked()
                        {
                            cmd = Some(Command::Fix);
                        }
                        if con_glyph_button(
                            ui,
                            "Remove (UNCON) — drop every constraint on the selection",
                            Icon::ConRemove,
                        )
                        .clicked()
                        {
                            cmd = Some(Command::Unconstrain);
                        }
                    });
                    if let Some(c) = cmd {
                        app.execute(c);
                    }
                });
        });
    egui::Area::new(egui::Id::new("constraint_bar_toggles"))
        .anchor(
            egui::Align2::RIGHT_TOP,
            egui::vec2(-(RIGHT_M + INSPECTOR_W + GAP + BAR_W + GAP), top_off),
        )
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.set_width(28.0);
                ui.spacing_mut().item_spacing.y = 4.0;
                if bar_toggle_onoff(
                    ui,
                    app.infer_constraints,
                    "Auto-constrain — infer coincident, horizontal / vertical \
                     and tangent constraints as you draw",
                    crate::icons::Icon::ConstAuto,
                    crate::icons::Icon::ConstAutoOff,
                ) {
                    app.infer_constraints = !app.infer_constraints;
                }
                if bar_toggle_onoff(
                    ui,
                    app.show_constraints,
                    "Show / hide constraint badges",
                    crate::icons::Icon::ConstShowHide,
                    crate::icons::Icon::ConstShowHideOff,
                ) {
                    app.show_constraints = !app.show_constraints;
                }
            });
        });

    const COMB_GAP: f32 = 28.0;
    egui::Area::new(egui::Id::new("curvature_comb_toggle"))
        .anchor(
            egui::Align2::RIGHT_TOP,
            egui::vec2(
                -(RIGHT_M + INSPECTOR_W + GAP + BAR_W + GAP + 28.0 + COMB_GAP),
                top_off,
            ),
        )
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if bar_toggle(
                ui,
                app.comb_on,
                "Curvature Comb — show curvature teeth on selected curves",
                crate::icons::Icon::CurvComb,
            ) {
                app.comb_on = !app.comb_on;
            }
        });
}

fn bar_toggle(ui: &mut egui::Ui, on: bool, tooltip: &str, icon: crate::icons::Icon) -> bool {
    let (rect, mut resp) = ui.allocate_exact_size(egui::Vec2::splat(28.0), egui::Sense::click());
    let hovered = resp.hovered();
    let anim = ui.ctx().animate_bool(resp.id, hovered);
    let painter = ui.painter_at(rect);
    if anim > 0.001 {
        painter.rect_filled(
            rect,
            7.0,
            crate::theme::WIDGET_HOVER.gamma_multiply(anim * 0.8),
        );
    }
    let alpha = if on {
        255
    } else if hovered {
        200
    } else {
        140
    };
    let tint = egui::Color32::from_white_alpha(alpha);
    crate::icons::paint_icon(&painter, ui.ctx(), icon, rect.shrink(5.0), tint);
    if hovered {
        resp = resp.on_hover_ui(|ui| crate::icons::rich_tooltip(ui, tooltip));
    }
    resp.clicked()
}

fn bar_toggle_onoff(
    ui: &mut egui::Ui,
    on: bool,
    tooltip: &str,
    on_icon: crate::icons::Icon,
    off_icon: crate::icons::Icon,
) -> bool {
    let (rect, mut resp) = ui.allocate_exact_size(egui::Vec2::splat(28.0), egui::Sense::click());
    let hovered = resp.hovered();
    let anim = ui.ctx().animate_bool(resp.id, hovered);
    let painter = ui.painter_at(rect);
    if anim > 0.001 {
        painter.rect_filled(
            rect,
            7.0,
            crate::theme::WIDGET_HOVER.gamma_multiply(anim * 0.8),
        );
    }
    let icon = if on { on_icon } else { off_icon };
    let tint = egui::Color32::from_white_alpha(if on || hovered { 255 } else { 210 });
    crate::icons::paint_icon(&painter, ui.ctx(), icon, rect.shrink(5.0), tint);
    if hovered {
        resp = resp.on_hover_ui(|ui| crate::icons::rich_tooltip(ui, tooltip));
    }
    resp.clicked()
}

fn con_glyph_button(ui: &mut egui::Ui, tooltip: &str, icon: crate::icons::Icon) -> egui::Response {
    let (rect, mut resp) = ui.allocate_exact_size(egui::Vec2::splat(30.0), egui::Sense::click());
    let enabled = ui.is_enabled();
    let hovered = resp.hovered() && enabled;
    let anim = ui.ctx().animate_bool(resp.id, hovered);
    let painter = ui.painter_at(rect);
    if anim > 0.001 {
        painter.rect_filled(
            rect,
            8.0,
            crate::theme::WIDGET_HOVER.gamma_multiply(anim * 0.9),
        );
    }
    let area = egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(20.0));
    // Dim the glyph when the button is disabled, matching the app's other
    // enabled/disabled icon buttons.
    let tint = if enabled {
        egui::Color32::WHITE
    } else {
        egui::Color32::WHITE.gamma_multiply(0.35)
    };
    crate::icons::paint_icon(&painter, ui.ctx(), icon, area, tint);
    if hovered {
        resp = resp.on_hover_ui(|ui| crate::icons::rich_tooltip(ui, tooltip));
    }
    resp
}

fn bar_divider(ui: &mut egui::Ui) {
    ui.add_space(2.0);
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
    );
    ui.add_space(2.0);
}

fn inspector_header(ui: &mut egui::Ui, app: &AppState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("PROPERTIES")
                .size(11.0)
                .color(crate::theme::TEXT_DIM)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let n = app.selection.len();
            let txt = if n == 1 {
                "1 selected".to_string()
            } else {
                format!("{n} selected")
            };
            let galley = ui.painter().layout_no_wrap(
                txt.clone(),
                egui::FontId::monospace(11.0),
                crate::theme::ACCENT_BRIGHT,
            );
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(galley.size().x + 14.0, 20.0),
                egui::Sense::hover(),
            );
            ui.painter().rect(
                rect,
                6.0,
                crate::theme::ACCENT_DIM,
                egui::Stroke::new(1.0, crate::theme::ACCENT),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                txt,
                egui::FontId::monospace(11.0),
                crate::theme::ACCENT_BRIGHT,
            );
        });
    });
}

pub(super) fn divider_h(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().hline(
        rect.x_range(),
        rect.center().y,
        egui::Stroke::new(1.0, crate::theme::OUTLINE),
    );
}

fn layers_section(ui: &mut egui::Ui, app: &mut AppState) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new("LAYERS")
                .size(10.0)
                .color(crate::theme::TEXT_DIM)
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::icons::icon_button_sized(
                ui,
                crate::icons::Icon::AddLayer,
                "New Layer",
                false,
                38.0,
            )
            .clicked()
            {
                let n = app.document.layers.layers.len();
                app.document.layers.add(Layer::new(format!("Layer{}", n)));
            }
        });
    });
    ui.add_space(4.0);
    let current = app.document.layers.current;
    let n_layers = app.document.layers.layers.len();
    let mut counts = vec![0usize; n_layers];
    for e in app.document.iter() {
        if e.layer < n_layers {
            counts[e.layer] += 1;
        }
    }
    let rows: Vec<(usize, String, [u8; 3], bool, usize)> = app
        .document
        .layers
        .layers
        .iter()
        .enumerate()
        .map(|(i, l)| {
            (
                i,
                l.name.clone(),
                [l.color.0, l.color.1, l.color.2],
                l.on,
                counts[i],
            )
        })
        .collect();
    let mut delete_layer: Option<usize> = None;
    for (i, name, rgb, on, count) in rows {
        let is_cur = i == current;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 7.0;
            ui.set_height(38.0);
            let (dr, dresp) = ui.allocate_exact_size(egui::vec2(5.0, 18.0), egui::Sense::click());
            let bar = egui::Rect::from_center_size(dr.center(), egui::vec2(3.0, 16.0));
            let col = if is_cur {
                crate::theme::ACCENT
            } else if dresp.hovered() {
                crate::theme::TEXT_DIM
            } else {
                crate::theme::OUTLINE
            };
            ui.painter().rect_filled(bar, 2.0, col);
            if dresp
                .on_hover_text("Set as the current drawing layer")
                .clicked()
            {
                app.document.layers.current = i;
            }
            let mut c = rgb;
            let changed = ui
                .scope(|ui| {
                    ui.spacing_mut().interact_size = egui::vec2(14.0, 14.0);
                    ui.color_edit_button_srgb(&mut c).changed()
                })
                .inner;
            if changed && let Some(l) = app.document.layers.get_mut(i) {
                l.color = (c[0], c[1], c[2]);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                use crate::icons::{Icon, icon_button_sized};
                let deletable = i != 0 && i != current;
                ui.add_enabled_ui(deletable, |ui| {
                    let tip = if deletable {
                        "Delete this layer (its objects move to layer 0)"
                    } else {
                        "Layer 0 and the current layer can't be deleted"
                    };
                    if icon_button_sized(ui, Icon::Delete, tip, false, 20.0).clicked() {
                        delete_layer = Some(i);
                    }
                });
                let icon = if on { Icon::Eye } else { Icon::EyeOff };
                if icon_button_sized(ui, icon, "Show / hide this layer", false, 20.0).clicked()
                    && let Some(l) = app.document.layers.get_mut(i)
                {
                    l.on = !on;
                }
                ui.add_space(6.0);
                layer_appearance_menus(ui, app, i);
                ui.add_space(10.0);
                ui.label(
                    egui::RichText::new(format!("{count:>2}"))
                        .monospace()
                        .size(11.0)
                        .color(crate::theme::TEXT_DIM),
                );
                ui.add_space(4.0);
                let mut buf = name.clone();
                let name_col = if is_cur {
                    crate::theme::TEXT
                } else {
                    crate::theme::TEXT_DIM
                };
                let resp = ui.add_sized(
                    [ui.available_width(), 22.0],
                    egui::TextEdit::singleline(&mut buf)
                        .frame(egui::Frame::NONE)
                        .text_color(name_col)
                        .font(egui::TextStyle::Monospace),
                );
                if resp.changed()
                    && let Some(l) = app.document.layers.get_mut(i)
                {
                    l.name = buf;
                }
            });
        });
    }
    if let Some(idx) = delete_layer
        && idx != 0
        && idx != app.document.layers.current
    {
        let lname = app.document.layers.layers[idx].name.clone();
        app.history.snapshot(&app.document);
        let ids: Vec<_> = app.document.iter().map(|e| e.id).collect();
        for id in ids {
            if let Some(e) = app.document.get_mut(id) {
                if e.layer == idx {
                    e.layer = 0;
                } else if e.layer > idx {
                    e.layer -= 1;
                }
            }
        }
        let _ = app.document.layers.delete(&lname);
    }
}

fn layer_appearance_menus(ui: &mut egui::Ui, app: &mut AppState, i: usize) {
    use oxidraft_document::LineTypeRef;
    let cur_lt = app
        .document
        .layers
        .get(i)
        .map(|l| l.line_type.clone())
        .unwrap_or(LineTypeRef::Named("Continuous".into()));
    let lt_glyph = match &cur_lt {
        LineTypeRef::Named(n) if n == "Dashed" => "╌╌",
        LineTypeRef::Named(n) if n == "Dotted" => "··",
        LineTypeRef::Named(n) if n == "Center" => "─·",
        _ => "──",
    };
    ui.menu_button(egui::RichText::new(lt_glyph).monospace().size(12.0), |ui| {
        for (lbl, name) in [
            ("Solid", "Continuous"),
            ("Dashed", "Dashed"),
            ("Dotted", "Dotted"),
            ("Center", "Center"),
        ] {
            let val = LineTypeRef::Named(name.into());
            if ui.selectable_label(cur_lt == val, lbl).clicked() {
                app.history.snapshot(&app.document);
                if let Some(l) = app.document.layers.get_mut(i) {
                    l.line_type = val;
                }
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("Layer line type");
    let cur_w = app
        .document
        .layers
        .get(i)
        .map(|l| l.line_weight_mm)
        .unwrap_or(0.0);
    let w_lbl = if cur_w <= 0.0 {
        "—".to_string()
    } else {
        format!("{cur_w:.2}")
    };
    ui.menu_button(egui::RichText::new(w_lbl).monospace().size(11.0), |ui| {
        for mm in [0.0, 0.13, 0.25, 0.35, 0.50, 0.70, 1.00] {
            let lbl = if mm <= 0.0 {
                "Default (hairline)".to_string()
            } else {
                format!("{mm:.2} mm")
            };
            if ui
                .selectable_label((cur_w - mm).abs() < 1e-9, lbl)
                .clicked()
            {
                app.history.snapshot(&app.document);
                if let Some(l) = app.document.layers.get_mut(i) {
                    l.line_weight_mm = mm;
                }
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("Layer line weight");
}

pub(super) fn tool_hint_panel(ctx: &Context, app: &AppState, canvas_rect: egui::Rect) {
    let (title, rows) = match &app.hint_tool {
        Some(t) => hints_for_tool(t),
        None => tool_hints(app),
    };
    if rows.is_empty() {
        return;
    }
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("tool_hint_panel"),
    ));
    let title_col = crate::theme::ACCENT_BRIGHT.gamma_multiply(0.5);
    let key_col = crate::theme::TEXT.gamma_multiply(0.5);
    let desc_col = crate::theme::TEXT_DIM.gamma_multiply(0.5);
    let title_font = egui::FontId::proportional(11.5);
    let key_font = egui::FontId::monospace(11.0);
    let desc_font = egui::FontId::proportional(11.5);
    let row_gap = 6.0;
    let line_gap = 5.0;
    let cell_min = 46.0;
    let title_g = painter.layout_no_wrap(title.to_string(), title_font, title_col);
    let row_g: Vec<(std::sync::Arc<egui::Galley>, std::sync::Arc<egui::Galley>)> = rows
        .iter()
        .map(|(keys, desc)| {
            (
                painter.layout_no_wrap(keys.to_string(), key_font.clone(), key_col),
                painter.layout_no_wrap(desc.to_string(), desc_font.clone(), desc_col),
            )
        })
        .collect();
    let mut width = title_g.size().x;
    let mut height = title_g.size().y;
    for (kg, dg) in &row_g {
        let cell_w = kg.size().x.max(cell_min);
        width = width.max(cell_w + row_gap + dg.size().x);
        height += line_gap + kg.size().y.max(16.0).max(dg.size().y);
    }
    let screen = ctx.content_rect();
    let right = screen.right() - (12.0 + 292.0 + 12.0);
    let left = right - width;
    let inspector_bottom =
        canvas_rect.top() + 76.0 + (canvas_rect.height() - 76.0 - 80.0).max(160.0);
    let mut y = inspector_bottom - height;
    painter.galley(egui::pos2(left, y), title_g.clone(), title_col);
    y += title_g.size().y + line_gap;
    for (kg, dg) in row_g {
        let row_h = kg.size().y.max(16.0).max(dg.size().y);
        let cell_w = kg.size().x.max(cell_min);
        painter.galley(egui::pos2(left, y), kg, key_col);
        painter.galley(egui::pos2(left + cell_w + row_gap, y), dg, desc_col);
        y += row_h + line_gap;
    }
}

fn tool_hints(app: &AppState) -> (&'static str, Vec<(&'static str, &'static str)>) {
    if matches!(app.tool, Tool::Select) {
        if app.document.is_empty() || (!app.has_selection() && app.document.len() <= 1) {
            return (
                "Getting started",
                vec![
                    ("L", "draw a line"),
                    ("C", "draw a circle"),
                    ("R", "draw a rectangle"),
                    ("Q", "tool wheel"),
                    ("Ctrl+F", "all commands"),
                ],
            );
        }
        if app.has_selection() {
            return (
                "Selection",
                vec![
                    ("Drag", "grips to reshape"),
                    ("Ctrl+C / V", "copy / paste"),
                    ("Del", "delete"),
                    ("Esc", "deselect"),
                    ("Q", "modify wheel"),
                ],
            );
        }
    }
    hints_for_tool(&app.tool)
}

fn hints_for_tool(tool: &Tool) -> (&'static str, Vec<(&'static str, &'static str)>) {
    use Tool::*;
    match tool {
        Select => (
            "Select",
            vec![
                ("Click", "pick an entity"),
                ("Drag →", "window select"),
                ("Drag ←", "crossing select"),
                ("Shift", "add / remove"),
            ],
        ),
        Line { .. } => (
            "Line",
            vec![
                ("Click", "set points"),
                ("type", "length / x,y"),
                ("Esc", "finish"),
            ],
        ),
        Polyline { .. } => (
            "Polyline",
            vec![
                ("Click", "add a point"),
                ("Enter", "finish"),
                ("C", "close"),
                ("Esc", "cancel"),
            ],
        ),
        Spline { .. } => (
            "Spline",
            vec![
                ("Click", "add control vertex"),
                ("Enter", "finish"),
                ("C", "close"),
                ("Esc", "cancel"),
            ],
        ),
        Rectangle { .. } => (
            "Rectangle",
            vec![
                ("Click", "two corners"),
                ("type", "width / height"),
                ("aim", "sets direction"),
            ],
        ),
        Polygon { .. } => (
            "Polygon",
            vec![
                ("type", "side count"),
                ("Click", "center, then radius"),
                ("aim", "sets direction"),
            ],
        ),
        Circle { .. } | CircleTwoPoint { .. } | CircleThreePoint { .. } => (
            "Circle",
            vec![("Click", "center, then radius"), ("type", "radius")],
        ),
        Arc3 { .. } | ArcStartCenterEnd { .. } | ArcCenterStartEnd { .. } => {
            ("Arc", vec![("Click", "three points"), ("Esc", "cancel")])
        }
        Ellipse { .. } => (
            "Ellipse",
            vec![
                ("Click", "center, axes"),
                ("type", "major / minor"),
                ("Tab", "switch field"),
            ],
        ),
        Text { .. } => (
            "Text",
            vec![
                ("Click", "anchor point"),
                ("Enter", "place"),
                ("Esc", "cancel"),
            ],
        ),
        Move { .. } | Copy { .. } => (
            "Move / Copy",
            vec![
                ("Click", "base, destination"),
                ("type", "distance / @x,y"),
                ("Esc", "cancel"),
            ],
        ),
        Rotate { .. } => (
            "Rotate",
            vec![
                ("Click", "base, then angle"),
                ("type", "angle°"),
                ("Esc", "cancel"),
            ],
        ),
        Scale { .. } => (
            "Scale",
            vec![
                ("Click", "base, then factor"),
                ("type", "factor"),
                ("Esc", "cancel"),
            ],
        ),
        Mirror { .. } => (
            "Mirror",
            vec![("Click", "two axis points"), ("Esc", "cancel")],
        ),
        Offset { .. } => (
            "Offset",
            vec![
                ("type", "distance"),
                ("Click", "curve, then side"),
                ("Esc", "cancel"),
            ],
        ),
        Trim => ("Trim", vec![("Click", "piece to cut"), ("Esc", "finish")]),
        Extend => (
            "Extend",
            vec![("Click", "end to lengthen"), ("Esc", "finish")],
        ),
        Fillet { .. } => (
            "Fillet",
            vec![
                ("type", "radius"),
                ("Click", "two lines"),
                ("Esc", "cancel"),
            ],
        ),
        Chamfer { .. } => (
            "Chamfer",
            vec![
                ("type", "distance"),
                ("Click", "two lines"),
                ("Esc", "cancel"),
            ],
        ),
        Blend { .. } => (
            "Blend",
            vec![
                ("G0–G3", "continuity"),
                ("type", "tension"),
                ("Click", "two entities"),
                ("Esc", "cancel"),
            ],
        ),
        Stretch { .. } => (
            "Stretch",
            vec![
                ("Drag", "crossing window"),
                ("Click", "base, destination"),
                ("Esc", "cancel"),
            ],
        ),
        Dimension { .. } => (
            "Dimension",
            vec![("Click", "two points, offset"), ("Esc", "cancel")],
        ),
        Hatch => (
            "Hatch",
            vec![("Click", "inside an area"), ("Esc", "finish")],
        ),
        _ => ("", vec![]),
    }
}

pub(super) fn contextual_toolbar(ctx: &Context, app: &mut AppState, canvas_rect: egui::Rect) {
    if !matches!(app.tool, Tool::Select) || !app.has_selection() {
        return;
    }
    let Some(bbox) = app
        .selection
        .iter()
        .filter(|&&id| id != app.origin_id)
        .filter_map(|&id| app.document.get(id).and_then(|e| e.bounding_box()))
        .reduce(|a, b| a.union(&b))
    else {
        return;
    };
    let cxw = (bbox.min.x + bbox.max.x) * 0.5;
    let topw = bbox.max.y;
    let (sx, sy) = app.view.world_to_screen(cxw, topw);
    let anchor = canvas_rect.min + egui::vec2(sx as f32, sy as f32) - egui::vec2(0.0, 50.0);
    let anchor = egui::pos2(
        anchor
            .x
            .clamp(canvas_rect.left() + 90.0, canvas_rect.right() - 200.0),
        anchor
            .y
            .clamp(canvas_rect.top() + 70.0, canvas_rect.bottom() - 60.0),
    );
    egui::Area::new(egui::Id::new("contextual_toolbar"))
        .fixed_pos(anchor)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            crate::theme::glass(crate::theme::tok::R_MD)
                .inner_margin(egui::Margin::same(5))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                    ui.horizontal(|ui| {
                        use crate::icons::{Icon, icon_button_sized};
                        if icon_button_sized(ui, Icon::Copy, "Duplicate  (Shift+C)", false, 38.0)
                            .clicked()
                        {
                            app.execute(Command::Activate(Tool::Copy {
                                base: None,
                                ids: vec![],
                            }));
                        }
                        if icon_button_sized(ui, Icon::Mirror, "Mirror  (Shift+I)", false, 38.0)
                            .clicked()
                        {
                            app.execute(Command::Activate(Tool::Mirror {
                                first: None,
                                ids: vec![],
                            }));
                        }
                        if icon_button_sized(ui, Icon::Rotate, "Rotate  (Shift+R)", false, 38.0)
                            .clicked()
                        {
                            app.execute(Command::Activate(Tool::Rotate {
                                base: None,
                                ids: vec![],
                            }));
                        }
                        if icon_button_sized(ui, Icon::Offset, "Offset  (Shift+O)", false, 38.0)
                            .clicked()
                        {
                            app.execute(Command::Activate(Tool::Offset {
                                dist: 1.0,
                                source: None,
                            }));
                        }
                        pill_sep(ui);
                        if icon_button_sized(ui, Icon::Delete, "Delete  (Del)", false, 38.0)
                            .clicked()
                        {
                            app.erase_selection();
                        }
                    });
                });
        });
}

fn prop_section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(15.0);
    ui.add(egui::Label::new(
        egui::RichText::new(title)
            .size(10.0)
            .color(crate::theme::TEXT_DIM)
            .strong(),
    ));
    ui.add_space(7.0);
}

fn prop_caption(ui: &mut egui::Ui, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .size(10.0)
                .color(crate::theme::TEXT_DIM),
        )
        .truncate(),
    );
}

fn style_value_box(ui: &mut egui::Ui) {
    let r = egui::CornerRadius::same(9);
    let v = ui.visuals_mut();
    v.widgets.inactive.bg_fill = crate::theme::WIDGET_BG;
    v.widgets.inactive.weak_bg_fill = crate::theme::WIDGET_BG;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, crate::theme::OUTLINE);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = crate::theme::WIDGET_HOVER;
    v.widgets.hovered.weak_bg_fill = crate::theme::WIDGET_HOVER;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, crate::theme::ACCENT_DIM);
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.bg_fill = crate::theme::WIDGET_HOVER;
    v.widgets.active.weak_bg_fill = crate::theme::WIDGET_HOVER;
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, crate::theme::ACCENT);
    v.widgets.active.corner_radius = r;
}

fn num_field(ui: &mut egui::Ui, caption: &str, v: &mut f64, speed: f64) -> bool {
    ui.scope(|ui| {
        prop_caption(ui, caption);
        ui.add_space(4.0);
        style_value_box(ui);
        ui.add_sized(
            [ui.available_width(), 30.0],
            egui::DragValue::new(v).speed(speed).max_decimals(4),
        )
        .changed()
    })
    .inner
}

fn xy_fields(ui: &mut egui::Ui, ca: &str, a: &mut f64, cb: &str, b: &mut f64, speed: f64) -> bool {
    let mut changed = false;
    ui.columns(2, |c| {
        changed |= num_field(&mut c[0], ca, a, speed);
        changed |= num_field(&mut c[1], cb, b, speed);
    });
    changed
}

fn metric_field(ui: &mut egui::Ui, caption: &str, value: f64) {
    ui.scope(|ui| {
        prop_caption(ui, caption);
        ui.add_space(4.0);
        style_value_box(ui);
        let mut v = value;
        ui.add_enabled_ui(false, |ui| {
            ui.add_sized(
                [ui.available_width(), 30.0],
                egui::DragValue::new(&mut v).max_decimals(4),
            );
        });
    });
}

fn kind_label(kind: &EntityKind) -> &'static str {
    match kind {
        EntityKind::Curve(Curve::Line(_)) => "Line",
        EntityKind::Curve(Curve::Arc(a)) => {
            let span = (a.end_angle - a.start_angle).abs();
            if (span - std::f64::consts::TAU).abs() < 1e-9 {
                "Circle"
            } else {
                "Arc"
            }
        }
        EntityKind::Curve(Curve::Ellipse(_)) => "Ellipse",
        EntityKind::Curve(Curve::Bezier(_)) => "Bézier",
        EntityKind::Curve(Curve::Poly(_)) => "Polyline",
        EntityKind::Curve(Curve::Rational(_)) | EntityKind::Curve(Curve::Nurbs(_)) => "Spline",
        EntityKind::Point(_) => "Point",
        EntityKind::Text { .. } => "Text",
        EntityKind::XLine { .. } => "Construction line",
        EntityKind::Ray { .. } => "Ray",
        EntityKind::Insert { .. } => "Block insert",
        EntityKind::Hatch { .. } => "Hatch",
        EntityKind::Dimension { .. } => "Dimension",
        EntityKind::OrthoDim { vertical: true, .. } => "Vertical dimension",
        EntityKind::OrthoDim { .. } => "Horizontal dimension",
        EntityKind::AngularDim { .. } => "Angular dimension",
        EntityKind::RadialDim { diameter: true, .. } => "Diameter dimension",
        EntityKind::RadialDim { .. } => "Radius dimension",
    }
}

fn kind_icon(kind: &EntityKind) -> crate::icons::Icon {
    use crate::icons::Icon;
    match kind {
        EntityKind::Curve(Curve::Line(_)) | EntityKind::XLine { .. } | EntityKind::Ray { .. } => {
            Icon::Line
        }
        EntityKind::Curve(Curve::Arc(a)) => {
            let span = (a.end_angle - a.start_angle).abs();
            if (span - std::f64::consts::TAU).abs() < 1e-9 {
                Icon::Circle
            } else {
                Icon::Arc
            }
        }
        EntityKind::Curve(Curve::Ellipse(_)) => Icon::Ellipse,
        EntityKind::Curve(Curve::Poly(_)) => Icon::Polyline,
        EntityKind::Curve(Curve::Bezier(_))
        | EntityKind::Curve(Curve::Rational(_))
        | EntityKind::Curve(Curve::Nurbs(_)) => Icon::Spline,
        EntityKind::Text { .. } => Icon::Text,
        EntityKind::Hatch { .. } => Icon::Hatch,
        _ => Icon::Select,
    }
}

fn object_header(ui: &mut egui::Ui, name: &str, subtitle: &str, icon: crate::icons::Icon) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
        ui.painter().rect(
            rect,
            10.0,
            crate::theme::ACCENT_DIM,
            egui::Stroke::new(1.0, crate::theme::ACCENT),
            egui::StrokeKind::Inside,
        );
        crate::icons::paint_icon(
            &ui.painter_at(rect),
            ui.ctx(),
            icon,
            rect.shrink(10.0),
            crate::theme::ACCENT_BRIGHT,
        );
        ui.add_space(4.0);
        ui.vertical(|ui| {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new(name)
                    .size(14.0)
                    .strong()
                    .color(crate::theme::TEXT),
            );
            ui.label(
                egui::RichText::new(subtitle)
                    .size(11.5)
                    .monospace()
                    .color(crate::theme::TEXT_DIM),
            );
        });
    });
}

fn dim_override_editor(ui: &mut egui::Ui, app: &mut AppState, id: oxidraft_document::EntityId) {
    let is_dim = matches!(
        app.document.get(id).map(|e| &e.kind),
        Some(
            EntityKind::Dimension { .. }
                | EntityKind::OrthoDim { .. }
                | EntityKind::AngularDim { .. }
                | EntityKind::RadialDim { .. }
        )
    );
    if !is_dim {
        return;
    }
    prop_section(ui, "TEXT OVERRIDE");
    let mut buf = app.dim_override(id).unwrap_or_default();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut buf)
            .hint_text("measured value")
            .desired_width(f32::INFINITY),
    );
    if resp.gained_focus() {
        app.begin_edit();
    }
    if resp.changed() {
        app.set_dim_override(id, Some(buf));
    }
}

fn measurements(ui: &mut egui::Ui, kind: &EntityKind) {
    use oxidraft_geometry::CurveSegment;
    match kind {
        EntityKind::Dimension { p1, p2, .. } => {
            prop_section(ui, "MEASUREMENTS");
            metric_field(ui, "Length", p1.dist_f64(p2));
            return;
        }
        EntityKind::OrthoDim {
            p1, p2, vertical, ..
        } => {
            prop_section(ui, "MEASUREMENTS");
            let (a, b) = (p1.to_f64(), p2.to_f64());
            let d = if *vertical {
                (b.1 - a.1).abs()
            } else {
                (b.0 - a.0).abs()
            };
            metric_field(ui, if *vertical { "Height" } else { "Width" }, d);
            return;
        }
        EntityKind::AngularDim { center, p1, p2, .. } => {
            prop_section(ui, "MEASUREMENTS");
            let (cx, cy) = center.to_f64();
            let (a1x, a1y) = p1.to_f64();
            let (a2x, a2y) = p2.to_f64();
            let a = oxidraft_geometry::wrap_deg360(
                ((a2y - cy).atan2(a2x - cx) - (a1y - cy).atan2(a1x - cx)).to_degrees(),
            );
            metric_field(ui, "Angle °", a);
            return;
        }
        EntityKind::RadialDim {
            center,
            edge,
            diameter,
            ..
        } => {
            prop_section(ui, "MEASUREMENTS");
            let r = center.dist_f64(edge);
            ui.columns(2, |c| {
                metric_field(&mut c[0], "Radius", r);
                metric_field(&mut c[1], "Diameter", 2.0 * r);
            });
            let _ = diameter;
            return;
        }
        _ => {}
    }
    let EntityKind::Curve(c) = kind else {
        return;
    };
    prop_section(ui, "MEASUREMENTS");
    match c {
        Curve::Line(l) => {
            let (dx, dy) = (l.p1.x - l.p0.x, l.p1.y - l.p0.y);
            let len = (dx * dx + dy * dy).sqrt();
            let ang = dy.atan2(dx).to_degrees();
            ui.columns(2, |c| {
                metric_field(&mut c[0], "Length", len);
                metric_field(&mut c[1], "Angle °", ang);
            });
        }
        Curve::Arc(a) => {
            let span = (a.end_angle - a.start_angle).abs();
            let is_circle = (span - std::f64::consts::TAU).abs() < 1e-9;
            if is_circle {
                ui.columns(2, |c| {
                    metric_field(&mut c[0], "Circumference", std::f64::consts::TAU * a.radius);
                    metric_field(
                        &mut c[1],
                        "Area",
                        std::f64::consts::PI * a.radius * a.radius,
                    );
                });
            } else {
                ui.columns(2, |c| {
                    metric_field(&mut c[0], "Arc length", a.radius * span);
                    metric_field(&mut c[1], "Sweep °", span.to_degrees());
                });
            }
        }
        other => metric_field(ui, "Length", other.arc_length()),
    }
}

fn appearance_row(
    ui: &mut egui::Ui,
    label: &str,
    value: String,
    swatch: Option<egui::Color32>,
    line_sample: bool,
    add_options: impl FnOnce(&mut egui::Ui),
) {
    let id = ui.make_persistent_id(("appearance_row", label));
    let inner = egui::Frame::new()
        .fill(crate::theme::WIDGET_BG)
        .stroke(egui::Stroke::new(1.0, crate::theme::OUTLINE))
        .corner_radius(egui::CornerRadius::same(9))
        .inner_margin(egui::Margin::symmetric(11, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(22.0);
                ui.label(
                    egui::RichText::new(label)
                        .size(12.5)
                        .color(crate::theme::TEXT_DIM),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (cr, _) =
                        ui.allocate_exact_size(egui::vec2(12.0, 22.0), egui::Sense::hover());
                    let cc = cr.center();
                    let (dx, dy) = (3.0, 2.0);
                    ui.painter().add(egui::Shape::line(
                        vec![
                            egui::pos2(cc.x - dx, cc.y - dy * 0.6),
                            egui::pos2(cc.x, cc.y + dy * 0.9),
                            egui::pos2(cc.x + dx, cc.y - dy * 0.6),
                        ],
                        egui::Stroke::new(1.3, crate::theme::TEXT_DIM),
                    ));
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(value)
                            .size(12.5)
                            .color(crate::theme::TEXT),
                    );
                    if let Some(c) = swatch {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        ui.painter().rect_filled(r, 3.0, c);
                    }
                    if line_sample {
                        let (r, _) =
                            ui.allocate_exact_size(egui::vec2(34.0, 12.0), egui::Sense::hover());
                        ui.painter().hline(
                            r.x_range(),
                            r.center().y,
                            egui::Stroke::new(1.6, crate::theme::TEXT),
                        );
                    }
                });
            });
        });
    let rect = inner.response.rect;
    let resp = ui.interact(rect, id, egui::Sense::click());
    if resp.hovered() {
        ui.painter().rect_stroke(
            rect,
            egui::CornerRadius::same(9),
            egui::Stroke::new(1.0, crate::theme::ACCENT_DIM),
            egui::StrokeKind::Inside,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    egui::Popup::menu(&resp).show(add_options);
    ui.add_space(8.0);
}

fn lw_options() -> [(&'static str, oxidraft_document::LineWeight); 7] {
    use oxidraft_document::LineWeight::{ByLayer, Hundredths};
    [
        ("By layer", ByLayer),
        ("0.13 mm", Hundredths(13)),
        ("0.25 mm", Hundredths(25)),
        ("0.35 mm", Hundredths(35)),
        ("0.50 mm", Hundredths(50)),
        ("0.70 mm", Hundredths(70)),
        ("1.00 mm", Hundredths(100)),
    ]
}

fn lw_label(w: &oxidraft_document::LineWeight) -> String {
    use oxidraft_document::LineWeight;
    match w {
        LineWeight::ByBlock => "By block".into(),
        LineWeight::Hundredths(h) => format!("{:.2} mm", *h as f64 / 100.0),
        LineWeight::ByLayer => "By layer".into(),
    }
}

fn lt_options() -> [(&'static str, oxidraft_document::LineTypeRef); 5] {
    use oxidraft_document::LineTypeRef;
    [
        ("By layer", LineTypeRef::ByLayer),
        ("Solid", LineTypeRef::Named("Continuous".into())),
        ("Dashed", LineTypeRef::Named("Dashed".into())),
        ("Dotted", LineTypeRef::Named("Dotted".into())),
        ("Center", LineTypeRef::Named("Center".into())),
    ]
}

fn lt_label(t: &oxidraft_document::LineTypeRef) -> String {
    use oxidraft_document::LineTypeRef;
    match t {
        LineTypeRef::ByBlock => "By block".into(),
        LineTypeRef::Named(n) if n == "Continuous" => "Solid".into(),
        LineTypeRef::Named(n) => n.clone(),
        LineTypeRef::ByLayer => "By layer".into(),
    }
}

fn appearance_section(ui: &mut egui::Ui, app: &mut AppState, sel: &[oxidraft_document::EntityId]) {
    prop_section(ui, "APPEARANCE");
    let first_lw = sel
        .first()
        .and_then(|&id| app.document.get(id))
        .map(|e| e.line_weight.clone());
    let lw_lbl = first_lw
        .as_ref()
        .map(lw_label)
        .unwrap_or_else(|| "By layer".into());
    appearance_row(ui, "Line weight", lw_lbl, None, false, |ui| {
        for (lbl, val) in lw_options() {
            if ui
                .selectable_label(first_lw.as_ref() == Some(&val), lbl)
                .clicked()
            {
                app.history.snapshot(&app.document);
                for &id in sel {
                    if let Some(e) = app.document.get_mut(id) {
                        e.line_weight = val.clone();
                    }
                }
                ui.close();
            }
        }
    });
    let first_lt = sel
        .first()
        .and_then(|&id| app.document.get(id))
        .map(|e| e.line_type.clone());
    let lt_lbl = first_lt
        .as_ref()
        .map(lt_label)
        .unwrap_or_else(|| "By layer".into());
    appearance_row(ui, "Line type", lt_lbl, None, true, |ui| {
        for (lbl, val) in lt_options() {
            if ui
                .selectable_label(first_lt.as_ref() == Some(&val), lbl)
                .clicked()
            {
                app.history.snapshot(&app.document);
                for &id in sel {
                    if let Some(e) = app.document.get_mut(id) {
                        e.line_type = val.clone();
                    }
                }
                ui.close();
            }
        }
    });
    let layer_names: Vec<String> = app
        .document
        .layers
        .layers
        .iter()
        .map(|l| l.name.clone())
        .collect();
    let first_layer = sel
        .first()
        .and_then(|&id| app.document.get(id))
        .map(|e| e.layer)
        .unwrap_or(0);
    let mixed = sel
        .iter()
        .any(|&id| app.document.get(id).map(|e| e.layer) != Some(first_layer));
    let layer_value = if mixed {
        "(mixed)".to_string()
    } else {
        layer_names.get(first_layer).cloned().unwrap_or_default()
    };
    let swatch = app
        .document
        .layers
        .get(first_layer)
        .map(|l| egui::Color32::from_rgb(l.color.0, l.color.1, l.color.2));
    appearance_row(ui, "Layer", layer_value, swatch, false, |ui| {
        for (i, name) in layer_names.iter().enumerate() {
            if ui.selectable_label(i == first_layer, name).clicked() {
                app.history.snapshot(&app.document);
                for &id in sel {
                    if let Some(e) = app.document.get_mut(id) {
                        e.layer = i;
                    }
                }
                ui.close();
            }
        }
    });
}

fn selection_properties(ui: &mut egui::Ui, app: &mut AppState) {
    let sel: Vec<_> = app.selection.clone();
    if sel.is_empty() {
        ui.add(egui::Label::new(
            egui::RichText::new("Nothing selected").color(crate::theme::TEXT_DIM),
        ));
        ui.add(egui::Label::new(
            egui::RichText::new(format!("{} objects in drawing", app.document.len()))
                .size(11.0)
                .color(crate::theme::TEXT_DIM),
        ));
        return;
    }
    if sel.len() == 1 {
        let id = sel[0];
        if let Some(kind) = app.document.get(id).map(|e| e.kind.clone()) {
            let layer_idx = app.document.get(id).map(|e| e.layer).unwrap_or(0);
            let layer_name = app
                .document
                .layers
                .get(layer_idx)
                .map(|l| l.name.clone())
                .unwrap_or_default();
            object_header(
                ui,
                kind_label(&kind),
                &format!("Layer {layer_name}"),
                kind_icon(&kind),
            );
            edit_entity_geometry(ui, app, id);
            if let Some(e) = app.document.get(id) {
                measurements(ui, &e.kind);
            }
            dim_override_editor(ui, app, id);
        }
    } else {
        ui.add(egui::Label::new(
            egui::RichText::new(format!("{} objects selected", sel.len()))
                .size(14.0)
                .strong(),
        ));
    }
    constraints_section(ui, app, &sel);
    appearance_section(ui, app, &sel);
}

fn constraints_section(ui: &mut egui::Ui, app: &mut AppState, sel: &[oxidraft_document::EntityId]) {
    let touching: Vec<(usize, oxidraft_document::SketchConstraint)> = app
        .document
        .constraints
        .iter()
        .enumerate()
        .filter(|(_, c)| sel.iter().any(|&id| c.references(id)))
        .map(|(i, c)| (i, *c))
        .collect();
    if touching.is_empty() {
        return;
    }
    prop_section(ui, "CONSTRAINTS");
    let dof = oxidraft_cad::dof_report(&app.document, sel);
    ui.label(
        egui::RichText::new(if dof.dof == 0 {
            "Fully constrained".to_string()
        } else {
            format!("{} DOF remaining", dof.dof)
        })
        .size(11.0)
        .color(if dof.dof == 0 {
            crate::theme::SNAP
        } else {
            crate::theme::TEXT_DIM
        }),
    );
    ui.add_space(3.0);
    let mut to_remove = None;
    for (idx, c) in &touching {
        let is_redundant = dof.redundant.contains(idx);
        ui.horizontal(|ui| {
            let mut label = c.kind.label().to_string();
            if let Some(first) = label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            if c.b.is_some() {
                label.push_str("  ·  pair");
            }
            if let Some(v) = c.val {
                label.push_str(&format!("  ·  {}", (v * 1e4).round() / 1e4));
            }
            if is_redundant {
                label.push_str("  ·  redundant");
            }
            let mut rt = egui::RichText::new(label).size(12.0);
            if is_redundant {
                rt = rt.color(crate::theme::TEXT_DIM);
            }
            ui.label(rt);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::icons::icon_button_sized(
                    ui,
                    crate::icons::Icon::ConRemove,
                    "Remove this constraint",
                    false,
                    20.0,
                )
                .clicked()
                {
                    to_remove = Some(*c);
                }
            });
        });
    }
    if let Some(rc) = to_remove {
        app.history.snapshot(&app.document);
        app.document.constraints.retain(|c| *c != rc);
    }
    ui.add_space(4.0);
}

fn edit_entity_geometry(ui: &mut egui::Ui, app: &mut AppState, id: oxidraft_document::EntityId) {
    let entity = match app.document.get(id) {
        Some(e) => e.clone(),
        None => return,
    };
    match &entity.kind {
        EntityKind::Curve(Curve::Line(line)) => {
            prop_section(ui, "GEOMETRY");
            let mut p0x = line.p0.x;
            let mut p0y = line.p0.y;
            let mut p1x = line.p1.x;
            let mut p1y = line.p1.y;
            let mut length = (line.p1.x - line.p0.x).hypot(line.p1.y - line.p0.y);
            let mut endpoints_changed = false;
            prop_caption(ui, "Start");
            endpoints_changed |= xy_fields(ui, "X", &mut p0x, "Y", &mut p0y, 0.01);
            ui.add_space(4.0);
            prop_caption(ui, "End");
            endpoints_changed |= xy_fields(ui, "X", &mut p1x, "Y", &mut p1y, 0.01);
            ui.add_space(4.0);
            let length_changed = num_field(ui, "Length", &mut length, 0.01);
            if endpoints_changed || (length_changed && length > 1e-6) {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Curve(Curve::Line(ref mut l)) = e.kind
                {
                    if endpoints_changed {
                        l.p0 = Point2d::from_f64(p0x, p0y);
                        l.p1 = Point2d::from_f64(p1x, p1y);
                    } else {
                        let (mx, my) = ((l.p0.x + l.p1.x) * 0.5, (l.p0.y + l.p1.y) * 0.5);
                        let cur = (l.p1.x - l.p0.x).hypot(l.p1.y - l.p0.y);
                        if cur > 1e-9 {
                            let k = length / cur;
                            l.p0 =
                                Point2d::from_f64(mx + (l.p0.x - mx) * k, my + (l.p0.y - my) * k);
                            l.p1 =
                                Point2d::from_f64(mx + (l.p1.x - mx) * k, my + (l.p1.y - my) * k);
                        }
                    }
                }
                let new_len =
                    app.document
                        .get(id)
                        .and_then(|e| e.as_curve())
                        .and_then(|c| match c {
                            Curve::Line(l) => Some((l.p1.x - l.p0.x).hypot(l.p1.y - l.p0.y)),
                            _ => None,
                        });
                if let Some(new_len) = new_len
                    && app
                        .document
                        .constraints
                        .iter()
                        .any(|c| c.kind == oxidraft_document::ConstraintKind::Distance && c.a == id)
                {
                    app.document
                        .add_constraint(oxidraft_document::SketchConstraint::distance(id, new_len));
                }
                if !oxidraft_cad::resolve_after_edit(&mut app.document, id, None) {
                    app.command_log.push(
                        "Constraints not satisfiable after edit (UNCONSTRAIN to drop)".into(),
                    );
                }
            }
        }
        EntityKind::Curve(Curve::Arc(arc)) => {
            let span = (arc.end_angle - arc.start_angle).abs();
            let is_circle = (span - 2.0 * std::f64::consts::PI).abs() < 1e-9;
            prop_section(ui, "GEOMETRY");
            let mut cx = arc.center.x;
            let mut cy = arc.center.y;
            let mut r = arc.radius;
            let mut sa = arc.start_angle.to_degrees();
            let mut ea = arc.end_angle.to_degrees();
            let mut changed = false;
            prop_caption(ui, "Centre");
            changed |= xy_fields(ui, "X", &mut cx, "Y", &mut cy, 0.01);
            ui.add_space(4.0);
            changed |= num_field(ui, "Radius", &mut r, 0.01);
            if !is_circle {
                ui.add_space(4.0);
                ui.columns(2, |c| {
                    changed |= num_field(&mut c[0], "Start °", &mut sa, 0.5);
                    changed |= num_field(&mut c[1], "End °", &mut ea, 0.5);
                });
            }
            if changed {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Curve(Curve::Arc(ref mut a)) = e.kind
                {
                    a.center = Point2d::from_f64(cx, cy);
                    a.radius = r.max(0.001);
                    if !is_circle {
                        a.start_angle = sa.to_radians();
                        a.end_angle = ea.to_radians();
                    }
                }
                if app
                    .document
                    .constraints
                    .iter()
                    .any(|c| c.kind == oxidraft_document::ConstraintKind::Radius && c.a == id)
                {
                    app.document
                        .add_constraint(oxidraft_document::SketchConstraint::radius(
                            id,
                            r.max(0.001),
                        ));
                }
                if !oxidraft_cad::resolve_after_edit(&mut app.document, id, None) {
                    app.command_log.push(
                        "Constraints not satisfiable after edit (UNCONSTRAIN to drop)".into(),
                    );
                }
            }
        }
        EntityKind::Text {
            anchor,
            content,
            height,
            rotation,
            font,
        } => {
            prop_section(ui, "GEOMETRY");
            let mut ax = anchor.x;
            let mut ay = anchor.y;
            let mut h = *height;
            let mut rot = rotation.to_degrees();
            let mut txt = content.clone();
            let mut chosen_font = font.clone();
            let mut changed = false;
            prop_caption(ui, "Font");
            changed |= font_combo(ui, "prop_font", &mut chosen_font);
            ui.add_space(4.0);
            prop_caption(ui, "Anchor");
            changed |= xy_fields(ui, "X", &mut ax, "Y", &mut ay, 0.01);
            ui.add_space(4.0);
            ui.columns(2, |c| {
                changed |= num_field(&mut c[0], "Height", &mut h, 0.01);
                changed |= num_field(&mut c[1], "Rotation °", &mut rot, 0.5);
            });
            ui.add_space(4.0);
            prop_caption(ui, "Content");
            changed |= ui
                .add_sized(
                    [ui.available_width(), 48.0],
                    egui::TextEdit::multiline(&mut txt),
                )
                .changed();
            if changed {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Text {
                        anchor: ref mut a,
                        content: ref mut c,
                        height: ref mut ht,
                        rotation: ref mut rot_rad,
                        font: ref mut f,
                    } = e.kind
                {
                    *a = Point2d::from_f64(ax, ay);
                    *c = txt;
                    *ht = h.max(0.1);
                    *rot_rad = rot.to_radians();
                    *f = chosen_font;
                }
            }
        }
        EntityKind::Point(pt) => {
            prop_section(ui, "GEOMETRY");
            let mut px = pt.x;
            let mut py = pt.y;
            let mut changed = false;
            prop_caption(ui, "Position");
            changed |= xy_fields(ui, "X", &mut px, "Y", &mut py, 0.01);
            if changed {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Point(ref mut p) = e.kind
                {
                    *p = Point2d::from_f64(px, py);
                }
            }
        }
        EntityKind::Curve(Curve::Ellipse(el)) => {
            let span = (el.end_angle - el.start_angle).abs();
            let is_full = (span - std::f64::consts::TAU).abs() < 1e-9;
            prop_section(ui, "GEOMETRY");
            let mut cx = el.center.x;
            let mut cy = el.center.y;
            let mut major = el.semi_major;
            let mut minor = el.semi_minor;
            let mut rot = el.rotation.to_degrees();
            let mut sa = el.start_angle.to_degrees();
            let mut ea = el.end_angle.to_degrees();
            let mut changed = false;
            prop_caption(ui, "Centre");
            changed |= xy_fields(ui, "X", &mut cx, "Y", &mut cy, 0.01);
            ui.add_space(4.0);
            ui.columns(2, |c| {
                changed |= num_field(&mut c[0], "Semi-major", &mut major, 0.01);
                changed |= num_field(&mut c[1], "Semi-minor", &mut minor, 0.01);
            });
            ui.add_space(4.0);
            changed |= num_field(ui, "Rotation °", &mut rot, 0.5);
            if !is_full {
                ui.add_space(4.0);
                ui.columns(2, |c| {
                    changed |= num_field(&mut c[0], "Start °", &mut sa, 0.5);
                    changed |= num_field(&mut c[1], "End °", &mut ea, 0.5);
                });
            }
            if changed {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Curve(Curve::Ellipse(ref mut a)) = e.kind
                {
                    a.center = Point2d::from_f64(cx, cy);
                    a.semi_major = major.max(0.001);
                    a.semi_minor = minor.max(0.001);
                    a.rotation = rot.to_radians();
                    if !is_full {
                        a.start_angle = sa.to_radians();
                        a.end_angle = ea.to_radians();
                    }
                }
            }
        }
        EntityKind::Curve(Curve::Poly(pc)) => {
            use oxidraft_geometry::CurveSegment;
            prop_section(ui, "GEOMETRY");
            let segs = &pc.segments;
            let all_lines = !segs.is_empty() && segs.iter().all(|s| matches!(s, Curve::Line(_)));
            if !all_lines {
                ui.add(egui::Label::new(
                    egui::RichText::new(format!("{} segments — edit on canvas", segs.len()))
                        .size(11.0)
                        .italics()
                        .color(crate::theme::TEXT_DIM),
                ));
            } else {
                let mut verts: Vec<(f64, f64)> = Vec::with_capacity(segs.len() + 1);
                let (t0, _) = segs[0].domain();
                verts.push(segs[0].evaluate_f64(t0));
                for s in segs {
                    let (_, t1) = s.domain();
                    verts.push(s.evaluate_f64(t1));
                }
                let closed = {
                    let (a, b) = (verts[0], *verts.last().unwrap());
                    (a.0 - b.0).hypot(a.1 - b.1) < 1e-6
                };
                if closed {
                    verts.pop();
                }
                let mut changed = false;
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for (k, v) in verts.iter_mut().enumerate() {
                            prop_caption(ui, &format!("Vertex {}", k + 1));
                            changed |= xy_fields(ui, "X", &mut v.0, "Y", &mut v.1, 0.01);
                            ui.add_space(2.0);
                        }
                    });
                if changed {
                    app.history.snapshot(&app.document);
                    let n = verts.len();
                    let limit = if closed { n } else { n - 1 };
                    let mut new_segs: Vec<Curve> = Vec::with_capacity(limit);
                    for i in 0..limit {
                        let a = verts[i];
                        let b = verts[(i + 1) % n];
                        new_segs.push(Curve::Line(oxidraft_geometry::LineSeg::from_endpoints(
                            Point2d::from_f64(a.0, a.1),
                            Point2d::from_f64(b.0, b.1),
                        )));
                    }
                    if let Some(e) = app.document.get_mut(id)
                        && let EntityKind::Curve(Curve::Poly(ref mut p)) = e.kind
                    {
                        **p = oxidraft_geometry::PolyCurve::new(new_segs);
                    }
                }
            }
        }
        EntityKind::Hatch { pattern, .. } => {
            prop_section(ui, "PATTERN");
            let mut pat = *pattern;
            if hatch_pattern_editor(ui, &mut pat) {
                app.history.snapshot(&app.document);
                if let Some(e) = app.document.get_mut(id)
                    && let EntityKind::Hatch {
                        pattern: ref mut p, ..
                    } = e.kind
                {
                    *p = pat;
                }
                app.hatch_pattern = pat;
            }
        }
        _ => {
            prop_section(ui, "GEOMETRY");
            ui.add(egui::Label::new(
                egui::RichText::new("Not editable here")
                    .size(11.0)
                    .italics()
                    .color(crate::theme::TEXT_DIM),
            ));
        }
    }
}

fn hatch_pattern_editor(ui: &mut egui::Ui, pattern: &mut oxidraft_document::HatchPattern) -> bool {
    use oxidraft_document::HatchPattern as HP;
    let mut changed = false;
    let kind = match pattern {
        HP::Solid => "Solid",
        HP::Lines { .. } => "Lines",
        HP::Cross { .. } => "Cross-hatch",
        HP::Dots { .. } => "Dots",
    };
    egui::ComboBox::from_id_salt("hatch_pat")
        .selected_text(kind)
        .show_ui(ui, |ui| {
            let (a, s) = match *pattern {
                HP::Lines { angle_deg, spacing } | HP::Cross { angle_deg, spacing } => {
                    (angle_deg, spacing)
                }
                HP::Dots { spacing } => (45.0, spacing),
                HP::Solid => (45.0, 1.0),
            };
            for (label, cand) in [
                ("Solid", HP::Solid),
                (
                    "Lines",
                    HP::Lines {
                        angle_deg: a,
                        spacing: s.max(0.1),
                    },
                ),
                (
                    "Cross-hatch",
                    HP::Cross {
                        angle_deg: a,
                        spacing: s.max(0.1),
                    },
                ),
                (
                    "Dots",
                    HP::Dots {
                        spacing: s.max(0.1),
                    },
                ),
            ] {
                let selected = std::mem::discriminant(pattern) == std::mem::discriminant(&cand);
                if ui.selectable_label(selected, label).clicked() && !selected {
                    *pattern = cand;
                    changed = true;
                }
            }
        });
    match pattern {
        HP::Lines { angle_deg, spacing } | HP::Cross { angle_deg, spacing } => {
            ui.add_space(4.0);
            ui.columns(2, |c| {
                changed |= num_field(&mut c[0], "Angle °", &mut *angle_deg, 1.0);
                changed |= num_field(&mut c[1], "Spacing", &mut *spacing, 0.05);
            });
            *spacing = spacing.max(0.05);
        }
        HP::Dots { spacing } => {
            ui.add_space(4.0);
            changed |= num_field(ui, "Spacing", &mut *spacing, 0.05);
            *spacing = spacing.max(0.05);
        }
        HP::Solid => {}
    }
    changed
}

fn maybe_save(app: &mut AppState) -> bool {
    if !app.is_dirty() {
        return true;
    }
    let name = app
        .current_file_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Untitled".to_string());
    let res = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title("Unsaved changes")
        .set_description(format!("Save changes to \"{name}\" before continuing?"))
        .set_buttons(rfd::MessageButtons::YesNoCancel)
        .show();
    match res {
        rfd::MessageDialogResult::Yes => {
            if !app.save_file() {
                file_save_as(app);
            }
            !app.is_dirty()
        }
        rfd::MessageDialogResult::No => {
            crate::autosave::discard_recovery();
            true
        }
        _ => false,
    }
}

fn file_open(app: &mut AppState) {
    if let Some(path) = FileDialog::new()
        .add_filter(
            "All supported (o2d, e2d, dxf, svg)",
            &["o2d", "e2d", "dxf", "svg"],
        )
        .add_filter("oxiDRAFT drawing", &["o2d", "e2d"])
        .add_filter("DXF (ASCII)", &["dxf"])
        .add_filter("SVG", &["svg"])
        .pick_file()
    {
        app.open_file(path);
    }
}

fn file_save_as(app: &mut AppState) {
    let suggested = app
        .current_file_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled.o2d".to_string());
    if let Some(path) = FileDialog::new()
        .add_filter("oxiDRAFT drawing", &["o2d"])
        .add_filter("DXF (ASCII)", &["dxf"])
        .add_filter("SVG", &["svg"])
        .set_file_name(&suggested)
        .save_file()
    {
        app.save_file_to(path);
    }
}
