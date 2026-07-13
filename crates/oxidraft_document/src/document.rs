use crate::constraint::{ConstraintKind, SketchConstraint};
use crate::entity::{Entity, EntityId, EntityKind};
use crate::layer::LayerTable;
use crate::properties::LineTypeDef;
use oxidraft_geometry::{BoundingBox, Point2d};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Units {
    Unitless,
    Millimeters,
    Centimeters,
    Meters,
    Kilometers,
    Inches,
    Feet,
}

impl Units {
    pub fn short_name(self) -> &'static str {
        match self {
            Units::Unitless => "",
            Units::Millimeters => "mm",
            Units::Centimeters => "cm",
            Units::Meters => "m",
            Units::Kilometers => "km",
            Units::Inches => "in",
            Units::Feet => "ft",
        }
    }

    pub fn format_measure(self, value: f64, precision: usize) -> String {
        let s = self.short_name();
        if s.is_empty() {
            format!("{value:.*}", precision)
        } else {
            format!("{value:.*} {s}", precision)
        }
    }

    pub fn visible_range(self) -> (f64, f64) {
        match self {
            Units::Millimeters => (0.05, 50_000.0),
            Units::Centimeters => (0.01, 100_000.0),
            Units::Meters => (0.001, 100_000.0),
            Units::Kilometers => (0.0001, 50_000.0),
            Units::Inches => (0.001, 100_000.0),
            Units::Feet => (0.001, 100_000.0),
            Units::Unitless => (0.001, 1_000_000.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub name: String,
    pub base_point: Point2d,
    pub entities: Vec<Entity>,
}

#[derive(Clone, Debug)]
pub struct NamedView {
    pub name: String,
    pub center: (f64, f64),
    pub zoom: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DimStyle {
    pub text_height: f64,
    pub arrow_size: f64,
    pub font: Option<String>,
    pub precision: usize,
}

impl Default for DimStyle {
    fn default() -> Self {
        DimStyle {
            text_height: 1.0,
            arrow_size: 1.0,
            font: None,
            precision: 2,
        }
    }
}

pub const DIMENSION_LAYER: &str = "Dimensions";

#[derive(Clone, Debug)]
pub struct Settings {
    pub units: Units,
    pub grid_spacing: f64,
    pub snap_spacing: f64,
    pub dim_style: DimStyle,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            units: Units::Millimeters,
            grid_spacing: 10.0,
            snap_spacing: 1.0,
            dim_style: DimStyle::default(),
        }
    }
}

#[derive(Clone)]
pub struct Document {
    pub layers: LayerTable,
    pub line_types: Vec<LineTypeDef>,
    pub blocks: HashMap<String, Block>,
    pub entities: HashMap<EntityId, Entity>,
    pub order: Vec<EntityId>,
    pub views: Vec<NamedView>,
    pub settings: Settings,
    pub constraints: Vec<SketchConstraint>,
    next_id: u64,
}

impl Default for Document {
    fn default() -> Self {
        Document {
            layers: LayerTable::default(),
            line_types: vec![
                LineTypeDef::continuous(),
                LineTypeDef::dashed(),
                LineTypeDef::dotted(),
                LineTypeDef::center(),
            ],
            blocks: HashMap::new(),
            entities: HashMap::new(),
            order: Vec::new(),
            views: Vec::new(),
            settings: Settings::default(),
            constraints: Vec::new(),
            next_id: 1,
        }
    }
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    fn alloc_id(&mut self) -> EntityId {
        let id = EntityId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add(&mut self, kind: EntityKind) -> EntityId {
        let layer = self.layers.current;
        self.add_on_layer(kind, layer)
    }

    pub fn add_on_layer(&mut self, kind: EntityKind, layer: usize) -> EntityId {
        let id = self.alloc_id();
        let entity = Entity::new(id, kind, layer);
        self.entities.insert(id, entity);
        self.order.push(id);
        id
    }

    pub fn add_entity(&mut self, mut entity: Entity) -> EntityId {
        let id = self.alloc_id();
        entity.id = id;
        self.entities.insert(id, entity);
        self.order.push(id);
        id
    }

    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        self.entities.get(&id)
    }
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    pub fn remove(&mut self, id: EntityId) -> Option<Entity> {
        self.order.retain(|&e| e != id);
        self.constraints.retain(|c| !c.references(id));
        self.entities.remove(&id)
    }

    /// Records the constraint unless an equivalent one already exists.
    /// Re-adding an existing valued constraint (Radius) with a different
    /// value retargets it in place. Returns whether the document changed.
    pub fn add_constraint(&mut self, mut c: SketchConstraint) -> bool {
        // The (0, 180] angle invariant is enforced here, the one gate every
        // record passes through (interactive command, file load, retarget),
        // so downstream readers — the solver lowering, the dimension badge —
        // can trust it without re-folding.
        if c.kind == ConstraintKind::Angle
            && let Some(v) = c.val
        {
            c.val = Some(crate::constraint::normalize_angle_deg(v));
        }
        if let Some(existing) = self.constraints.iter_mut().find(|e| e.same_relation(&c)) {
            let mut changed = false;
            if existing.val != c.val {
                existing.val = c.val;
                changed = true;
            }
            // A record carrying a placement moves the annotation; one
            // without (solver retargets, file loads) leaves it where the
            // user put it.
            if c.place.is_some() && existing.place != c.place {
                existing.place = c.place;
                changed = true;
            }
            return changed;
        }
        self.constraints.push(c);
        true
    }

    pub fn constraints_on(&self, id: EntityId) -> impl Iterator<Item = &SketchConstraint> {
        self.constraints.iter().filter(move |c| c.references(id))
    }

    pub fn remove_constraints_on(&mut self, id: EntityId) -> usize {
        let before = self.constraints.len();
        self.constraints.retain(|c| !c.references(id));
        before - self.constraints.len()
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &Entity> {
        self.order
            .iter()
            .filter_map(move |id| self.entities.get(id))
    }

    pub fn editable_entities(&self) -> impl DoubleEndedIterator<Item = &Entity> {
        self.iter().filter(move |e| {
            self.layers
                .get(e.layer)
                .map(|l| l.is_editable())
                .unwrap_or(false)
        })
    }

    pub fn extents(&self) -> Option<BoundingBox> {
        let mut acc: Option<BoundingBox> = None;
        for e in self.iter() {
            if let Some(bb) = e.bounding_box() {
                acc = Some(match acc {
                    Some(a) => a.union(&bb),
                    None => bb,
                });
            }
        }
        acc
    }

    pub fn define_block(&mut self, block: Block) {
        self.blocks.insert(block.name.clone(), block);
    }

    pub fn explode_insert(&self, insert: &Entity) -> Vec<Entity> {
        if let EntityKind::Insert { block, transform } = &insert.kind
            && let Some(b) = self.blocks.get(block)
        {
            return b
                .entities
                .iter()
                .map(|e| {
                    let mut copy = e.clone();
                    copy.transform(transform);
                    copy.layer = insert.layer;
                    copy
                })
                .collect();
        }
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxidraft_geometry::{Curve, LineSeg, Transform2d};

    fn pt(x: i64, y: i64) -> Point2d {
        Point2d::from_i64(x, y)
    }
    fn line(x0: i64, y0: i64, x1: i64, y1: i64) -> EntityKind {
        EntityKind::Curve(Curve::Line(LineSeg::from_endpoints(pt(x0, y0), pt(x1, y1))))
    }

    #[test]
    fn add_remove_entities() {
        let mut doc = Document::new();
        let a = doc.add(line(0, 0, 1, 1));
        let b = doc.add(line(1, 1, 2, 2));
        assert_eq!(doc.len(), 2);
        doc.remove(a);
        assert_eq!(doc.len(), 1);
        assert!(doc.get(a).is_none());
        assert!(doc.get(b).is_some());
    }

    #[test]
    fn insertion_order_preserved() {
        let mut doc = Document::new();
        let ids: Vec<_> = (0..5).map(|i| doc.add(line(i, 0, i + 1, 0))).collect();
        let seen: Vec<_> = doc.iter().map(|e| e.id).collect();
        assert_eq!(seen, ids);
    }

    #[test]
    fn extents_covers_all() {
        let mut doc = Document::new();
        doc.add(line(0, 0, 2, 2));
        doc.add(line(5, 5, 8, 1));
        let bb = doc.extents().unwrap();
        assert_eq!(bb.min, pt(0, 0));
        assert_eq!(bb.max, pt(8, 5));
    }

    #[test]
    fn block_insert_explodes_to_world() {
        let mut doc = Document::new();
        doc.define_block(Block {
            name: "tick".into(),
            base_point: pt(0, 0),
            entities: vec![Entity::new(EntityId(0), line(0, 0, 1, 0), 0)],
        });
        let insert = doc.add(EntityKind::Insert {
            block: "tick".into(),
            transform: Transform2d::translation(10.0, 10.0),
        });
        let exploded = doc.explode_insert(doc.get(insert).unwrap());
        assert_eq!(exploded.len(), 1);
        if let Curve::Line(l) = exploded[0].as_curve().unwrap() {
            assert_eq!(l.p0, pt(10, 10));
            assert_eq!(l.p1, pt(11, 10));
        } else {
            panic!()
        }
    }
}
