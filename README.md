<p align="center">
  <img src="crates/oxidraft_ui/assets/logotype/oxidraft_logotype.png" alt="oxiDRAFT" width="460">
</p>

<h1 align="center">oxiDRAFT</h1>

<p align="center">
  <em>A fast, from-scratch 2D CAD system written in Rust — an exact geometry kernel under a modern, direct-manipulation interface.</em>
</p>

<p align="center">
  <a href="https://github.com/fcoltro/oxiDRAFT/actions/workflows/release.yml"><img src="https://github.com/fcoltro/oxiDRAFT/actions/workflows/release.yml/badge.svg" alt="Release build"></a>
  <a href="https://github.com/fcoltro/oxiDRAFT/releases/latest"><img src="https://img.shields.io/github/v/release/fcoltro/oxiDRAFT?include_prereleases&label=download" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/rust-2024-orange.svg" alt="Rust 2024">
  <img src="https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg" alt="GPL-3.0-or-later">
  <img src="https://img.shields.io/badge/platforms-Windows%20%C2%B7%20Linux%20%C2%B7%20macOS-555" alt="Platforms">
</p>

---

**oxiDRAFT** is a 2D CAD drafting environment built entirely from the ground up in
Rust — no CAD engine dependencies, no port of an existing kernel. It pairs the
precision of a real geometry core with the feel of a modern app: a deep,
glass-panelled dark UI, a `Ctrl+F` command palette, live object snapping, and
CAD-style grips on everything you draw.

The kernel works in **f64 coordinates with tolerance-based predicates**. Lines,
circular arcs, elliptical arcs, cubic Béziers, polycurves, rational Béziers and
clamped-cubic **NURBS** are first-class primitives. Intersections, offsets,
distances, curvature, **G0–G3 continuity blends** and planar booleans are
computed numerically, with **Shewchuk-exact orientation predicates** and
magnitude-relative tolerances keeping results robust even on near-degenerate or
large-coordinate input. The viewport renders through the egui painter with
**adaptive, zoom-aware tessellation** and **viewport culling** — smooth curves at
any zoom, only what's on screen is drawn.

## ⬇ Download

Pre-built binaries for **Windows, Linux and macOS** (Intel + Apple Silicon) are
published on every release:

