#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point2d {
    pub x: f64,
    pub y: f64,
}

impl Point2d {
    pub fn new(x: f64, y: f64) -> Self {
        Point2d { x, y }
    }

    pub fn from_i64(x: i64, y: i64) -> Self {
        Point2d {
            x: x as f64,
            y: y as f64,
        }
    }

    pub fn from_f64(x: f64, y: f64) -> Self {
        Point2d { x, y }
    }

    #[inline]
    pub fn to_f64(&self) -> (f64, f64) {
        (self.x, self.y)
    }

    #[inline]
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    #[inline]
    pub fn dist_sq(&self, other: &Point2d) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    #[inline]
    pub fn dist_f64(&self, other: &Point2d) -> f64 {
        self.dist_sq(other).sqrt()
    }

    #[inline]
    pub fn midpoint(&self, other: &Point2d) -> Point2d {
        Point2d {
            x: (self.x + other.x) / 2.0,
            y: (self.y + other.y) / 2.0,
        }
    }

    #[inline]
    pub fn lerp(&self, other: &Point2d, t: f64) -> Point2d {
        Point2d {
            x: self.x + t * (other.x - self.x),
            y: self.y + t * (other.y - self.y),
        }
    }
}

impl std::fmt::Display for Point2d {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    pub min: Point2d,
    pub max: Point2d,
}

impl BoundingBox {
    pub fn new(min: Point2d, max: Point2d) -> Self {
        BoundingBox { min, max }
    }

    pub fn from_corners(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        BoundingBox {
            min: Point2d::from_f64(x0.min(x1), y0.min(y1)),
            max: Point2d::from_f64(x0.max(x1), y0.max(y1)),
        }
    }

    pub fn contains_point_f64(&self, x: f64, y: f64) -> bool {
        x >= self.min.x && x <= self.max.x && y >= self.min.y && y <= self.max.y
    }

    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.max.x >= other.min.x
            && self.min.x <= other.max.x
            && self.max.y >= other.min.y
            && self.min.y <= other.max.y
    }

    /// True when every coordinate is a finite number — NaN or infinity in a
    /// box poisons any union/zoom-fit arithmetic it participates in.
    pub fn is_finite(&self) -> bool {
        self.min.x.is_finite()
            && self.min.y.is_finite()
            && self.max.x.is_finite()
            && self.max.y.is_finite()
    }

    pub fn union(&self, other: &BoundingBox) -> BoundingBox {
        BoundingBox {
            min: Point2d {
                x: self.min.x.min(other.min.x),
                y: self.min.y.min(other.min.y),
            },
            max: Point2d {
                x: self.max.x.max(other.max.x),
                y: self.max.y.max(other.max.y),
            },
        }
    }
}

impl std::fmt::Display for BoundingBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{} → {}]", self.min, self.max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_exact() {
        let a = Point2d::from_i64(0, 0);
        let b = Point2d::from_i64(4, 6);
        let m = a.midpoint(&b);
        assert_eq!(m, Point2d::new(2.0, 3.0));
    }

    #[test]
    fn lerp_quarter() {
        let a = Point2d::from_i64(0, 0);
        let b = Point2d::from_i64(10, 10);
        let p = a.lerp(&b, 0.25);
        assert_eq!(p, Point2d::new(2.5, 2.5));
    }

    #[test]
    fn bbox_intersects() {
        let a = BoundingBox::from_corners(0.0, 0.0, 2.0, 2.0);
        let b = BoundingBox::from_corners(1.0, 1.0, 3.0, 3.0);
        let c = BoundingBox::from_corners(5.0, 5.0, 7.0, 7.0);
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }
}
