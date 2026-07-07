use crate::properties::LineTypeRef;

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub color: (u8, u8, u8),
    pub line_type: LineTypeRef,
    pub line_weight_mm: f64,
    pub on: bool,
    pub frozen: bool,
    pub locked: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Layer {
            name: name.into(),
            color: (255, 255, 255),
            line_type: LineTypeRef::Named("Continuous".into()),
            line_weight_mm: 0.0,
            on: true,
            frozen: false,
            locked: false,
        }
    }

    pub fn with_color(mut self, r: u8, g: u8, b: u8) -> Self {
        self.color = (r, g, b);
        self
    }

    pub fn with_line_type(mut self, name: impl Into<String>) -> Self {
        self.line_type = LineTypeRef::Named(name.into());
        self
    }

    pub fn is_visible(&self) -> bool {
        self.on && !self.frozen
    }

    pub fn is_editable(&self) -> bool {
        self.is_visible() && !self.locked
    }
}

#[derive(Clone, Debug)]
pub struct LayerTable {
    pub layers: Vec<Layer>,
    pub current: usize,
}

impl Default for LayerTable {
    fn default() -> Self {
        LayerTable {
            layers: vec![Layer::new("0")],
            current: 0,
        }
    }
}

impl LayerTable {
    pub fn add(&mut self, layer: Layer) -> usize {
        if let Some(i) = self.index_of(&layer.name) {
            return i;
        }
        self.layers.push(layer);
        self.layers.len() - 1
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.layers.iter().position(|l| l.name == name)
    }

    pub fn get(&self, idx: usize) -> Option<&Layer> {
        self.layers.get(idx)
    }
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut Layer> {
        self.layers.get_mut(idx)
    }

    pub fn current_layer(&self) -> &Layer {
        &self.layers[self.current]
    }

    pub fn set_current(&mut self, name: &str) -> bool {
        if let Some(i) = self.index_of(name) {
            self.current = i;
            true
        } else {
            false
        }
    }

    pub fn delete(&mut self, name: &str) -> Result<(), &'static str> {
        if name == "0" {
            return Err("cannot delete layer 0");
        }
        let idx = self.index_of(name).ok_or("layer not found")?;
        if idx == self.current {
            return Err("cannot delete the current layer");
        }
        self.layers.remove(idx);
        if self.current > idx {
            self.current -= 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_has_layer_zero() {
        let lt = LayerTable::default();
        assert_eq!(lt.layers.len(), 1);
        assert_eq!(lt.current_layer().name, "0");
    }

    #[test]
    fn add_and_set_current() {
        let mut lt = LayerTable::default();
        lt.add(Layer::new("walls").with_color(255, 0, 0));
        assert!(lt.set_current("walls"));
        assert_eq!(lt.current_layer().name, "walls");
        assert_eq!(lt.current_layer().color, (255, 0, 0));
    }

    #[test]
    fn duplicate_name_returns_existing() {
        let mut lt = LayerTable::default();
        let a = lt.add(Layer::new("x"));
        let b = lt.add(Layer::new("x"));
        assert_eq!(a, b);
        assert_eq!(lt.layers.len(), 2);
    }

    #[test]
    fn cannot_delete_zero_or_current() {
        let mut lt = LayerTable::default();
        lt.add(Layer::new("temp"));
        lt.set_current("temp");
        assert!(lt.delete("0").is_err());
        assert!(lt.delete("temp").is_err());
        lt.set_current("0");
        assert!(lt.delete("temp").is_ok());
    }

    #[test]
    fn visibility_and_editability() {
        let mut l = Layer::new("test");
        assert!(l.is_visible() && l.is_editable());
        l.locked = true;
        assert!(l.is_visible() && !l.is_editable());
        l.frozen = true;
        assert!(!l.is_visible() && !l.is_editable());
    }
}
