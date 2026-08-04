# Anchor-aware Smart Dimension

**Status:** approved, not yet implemented
**Date:** 2026-08-04

## Problem

Smart Dimension cannot dimension to a point. `AppState::smart_dimension`
(`crates/oxidraft_ui/src/state.rs`) is typed `(a: EntityId, b: Option<EntityId>,
place)` — it takes whole entities and has no way to name a point *on* one. So
the four dimensions it can produce are the only four it will ever produce:

| Picks | Constraint |
| ----- | ---------- |
| one line | `Distance` (length) |
| one circle/arc | `Radius` |
| two parallel lines | `LineDistance` |
| two crossing lines | `Angle` |

Dimensioning between two circle centres is impossible, not because the
constraint is missing, but because there is no pick that means "this centre".

Three constraint kinds are already built for exactly this and have **no UI
path whatsoever**:

- `ConstraintKind::PointDistance`, `HDistance`, `VDistance` — declared in
  `oxidraft_document`, lowered by the solver
  (`crates/oxidraft_constraint/src/lib.rs`, `Constraint::Distance` /
  `HorizontalDistance` / `VerticalDistance`), constructed by the finished
  `constrain_point_distance()`
  (`crates/oxidraft_cad/src/constrain.rs`), drawn as badges, and saved and
  loaded. Nothing in the UI calls any of it: no command verb, no button, no
  tool. `constrain_point_distance` has zero callers outside its own tests.

Anchors themselves are *not* the gap. `ANCHOR_DERIVED` already resolves a
line's midpoint and an arc's centre for the relational constraints
(`weld_anchor_at` in `state/modify.rs`, `ShapeVars::anchor` in `constrain.rs`),
so welding to a circle centre works today. Only the dimensioning path cannot
reach an anchor.

## Approach

Teach Smart Dimension to pick point anchors, which makes the three orphaned
kinds reachable, and add one new kind for point-to-line.

### Pick model

`Tool::DimConstraint`'s `first: Option<EntityId>` is what forces entity-level
picking. It is replaced by a target that can be either:

```rust
/// One thing Smart Dimension has picked: a whole entity, or a point on one.
enum DimTarget {
    /// The entity itself — a line (its length) or circle/arc (its radius).
    Entity(EntityId),
    /// A point anchor: (entity, anchor index, resolved world position).
    Anchor(EntityId, u8, Point2d),
}
```

Which one a click produces is read off the pick's `SnapKind`:
`Endpoint`, `Midpoint`, `Center`, and `Node` yield `Anchor`; everything else
(including no snap) yields `Entity`. This is the rule `Tool::Line` already uses
to tell "tangent to this circle" from "endpoint here", and `Tool::Circle` uses
to read its construction — no new modality is introduced.

`Anchor` carries its resolved world position inline, for the same reason
`TanAnchor` does: the live preview and the commit then need no document lookup,
and typed-coordinate entry keeps working the way it already does for lines.

Tab re-cycles the reading of the pick just made, so a snap that guessed wrong
is corrected in place rather than by cancelling the pick. It flips that one
pick between its two readings — `Anchor` on some entity, or `Entity` on the
entity that anchor belongs to — so a centre-snap that was meant as "this
circle's radius" becomes it, and a mid-span click that was meant as "this
line's midpoint" becomes that. `Tool::cycle_reading` already exists for this
(it drives Circle/Arc construction choice and the drafting Dimension tool's
radius/diameter flip).

### Resolution

| First pick | Second pick | Constraint |
| ---------- | ----------- | ---------- |
| line | *(placement)* | `Distance` |
| circle/arc | *(placement)* | `Radius` |
| line | parallel line | `LineDistance` |
| line | crossing line | `Angle` |
| **anchor** | **anchor** | **`PointDistance` / `HDistance` / `VDistance`** |
| **anchor** | **line** | **`PointLineDistance`** *(new)* |
| **line** | **anchor** | **`PointLineDistance`**, normalised to anchor-first |

