//! The oxiDRAFT executable: launches the eframe/egui desktop GUI, falling
//! back to a small non-interactive kernel demo if the GUI can't start (e.g.
//! no display) or if run with `demo`/`cli`/`--demo`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use oxidraft_geometry::{CircularArc, Curve, LineSeg, Point2d, intersect};
use oxidraft_ui::{AppState, UiPrefs, UiState, draw_ui, egui};

/// Storage key the saved [`UiPrefs`] are persisted under between sessions.
const PREFS_KEY: &str = "oxidraft_ui_prefs";

fn main() {
    std::panic::set_hook(Box::new(|info| {
        log_init();
        log(&format!("PANIC: {info}"));
    }));

    match std::env::args().nth(1).as_deref() {
        Some("demo") | Some("cli") | Some("--demo") => {
            run_demo();
        }
        _ => {
            log_init();
            if let Err(e) = run_gui() {
                log(&format!(
                    "GUI failed to start ({e}). Running the kernel demo instead."
                ));
                run_demo();
            }
        }
    }
}

/// Where the crash/startup log is written: next to the executable, or the
/// system temp dir if that location can't be determined.
fn log_path() -> std::path::PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
        .join("oxidraft_log.txt")
}

/// Truncates the log file and writes its header, for a fresh log per run.
fn log_init() {
    let _ = std::fs::write(log_path(), "oxiDRAFT log\n=============\n");
}

/// Appends a line to the log file and echoes it to stderr.
fn log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = writeln!(f, "{msg}");
    }
    eprintln!("{msg}");
}

/// The eframe application: owns the document/editor state and the
/// transient UI state redrawn each frame.
struct OxidraftCad {
    app: AppState,
    ui: UiState,
}

impl eframe::App for OxidraftCad {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        draw_ui(ui, &mut self.app, &mut self.ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(PREFS_KEY, self.app.ui_prefs().serialize());
    }
}

/// Opens the native window and runs the eframe event loop until the user
/// closes it, restoring saved [`UiPrefs`] on startup.
fn run_gui() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("oxiDRAFT")
            .with_icon(std::sync::Arc::new(oxidraft_ui::icons::app_icon()))
            .with_min_inner_size([900.0, 560.0])
            .with_inner_size([1200.0, 800.0]),
        multisampling: 4,
        ..Default::default()
    };
    eframe::run_native(
        "oxiDRAFT",
        options,
        Box::new(|cc| {
            log("Window created. Using the adaptive-tessellation egui painter.");
            let mut app = AppState::new(1200.0, 800.0);
            if let Some(s) = cc.storage.and_then(|s| s.get_string(PREFS_KEY)) {
                app.apply_prefs(&UiPrefs::deserialize(&s));
            }
            Ok(Box::new(OxidraftCad {
                app,
                ui: UiState::default(),
            }))
        }),
    )
}

/// Prints a small non-interactive demo of the geometry kernel (a line/circle
/// intersection) to stdout — the fallback when the GUI can't start, and the
/// `demo`/`cli`/`--demo` command-line mode.
fn run_demo() {
    println!("=== oxiDRAFT — Geometry Kernel Demo ===\n");

    let line = Curve::Line(LineSeg::from_endpoints(
        Point2d::from_f64(-8.0, 7.25),
        Point2d::from_f64(8.0, -4.75),
    ));
    let circle = Curve::Arc(CircularArc::new(
        Point2d::from_f64(0.0, 0.0),
        5.0,
        0.0,
        std::f64::consts::TAU,
    ));

    println!("Curve 1 (line):   3x + 4y - 5 = 0");
    println!("Curve 2 (circle): x² + y² - 25 = 0\n");

    let hits = intersect(&line, &circle);
    println!("Found {} intersection point(s):\n", hits.len());
    for (i, h) in hits.iter().enumerate() {
        let (x, y) = h.point;
        println!("  Point {}: x = {:.10},  y = {:.10}", i + 1, x, y);
        let line_err = (3.0 * x + 4.0 * y - 5.0).abs();
        let circle_err = (x * x + y * y - 25.0).abs();
        println!("    Residual on line:   {:.2e}", line_err);
        println!("    Residual on circle: {:.2e}", circle_err);
    }

    println!("\nGeometry runs on f64 + tolerance (robust, NURBS-ready kernel).");
    println!("Run `oxidraft_app` (no args) to launch the interactive CAD application.");
}
