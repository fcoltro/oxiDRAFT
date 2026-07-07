#[derive(Clone, Debug, PartialEq)]
pub enum Color {
    ByLayer,
    ByBlock,
    Rgb(u8, u8, u8),
}

impl Color {
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

    pub fn resolve(&self, layer_color: (u8, u8, u8)) -> (u8, u8, u8) {
        match self {
            Color::Rgb(r, g, b) => (*r, *g, *b),
            Color::ByLayer | Color::ByBlock => layer_color,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineWeight {
    ByLayer,
    ByBlock,
    Hundredths(i16),
}

impl LineWeight {
    pub fn to_mm(&self, layer_weight_mm: f64) -> f64 {
        match self {
            LineWeight::Hundredths(h) => *h as f64 / 100.0,
            LineWeight::ByLayer | LineWeight::ByBlock => layer_weight_mm,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LineTypeRef {
    ByLayer,
    ByBlock,
    Named(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LineTypeDef {
    pub name: String,
    pub description: String,
    pub pattern: Vec<f64>,
}

impl LineTypeDef {
    pub fn continuous() -> Self {
        LineTypeDef {
            name: "Continuous".into(),
            description: "Solid line".into(),
            pattern: vec![],
        }
    }
    pub fn dashed() -> Self {
        LineTypeDef {
            name: "Dashed".into(),
            description: "__ __ __".into(),
            pattern: vec![0.5, -0.25],
        }
    }
    pub fn dotted() -> Self {
        LineTypeDef {
            name: "Dotted".into(),
            description: ". . . .".into(),
            pattern: vec![0.0, -0.2],
        }
    }
    pub fn center() -> Self {
        LineTypeDef {
            name: "Center".into(),
            description: "____ _ ____".into(),
            pattern: vec![1.0, -0.25, 0.25, -0.25],
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct XData {
    pub entries: Vec<(String, String)>,
}

impl XData {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
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
