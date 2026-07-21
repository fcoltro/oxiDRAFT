//! Per-entity display properties: colour, line weight, line type, and extended
//! (application) data — each supporting the CAD `ByLayer`/`ByBlock`
//! inheritance model.

/// An entity's colour: an explicit RGB, or inherited from its layer/block.
#[derive(Clone, Debug, PartialEq)]
pub enum Color {
    /// Inherit the layer's colour.
    ByLayer,
    /// Inherit the containing block's colour.
    ByBlock,
    /// An explicit red/green/blue colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Maps an AutoCAD Color Index (ACI) to an RGB colour.
    pub fn from_aci(index: u8) -> Color {
        match index {
            1 => Color::Rgb(255, 0, 0),
            2 => Color::Rgb(255, 255, 0),
            3 => Color::Rgb(0, 255, 0),
            4 => Color::Rgb(0, 255, 255),
            5 => Color::Rgb(0, 0, 255),
            6 => Color::Rgb(255, 0, 255),
            7 => Color::Rgb(255, 255, 255),
            _ => Color::Rgb(128, 128, 128),
        }
    }

    /// The concrete RGB, resolving `ByLayer`/`ByBlock` against `layer_color`.
    pub fn resolve(&self, layer_color: (u8, u8, u8)) -> (u8, u8, u8) {
        match self {
            Color::Rgb(r, g, b) => (*r, *g, *b),
            Color::ByLayer | Color::ByBlock => layer_color,
        }
    }
}

/// An entity's line weight (thickness): explicit or inherited.
#[derive(Clone, Debug, PartialEq)]
pub enum LineWeight {
    /// Inherit the layer's line weight.
    ByLayer,
    /// Inherit the containing block's line weight.
    ByBlock,
    /// An explicit weight in hundredths of a millimetre.
    Hundredths(i16),
}

impl LineWeight {
    /// The concrete weight in millimetres, resolving inheritance against
    /// `layer_weight_mm`.
    pub fn to_mm(&self, layer_weight_mm: f64) -> f64 {
        match self {
            LineWeight::Hundredths(h) => *h as f64 / 100.0,
            LineWeight::ByLayer | LineWeight::ByBlock => layer_weight_mm,
        }
    }
}

/// A reference to a line type (dash pattern): explicit, or inherited.
#[derive(Clone, Debug, PartialEq)]
pub enum LineTypeRef {
    /// Inherit the layer's line type.
    ByLayer,
    /// Inherit the containing block's line type.
    ByBlock,
    /// A named line type, defined in the document's line-type table.
    Named(String),
}

/// The definition of a named line type: its dash/gap pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct LineTypeDef {
    /// Unique name.
    pub name: String,
    /// Human-readable description (often an ASCII preview).
    pub description: String,
    /// Dash lengths: positive = drawn dash, negative = gap; empty = solid.
    pub pattern: Vec<f64>,
}

impl LineTypeDef {
    /// The built-in continuous (solid) line type.
    pub fn continuous() -> Self {
        LineTypeDef {
            name: "Continuous".into(),
            description: "Solid line".into(),
            pattern: vec![],
        }
    }
    /// The built-in dashed line type.
    pub fn dashed() -> Self {
        LineTypeDef {
            name: "Dashed".into(),
            description: "__ __ __".into(),
            pattern: vec![0.5, -0.25],
        }
    }
    /// The built-in dotted line type.
    pub fn dotted() -> Self {
        LineTypeDef {
            name: "Dotted".into(),
            description: ". . . .".into(),
            pattern: vec![0.0, -0.2],
        }
    }
    /// The built-in centre-line type (long-short-long dashes).
    pub fn center() -> Self {
        LineTypeDef {
            name: "Center".into(),
            description: "____ _ ____".into(),
            pattern: vec![1.0, -0.25, 0.25, -0.25],
        }
    }
}

/// Extended application data attached to an entity: arbitrary key/value string
/// pairs preserved through the document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XData {
    /// The key/value pairs, in insertion order.
    pub entries: Vec<(String, String)>,
}

impl XData {
    /// The value for `key`, or `None` when absent.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    /// Sets `key` to `value`, overwriting any existing entry.
    pub fn set(&mut self, key: &str, value: &str) {
        if let Some(e) = self.entries.iter_mut().find(|(k, _)| k == key) {
            e.1 = value.to_string();
        } else {
            self.entries.push((key.to_string(), value.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_by_layer_resolves() {
        assert_eq!(Color::ByLayer.resolve((10, 20, 30)), (10, 20, 30));
        assert_eq!(Color::Rgb(1, 2, 3).resolve((10, 20, 30)), (1, 2, 3));
    }

    #[test]
    fn aci_red() {
        assert_eq!(Color::from_aci(1), Color::Rgb(255, 0, 0));
    }

    #[test]
    fn lineweight_mm() {
        assert!((LineWeight::Hundredths(25).to_mm(0.0) - 0.25).abs() < 1e-9);
        assert!((LineWeight::ByLayer.to_mm(0.5) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn xdata_set_get() {
        let mut x = XData::default();
        x.set("part_no", "A123");
        x.set("part_no", "B456");
        assert_eq!(x.get("part_no"), Some("B456"));
        assert_eq!(x.get("missing"), None);
    }
}
