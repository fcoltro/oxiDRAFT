use crate::entity::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintKind {
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    EqualLength,
    Coincident,
    /// A line entity tangent to a circular-arc entity (either order).
    Tangent,
    /// A circular-arc entity holding a driving radius, stored in `val`.
    Radius,
    /// A line entity holding a driving length, stored in `val`.
    Distance,
    /// Two parallel line entities held at a driving perpendicular distance
    /// (a width), stored in `val`. `a` is the reference line; `b` slides.
    LineDistance,
    /// Two line entities held at a driving angle. `val` is in degrees,
    /// normalized to (0, 180] — lines are undirected, so θ and θ+180° are
    /// the same relation, and 0° is recorded as 180° to survive the
    /// loader's positive-value check.
    Angle,
    /// A point entity permanently pinned wherever it currently sits — set
    /// automatically on structural anchors (the origin), never by a user
    /// command, so there's no bar/menu path that creates one.
    Fixed,
}

impl ConstraintKind {
    pub fn label(&self) -> &'static str {
        match self {
            ConstraintKind::Horizontal => "horizontal",
            ConstraintKind::Vertical => "vertical",
            ConstraintKind::Parallel => "parallel",
            ConstraintKind::Perpendicular => "perpendicular",
            ConstraintKind::EqualLength => "equal length",
            ConstraintKind::Coincident => "coincident",
            ConstraintKind::Tangent => "tangent",
            ConstraintKind::Radius => "radius",
            ConstraintKind::Distance => "length",
            ConstraintKind::LineDistance => "distance",
            ConstraintKind::Angle => "angle",
            ConstraintKind::Fixed => "fixed",
        }
    }

    /// Pair kinds relate two entities; the rest apply to one entity each.
    pub fn is_pair(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Parallel
                | ConstraintKind::Perpendicular
                | ConstraintKind::EqualLength
                | ConstraintKind::Coincident
                | ConstraintKind::Tangent
                | ConstraintKind::Angle
                | ConstraintKind::LineDistance
        )
    }

    /// Valued kinds carry a driving number in `val` (a radius, a length,
    /// a width, an angle); their record is corrupt without a positive value.
    pub fn is_valued(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Radius
                | ConstraintKind::Distance
                | ConstraintKind::LineDistance
                | ConstraintKind::Angle
        )
    }

    pub fn code(&self) -> &'static str {
        match self {
            ConstraintKind::Horizontal => "H",
            ConstraintKind::Vertical => "V",
            ConstraintKind::Parallel => "PAR",
            ConstraintKind::Perpendicular => "PERP",
            ConstraintKind::EqualLength => "EQL",
            ConstraintKind::Coincident => "COI",
            ConstraintKind::Tangent => "TAN",
            ConstraintKind::Radius => "RAD",
            ConstraintKind::Distance => "LEN",
            ConstraintKind::LineDistance => "LDIST",
            ConstraintKind::Angle => "ANG",
            ConstraintKind::Fixed => "FIX",
        }
    }

    pub fn from_code(s: &str) -> Option<ConstraintKind> {
        Some(match s {
            "FIX" => ConstraintKind::Fixed,
            "H" => ConstraintKind::Horizontal,
            "V" => ConstraintKind::Vertical,
            "PAR" => ConstraintKind::Parallel,
            "PERP" => ConstraintKind::Perpendicular,
            "EQL" => ConstraintKind::EqualLength,
            "COI" => ConstraintKind::Coincident,
            "TAN" => ConstraintKind::Tangent,
            "RAD" => ConstraintKind::Radius,
            "LEN" => ConstraintKind::Distance,
            "LDIST" => ConstraintKind::LineDistance,
            "ANG" => ConstraintKind::Angle,
            _ => return None,
        })
    }
}

/// Folds an angle in degrees into the canonical (0, 180]: lines are
/// undirected, so θ and θ+180° name the same relation, and 0° stores as
/// 180° so the record keeps a positive driving value (the loader rejects
/// non-positive ones). Non-finite input is left for the caller to reject.
pub fn normalize_angle_deg(deg: f64) -> f64 {
    if !deg.is_finite() {
        return deg;
    }
    let a = deg.rem_euclid(180.0);
    if a == 0.0 { 180.0 } else { a }
}

