#[derive(Clone, Debug)]
pub struct ViewTransform {
    pub center: (f64, f64),
    pub zoom: f64,
    pub width: f64,
    pub height: f64,
    pub min_visible: f64,
    pub max_visible: f64,
}

impl ViewTransform {
    pub fn new(width: f64, height: f64) -> Self {
        ViewTransform {
            center: (0.0, 0.0),
            zoom: 50.0,
            width,
            height,
            min_visible: 0.05,
            max_visible: 50_000.0,
        }
    }

    pub fn set_visible_range(&mut self, min_visible: f64, max_visible: f64) {
        self.min_visible = min_visible.max(1e-12);
        self.max_visible = max_visible.max(self.min_visible * 2.0);
        self.zoom = self.clamp_zoom(self.zoom);
    }

    fn clamp_zoom(&self, zoom: f64) -> f64 {
        let dim = self.width.max(self.height).max(1.0);
        let zoom_min = dim / self.max_visible;
        let zoom_max = dim / self.min_visible;
        zoom.clamp(zoom_min, zoom_max)
    }

    pub fn world_to_screen(&self, wx: f64, wy: f64) -> (f64, f64) {
        let sx = (wx - self.center.0) * self.zoom + self.width / 2.0;
        let sy = (self.center.1 - wy) * self.zoom + self.height / 2.0;
        (sx, sy)
    }

    pub fn screen_to_world(&self, sx: f64, sy: f64) -> (f64, f64) {
        let wx = self.center.0 + (sx - self.width / 2.0) / self.zoom;
        let wy = self.center.1 - (sy - self.height / 2.0) / self.zoom;
        (wx, wy)
    }

    pub fn pixel_world_size(&self) -> f64 {
        1.0 / self.zoom
    }

    pub fn zoom_percent(&self) -> f64 {
        self.zoom * 2.0
    }

    pub fn grid_spacing(&self) -> f64 {
        let raw = 80.0 * self.pixel_world_size();
        let mag = raw.log10().floor();
        let base = 10f64.powf(mag);
        if raw / base < 1.5 {
            base
        } else if raw / base < 3.5 {
            2.0 * base
        } else if raw / base < 7.5 {
            5.0 * base
        } else {
            10.0 * base
        }
    }

    pub fn snap_to_grid(&self, wx: f64, wy: f64) -> (f64, f64) {
        let g = self.grid_spacing();
        if !(g.is_finite() && g > 0.0) {
            return (wx, wy);
        }
        ((wx / g).round() * g, (wy / g).round() * g)
    }

    // The mutating methods below ignore non-finite input. One NaN reaching
    // `zoom` or `center` sticks — every later frame transforms through it and
    // the viewport goes permanently blank — so reject it at the boundary.

    pub fn pan_pixels(&mut self, dx: f64, dy: f64) {
        if !(dx.is_finite() && dy.is_finite()) {
            return;
        }
        self.center.0 -= dx / self.zoom;
        self.center.1 += dy / self.zoom;
    }

    pub fn zoom_at(&mut self, wx: f64, wy: f64, factor: f64) {
        if !(wx.is_finite() && wy.is_finite() && factor.is_finite() && factor > 0.0) {
            return;
        }
        let old = self.zoom;
        let new = self.clamp_zoom(old * factor);
        if new == old {
            return;
        }
        let eff = new / old;
        self.zoom = new;
        self.center.0 = wx + (self.center.0 - wx) / eff;
        self.center.1 = wy + (self.center.1 - wy) / eff;
    }

    pub fn at_zoom_in_limit(&self) -> bool {
        self.zoom >= self.clamp_zoom(self.zoom * 1.0001)
    }
    pub fn at_zoom_out_limit(&self) -> bool {
        self.zoom <= self.clamp_zoom(self.zoom * 0.9999)
    }