> **[→ Download the latest release](https://github.com/fcoltro/oxiDRAFT/releases/latest)**

Or build it yourself in one command (see [Build & run](#-build--run)).

## ✨ Features

### Draw
- **Line** and **Polyline**
- **Circle** — center+radius, 2-point (diameter), 3-point, tangent-tangent-radius (TTR), tangent-tangent-tangent (TTT)
- **Arc** — 3-point, start-center-end, center-start-end
- **Tangent line** — from a point or common to two circles/arcs
- **Ellipse** (center + two axes), **Rectangle**, **Polygon** (n-sided, type the count)
- **NURBS spline** — control-vertex authoring with draggable CV grips and per-vertex weights
- **Text** with a user-selectable on-canvas font (TTF/OTF), drawn as filled vector outlines; multi-line supported

### Modify
- **Move, Copy, Rotate, Scale, Mirror, Stretch**
- **Cut / Copy / Paste** (`Ctrl+X / C / V`) — paste lands at the cursor
- **Offset** — segment-mitred for polylines/polygons, exact for NURBS
- **Trim & Extend** — span-aware and spline-preserving
- **Fillet & Chamfer** — on two lines or around polyline corners
- **Blend** — bridge two entities (line, arc or spline) with a spline at **G0 / G1 / G2 / G3** continuity (positional, tangent, curvature, or curvature-rate); pick the continuity and tension live in the cursor HUD
- **Disjoint** (explode) and **Join**
- **Hatch** — region-based fill (solid, line, cross-hatch or dot patterns) with island detection
- CAD-style **grips** on every selected entity — drag to reshape, or type an exact value
- Contextual corner fillet/chamfer dots and bounding-box transform handles
- A floating **contextual toolbar** (duplicate, mirror, rotate, offset, delete) above the selection — follows multi-selections too

### Dimensions
- **Linear / Aligned** — distance between two points, parallel to the measured span
- **Horizontal / Vertical** — axis-locked linear dimensions
- **Angular** — from three points (vertex + a point on each side) or from two picked lines
- **Radius / Diameter** — pick a circle or arc, then aim the leader (`R` / `⌀`)
- Per-dimension **text override**, and a drawing-wide **decimal-precision** setting
- One shared dimension style (text height, arrow size, font, precision); dimensions land on an auto-created **Dimensions** layer

### Parametric constraints
- **Geometric** — Horizontal, Vertical, Parallel, Perpendicular, Equal length, Coincident (weld endpoints), and Tangent (line↔arc and arc↔arc)
- **Driving dimensions** — lock or drive a circle/arc's **radius** (`RADCON` / `DIACON`) or a line's **length** (`LENCON`); a bare command holds the current value, a numeric argument resizes to it
- Constraints **re-solve live** as you drag grips, move, or edit properties — a minimal-motion numeric solver pulls constrained neighbours along and keeps welded corners together
- On-canvas **glyph badges** show what's constrained; **click a badge to delete** that constraint, and the Properties inspector lists (and removes) every relation on the selection

### Snapping & input
- **Object snaps** — Endpoint, Midpoint, Center, Quadrant, Intersection, Perpendicular, Tangent, Nearest, Node, Insertion (each individually toggleable, with on-canvas markers + labels)
- **Grid** and **grid snap**, **polar / angle guides**, and **extension tracking**
- **Dynamic input HUD** — type a length/angle, radius, width×height or side count right at the cursor; sizes grow in the direction you aim (no negative numbers needed)
- **Coordinate entry** — `x,y` absolute, `@dx,dy` relative, `d<a` polar absolute, `@d<a` polar relative (degrees)
- A translucent **tool-hint panel** (bottom-right) listing the active tool's keys, with getting-started tips on a blank canvas
- **Cursor readout** for move/copy/rotate/scale, and a live **scale bar**

### Workspace & UI
- **Layers** — colour, show/hide, freeze, lock, rename, set-current, per-layer line type & weight; seeded with a sensible default set
- Editable **Properties inspector** — geometry, live measurements, colour, line weight, line type and layer; hatch-pattern editor; dimension text override
- **Line types** (Continuous, Dashed, Dotted, Center, or custom dash patterns) and **line weights** shown at a fixed on-screen scale
- **Drawing units** — mm / cm / m / km / in / ft / unitless — that bound the zoom range and label measurements
- **Curvature comb** on selected curves for smoothness inspection
- **`Ctrl+F` command palette** plus an always-available command line, **window / crossing marquee**, hover highlight, ghost previews, **undo / redo**
- **Radial tool wheel** — press `Q` for a **Tools** / **Modifiers** picker at the cursor; move toward either to reveal its full ring, then to a wedge and click to activate it, or push out further on Circle/Arc/Dimension/Line to reveal their construction-method variants; press `Q` again or `Esc` to dismiss
- Modern dark, glass-panelled interface — top bar, inspector and status pill; preferences persist between sessions

### Geometry kernel
- Curve primitives: line, circular arc, elliptical arc, cubic Bézier, rational Bézier, polycurve, clamped-cubic **NURBS**
- Numeric **intersect / distance / curvature / offset / split / reverse**; tangent solvers (3-point circle, TTR, TTT, tangent lines)
- **Continuity blends** — a single degree-`2n+1` polynomial Bézier matching position, tangent, curvature vector and curvature rate at both joins for exact **G0–G3**
- Exact **boolean region** ops (union / intersection / difference / xor) via Greiner–Hormann clipping with robust winding
- **Shewchuk-exact** orientation predicates (via the `robust` crate); collinearity / parallelism tests use **magnitude-relative tolerances** so they hold at CAD-scale coordinates
- Affine **transforms** (translate, rotate, scale, mirror) — **conformal-aware**: a non-uniform scale or shear lowers a circle/arc to its exact rational form instead of corrupting it into a wrong-radius arc
- **Fallible constructors** (`try_new` + a `GeomError` enum) alongside the panicking `new`, so untrusted/imported geometry is rejected rather than crashing
- Shared numeric utilities (angle normalisation, point/segment distance) reused across every crate — 100% safe Rust (`unsafe` is forbidden workspace-wide)

### Interoperability
- Native **`.o2d`** format — lossless, atomic saves (opens old `.e2d` files too)
- **DXF** (ASCII) — import & export
- **SVG** — import & export
- Dimensions export to DXF/SVG as ordinary lines + text (so they render anywhere)

## ⌨ Commands

Type a verb in the command line, or use the `Draw`/`Modify` menus, the radial tool wheel (`Q`), or the `Ctrl+F` palette. Common aliases:

| Draw | | Modify | | Other | |
|------|--|--------|--|-------|--|
| `LINE` / `L` | Line | `MOVE` / `M` | Move | `SELECT` / `SE` | Select |
| `POLYLINE` / `PL` | Polyline | `COPY` / `CO` | Copy | `ERASE` / `E` / `DELETE` | Delete |
| `CIRCLE` / `C` | Circle (center, radius) | `ROTATE` / `RO` | Rotate | `DISJOINT` / `X` | Disjoint (explode) |
| `CIRCLE2P` / `C2P` | Circle (2 points) | `SCALE` / `SC` | Scale | `JOIN` / `J` | Join |
| `CIRCLE3P` / `C3P` | Circle (3 points) | `MIRROR` / `MI` | Mirror | `HATCH` / `H` | Hatch |
| `TTR` · `TTT` | Tangent circles | `OFFSET` / `O` | Offset | `UNDO` / `U` | Undo |
| `ARC` / `A` | Arc (3-point) | `TRIM` / `TR` | Trim | `REDO` | Redo |
| `ARCSCE` · `ARCCSE` | Arc (SCE / CSE) | `EXTEND` / `EX` | Extend | `ALL` | Select all |
| `TANGENT` / `TAN` | Tangent line | `FILLET` / `F` | Fillet | `ZOOM` / `Z` | Zoom / extents |
| `ELLIPSE` / `EL` | Ellipse | `CHAMFER` / `CHA` | Chamfer | `LAYER` / `LA` | Layer set / new |
| `RECTANGLE` / `REC` | Rectangle | `BLEND` / `BL` | Blend (G0–G3) | | |
| `POLYGON` / `POL` | Polygon | `STRETCH` / `S` | Stretch | | |
| `SPLINE` / `SPL` | NURBS spline | **Dimension** | | | |
| `TEXT` / `T` / `MTEXT` | Text | `DIMENSION` / `DIM` | Aligned | | |
| | | `DIMHOR` · `DIMVER` | Horizontal / Vertical | | |
| | | `DIMANG` · `DIMANGL` | Angular (3-pt / 2-line) | | |
| | | `DIMRAD` · `DIMDIA` | Radius / Diameter | | |

`POLYGON n` and `TTR r` accept an inline argument (side count / radius), and
`BLEND g2 1.5` presets the continuity and tension. Polyline and spline finish
with **Enter** or right-click, and close with **C**.

Constrain the current selection with `HOR` · `VER` (horizontal / vertical),
`PAR` · `PERP` (parallel / perpendicular), `EQL` (equal length), `COI`
(coincident), `TANCON` (tangent), `RADCON` · `DIACON` (drive a radius /
diameter) and `LENCON` (drive a line's length). The value commands take an
optional number — `RADCON 2.5`, `LENCON 40` — or lock the current value when
bare. `UNCON` drops every constraint on the selection.

### Keyboard shortcuts

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `L P C E A R G S T H` | Line / Polyline / Circle / Ellipse / Arc / Rectangle / polyGon / Spline / Text / Hatch | `Ctrl+N O S` | New / Open / Save |
| `Shift+ M C R S A I` | Move / Copy / Rotate / Stretch / scAle / mIrror | `Ctrl+Shift+S` | Save As |
| `Shift+ O T E F H B` | Offset / Trim / Extend / Fillet / cHamfer / Blend | `Ctrl+Z` · `Ctrl+Y` | Undo / Redo |
| `Shift+ X J` | disjoint (eXplode) / Join | `Ctrl+X C V` | Cut / Copy / Paste |
| `Esc` | Cancel / deselect | `Ctrl+A` | Select all |
| `Z` | Zoom extents | `Ctrl+F` | Command palette |
| `Space` | Repeat last command | `Del` | Delete selection |
| `Q` | Radial tool wheel — move to Tools or Modifiers, then a wedge | | |
| `F7`–`F12` | Toggle object snap · grid · grid snap · polar · tracking · dynamic input | | |

## 🔨 Build & run

Plain Cargo, no special toolchain — just a stable Rust toolchain.

```sh
cargo build --workspace
cargo test  --workspace

cargo run -p oxidraft_app          # launch the interactive CAD window
cargo run -p oxidraft_app -- demo  # headless geometry-kernel demo
```

> **Linux build dependencies:** the file dialogs use GTK, so install
> `libgtk-3-dev` (Debian/Ubuntu: `sudo apt-get install libgtk-3-dev`) before building.

## 🧱 Architecture

oxiDRAFT is a Cargo **workspace** — the kernel is fully decoupled from the UI, so
every crate below `oxidraft_ui` is headless and independently testable. Shared
package metadata, dependency versions and lints are defined once at the workspace
root and inherited by every crate.

| Crate | Responsibility |
|-------|----------------|
| `oxidraft_geometry` | Curve primitives (line, arc, ellipse, cubic, rational, polycurve, NURBS), conformal-aware transforms, ops (intersect / distance / curvature / offset / split / tangent / blend), and shared numeric utilities (angle / point-segment helpers) reused across every crate |
| `oxidraft_spatial` | Adaptive quadtree + Morton-code spatial index (standalone; reserved for upcoming query acceleration / 3D — picking currently uses bounding-box-filtered scans) |
| `oxidraft_boolean` | Planar region boolean ops (union / intersection / difference / xor) with robust winding |
| `oxidraft_document` | Document / layer / entity / block model, plus the shared dimension geometry + labelling used by both the renderer and the exporters |
| `oxidraft_cad` | Snapping, selection, grips, draw + edit (trim / extend / fillet / chamfer / blend / offset / hatch / join / explode / inquiry) |
| `oxidraft_io` | DXF, SVG and native `.o2d` import / export |
| `oxidraft_ui` | Headless app state + egui view (toolbars, canvas, panels, command palette, dialogs) |
| `apps/oxidraft_app` | eframe GUI host + headless kernel demo |

> `.o2d` is the current native format (was `.e2d` before the project's rename
> from eiderFLAT to oxiDRAFT) — the loader still reads old `.e2d` files, it
> just no longer writes them.

## 📄 License

**oxiDRAFT is free software, licensed under the GNU General Public License v3.0 or
later** (`GPL-3.0-or-later`) — see [LICENSE](LICENSE). You may use, study, modify
and redistribute it under those terms; derivative works must remain GPL-licensed
and share their source.