The first four rows are today's behaviour and do not change.

For anchor-to-anchor, the placement click chooses among the three kinds via
`oxidraft_document::linear_orientation(p1, p2, line)`, which already returns
exactly this three-way answer and already drives drafting dimensions: dropping
the dimension line diagonally off the midpoint gives the aligned
`PointDistance`, directly above or below gives `HDistance`, off to either side
gives `VDistance`.

Anchor + circle/arc (point-to-rim distance) is **refused with a message**, not
guessed at. It would need a fifth kind and is out of scope.

## Constraint and persistence layer

`PointDistance` / `HDistance` / `VDistance` need **no changes**.
`constrain_point_distance()` already takes the kind, accepts `value: None` to
lock the current separation, stores `place`, validates against existing
constraints, rolls back on conflict, and re-solves. The work is calling it.

`PointLineDistance` is the one new kind:

- variant, `label()`, `code()` = `"PLDIST"`, `from_code()`
- membership in `is_pair()`, `is_valued()`, `has_anchors()`, following
  `PointOnLine` exactly: the anchor lives in `pts.0`, `b` is the whole line,
  `pts.1` is unused by construction
- a `constrain_point_line_distance()` builder shaped like its point-distance
  sibling (validate → add → conflict-check → re-solve → write back)
- solver lowering is one match arm in `component_sketch`:
  `Constraint::PointLineDistance(q, a0, a1, d)` already exists and is already
  used to implement `LineDistance`
- badge rendering joins the existing valued-distance group in `overlays.rs`
- `Icon::for_constraint` maps it to `ConLengthLock`, with its siblings

Persistence needs **no format change**. The serializer is driven entirely off
`has_anchors()` / `is_valued()` / `code()`, so an anchored-and-valued kind
lands on the existing line shape (`C PLDIST ia ea ib eb v`) in
`crates/oxidraft_io/src/native.rs`. Old files never contain `PLDIST`; a new
file carrying one will not open in an older build, which is how every kind
before it was added.

## UI surface

- Command verbs `PDIST`, `HDIST`, `VDIST`, `PLDIST` are added to the parser.
  These four kinds are currently the only ones with no verb at all.
- `tool_prompt` gains arms for the new phases ("pick a second point, or a line
  to measure to"), following its existing per-phase switch.
- The live preview reuses `draw_dimension` / `draw_ortho_dim` exactly as the
  drafting Dimension tool does, so the placement click's effect is visible
  before it is committed.
- The Smart Dimension button and its tooltip stay where they are; only the
  tooltip text widens to mention points.

## Testing

Following the repo's existing style — behavioural tests at the layer that owns
the behaviour, each pinning a specific claim.

`oxidraft_cad`:
- `PLDIST` holds a point at its distance from a line across a drag of that line
- `PDIST`/`HDIST`/`VDIST` reject a degenerate (zero) separation, per
  `constrain_point_distance`'s existing positive-value rule

`oxidraft_io`:
- a document containing all four kinds survives save → load with anchors,
  values, and placements intact (mirrors the existing native-format tests)

`oxidraft_ui`:
- pick classification: a centre-snap click yields `Anchor`, a mid-span click on
  the same circle yields `Entity`
- anchor + anchor with three placement positions yields `PointDistance`,
  `HDistance`, `VDistance` respectively
- Tab re-cycles a pick's reading
- end-to-end through `AppState`: dimension between two circle centres, which is
  the originally reported case

## Out of scope

- Point-to-rim (point-to-circle) driving distance — a fifth kind.
- Any change to the four existing entity-level dimension behaviours.
- Broadening auto-constraint inference while drawing. Today it infers
  Coincident, Horizontal, Vertical, Tangent, and EqualLength on rectangle
  corners; Parallel, Perpendicular, Concentric, EqualRadius, Collinear and
  PointOnLine are never inferred. That is a real gap and a separate spec.