    pub fn zoom_to_bounds(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            return;
        }
        let w = (x1 - x0).max(1e-9);
        let h = (y1 - y0).max(1e-9);
        let margin = 1.1;
        let zx = self.width / (w * margin);
        let zy = self.height / (h * margin);
        self.zoom = self.clamp_zoom(zx.min(zy));
        self.center = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    }

    pub fn visible_bounds(&self) -> (f64, f64, f64, f64) {
        let hw = self.width / (2.0 * self.zoom);
        let hh = self.height / (2.0 * self.zoom);
        (
            self.center.0 - hw,
            self.center.1 - hh,
            self.center.0 + hw,
            self.center.1 + hh,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let v = ViewTransform::new(800.0, 600.0);
        let (sx, sy) = v.world_to_screen(3.5, -2.0);
        let (wx, wy) = v.screen_to_world(sx, sy);
        assert!((wx - 3.5).abs() < 1e-9 && (wy + 2.0).abs() < 1e-9);
    }

    #[test]
    fn center_is_screen_center() {
        let v = ViewTransform::new(800.0, 600.0);
        let (sx, sy) = v.world_to_screen(0.0, 0.0);
        assert!((sx - 400.0).abs() < 1e-9 && (sy - 300.0).abs() < 1e-9);
    }

    #[test]
    fn zoom_at_keeps_anchor() {
        let mut v = ViewTransform::new(800.0, 600.0);
        let before = v.world_to_screen(2.0, 1.0);
        v.zoom_at(2.0, 1.0, 2.5);
        let after = v.world_to_screen(2.0, 1.0);
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
    }

    #[test]
    fn zoom_is_clamped_to_unit_range() {
        let mut v = ViewTransform::new(800.0, 600.0);
        v.set_visible_range(0.05, 50_000.0);
        let dim = 800.0_f64;
        for _ in 0..200 {
            v.zoom_at(0.0, 0.0, 2.0);
        }
        assert!(
            (v.zoom - dim / 0.05).abs() / (dim / 0.05) < 1e-9,
            "zoom-in not capped: {}",
            v.zoom
        );
        assert!(v.at_zoom_in_limit());
        for _ in 0..400 {
            v.zoom_at(0.0, 0.0, 0.5);
        }
        assert!(
            (v.zoom - dim / 50_000.0).abs() / (dim / 50_000.0) < 1e-9,
            "zoom-out not capped: {}",
            v.zoom
        );
        assert!(v.at_zoom_out_limit());
    }

    #[test]
    fn snap_to_grid_rounds_to_nearest_intersection() {
        let v = ViewTransform::new(800.0, 600.0);
        let g = v.grid_spacing();
        assert!(g > 0.0 && g.is_finite());
        let (sx, sy) = v.snap_to_grid(0.37 * g + 0.001, -1.4 * g);
        assert!((sx / g - (sx / g).round()).abs() < 1e-9);
        assert!((sy / g - (sy / g).round()).abs() < 1e-9);
        assert!((sx - 0.0).abs() <= g * 0.5 + 1e-9);
        assert!((sy - (-g)).abs() <= g * 0.5 + 1e-9);
    }

    #[test]
    fn zoom_to_bounds_frames_box() {
        let mut v = ViewTransform::new(800.0, 600.0);
        v.zoom_to_bounds(0.0, 0.0, 100.0, 50.0);
        assert_eq!(v.center, (50.0, 25.0));
        let (x0, y0, x1, y1) = v.visible_bounds();
        assert!(x0 <= 0.0 && x1 >= 100.0 && y0 <= 0.0 && y1 >= 50.0);
    }

    #[test]
    fn non_finite_input_cannot_poison_the_view() {
        let mut v = ViewTransform::new(800.0, 600.0);
        v.zoom_to_bounds(f64::NAN, 0.0, 100.0, 50.0);
        v.zoom_at(f64::NAN, 1.0, 2.0);
        v.zoom_at(1.0, 1.0, f64::NAN);
        v.zoom_at(1.0, 1.0, 0.0);
        v.pan_pixels(f64::INFINITY, 3.0);
        assert!(
            v.zoom.is_finite()
                && v.zoom > 0.0
                && v.center.0.is_finite()
                && v.center.1.is_finite(),
            "view must ignore hostile input: zoom={} center={:?}",
            v.zoom,
            v.center
        );
    }
}