/// Anchor index in [`SketchConstraint::pts`] naming an entity's derived
/// point rather than an endpoint: a line's midpoint, an arc's center.
pub const ANCHOR_DERIVED: u8 = 2;

/// A persistent geometric relation between line entities. Pair kinds keep
/// `a` as the reference entity picked first; single kinds leave `b` empty.
/// Point-level kinds (Coincident) carry an anchor index for each entity in
/// `pts`: 0/1 name the entity's endpoints, and the special index
/// [`ANCHOR_DERIVED`] (2) names its derived point — a line's midpoint or an
/// arc/circle's center. Point entities use 0. Valued kinds (Radius,
/// Distance) carry their driving value in `val`, which identifies the
/// constraint's target, not its relation: two Radius constraints on one arc
/// are the same relation with different values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchConstraint {
    pub kind: ConstraintKind,
    pub a: EntityId,
    pub b: Option<EntityId>,
    pub pts: Option<(u8, u8)>,
    pub val: Option<f64>,
    /// Where the user placed the dimension annotation (world coordinates),
    /// for valued kinds dimensioned interactively. `None` falls back to the
    /// automatic layout. Not part of the relation's identity.
    pub place: Option<(f64, f64)>,
}

impl SketchConstraint {
    pub fn single(kind: ConstraintKind, a: EntityId) -> Self {
        SketchConstraint {
            kind,
            a,
            b: None,
            pts: None,
            val: None,
            place: None,
        }
    }

    pub fn pair(kind: ConstraintKind, a: EntityId, b: EntityId) -> Self {
        SketchConstraint {
            kind,
            a,
            b: Some(b),
            pts: None,
            val: None,
            place: None,
        }
    }

    pub fn coincident(a: EntityId, ea: u8, b: EntityId, eb: u8) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Coincident,
            a,
            b: Some(b),
            pts: Some((ea, eb)),
            val: None,
            place: None,
        }
    }

    pub fn radius(a: EntityId, value: f64) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Radius,
            a,
            b: None,
            pts: None,
            val: Some(value),
            place: None,
        }
    }

    /// A driving angle between two line entities, in degrees normalized
    /// to (0, 180] (see [`ConstraintKind::Angle`]).
    pub fn angle(a: EntityId, b: EntityId, degrees: f64) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Angle,
            a,
            b: Some(b),
            pts: None,
            val: Some(normalize_angle_deg(degrees)),
            place: None,
        }
    }

    /// A driving length on a single line entity, stored in `val`.
    pub fn distance(a: EntityId, value: f64) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Distance,
            a,
            b: None,
            pts: None,
            val: Some(value),
            place: None,
        }
    }

    /// A driving perpendicular distance (width) between two parallel line
    /// entities; `a` is the reference, `b` slides to meet the value.
    pub fn line_distance(a: EntityId, b: EntityId, value: f64) -> Self {
        SketchConstraint {
            kind: ConstraintKind::LineDistance,
            a,
            b: Some(b),
            pts: None,
            val: Some(value),
            place: None,
        }
    }

    /// Permanently pins a point entity wherever it currently sits.
    pub fn fixed(a: EntityId) -> Self {
        SketchConstraint::single(ConstraintKind::Fixed, a)
    }

    pub fn references(&self, id: EntityId) -> bool {
        self.a == id || self.b == Some(id)
    }

    /// All pair kinds are symmetric relations, so a duplicate with the
    /// entities (and endpoint indices) swapped is still the same constraint.
    /// `val` is deliberately not compared: a valued constraint on the same
    /// geometry is the same relation with a different target.
    pub fn same_relation(&self, other: &SketchConstraint) -> bool {
        if self.kind != other.kind {
            return false;
        }
        let straight = self.a == other.a && self.b == other.b && self.pts == other.pts;
        let swapped = self.b.is_some()
            && Some(self.a) == other.b
            && self.b == Some(other.a)
            && self.pts.map(|(x, y)| (y, x)) == other.pts;
        straight || swapped
    }
}
