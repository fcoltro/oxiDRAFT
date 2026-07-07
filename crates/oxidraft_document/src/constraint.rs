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
        )
    }

    /// Valued kinds carry a driving number in `val` (a radius, a length);
    /// their record is corrupt without a positive value.
    pub fn is_valued(&self) -> bool {
        matches!(self, ConstraintKind::Radius | ConstraintKind::Distance)
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
            _ => return None,
        })
    }
}

/// A persistent geometric relation between line entities. Pair kinds keep
/// `a` as the reference entity picked first; single kinds leave `b` empty.
/// Point-level kinds (Coincident) carry the endpoint index (0 or 1) of each
/// entity in `pts`. Valued kinds (Radius, Distance) carry their driving
/// value in `val`, which identifies the constraint's target, not its
/// relation: two Radius constraints on one arc are the same relation with
/// different values.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SketchConstraint {
    pub kind: ConstraintKind,
    pub a: EntityId,
    pub b: Option<EntityId>,
    pub pts: Option<(u8, u8)>,
    pub val: Option<f64>,
}

impl SketchConstraint {
    pub fn single(kind: ConstraintKind, a: EntityId) -> Self {
        SketchConstraint {
            kind,
            a,
            b: None,
            pts: None,
            val: None,
        }
    }

    pub fn pair(kind: ConstraintKind, a: EntityId, b: EntityId) -> Self {
        SketchConstraint {
            kind,
            a,
            b: Some(b),
            pts: None,
            val: None,
        }
    }

    pub fn coincident(a: EntityId, ea: u8, b: EntityId, eb: u8) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Coincident,
            a,
            b: Some(b),
            pts: Some((ea, eb)),
            val: None,
        }
    }

    pub fn radius(a: EntityId, value: f64) -> Self {
        SketchConstraint {
            kind: ConstraintKind::Radius,
            a,
            b: None,
            pts: None,
            val: Some(value),
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
