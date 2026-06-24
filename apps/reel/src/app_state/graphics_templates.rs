//! **Essential Graphics / Motion Graphics Templates.**
//!
//! A title/graphics *template* model in the spirit of Premiere's Essential
//! Graphics panel + `.mogrt` files. A [`GraphicsTemplate`] is a small layer
//! tree — text layers and shape layers — plus a set of **exposed editable
//! properties** that map onto layer fields. The template author decides which
//! fields are editable (the "Essential Properties"); the editor only mutates
//! those. A template **instantiates** into a [`GraphicsInstance`] (a concrete
//! graphics clip placed on the timeline at a `start`/`duration`), carrying its
//! own copy of the exposed-property *values* so two instances of the same
//! template can differ.
//!
//! Everything here is a pure data model + deterministic logic (no rendering),
//! so it is fully unit-testable.

use super::{App, Action};

// ============================================================================
// Layer model
// ============================================================================

/// A text layer inside a graphics template.
#[derive(Clone, Debug, PartialEq)]
pub struct GtTextLayer {
    pub name: String,
    pub text: String,
    pub font_family: String,
    pub font_size: f32,
    pub color: [f32; 4],
    /// Position in composition pixels (top-left origin).
    pub x: f32,
    pub y: f32,
    pub bold: bool,
    pub italic: bool,
}

impl Default for GtTextLayer {
    fn default() -> Self {
        Self {
            name: "Text".to_string(),
            text: "New Text".to_string(),
            font_family: "Arial".to_string(),
            font_size: 72.0,
            color: [1.0, 1.0, 1.0, 1.0],
            x: 0.0,
            y: 0.0,
            bold: false,
            italic: false,
        }
    }
}

/// Shape kind for a shape layer.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum GtShapeKind {
    #[default]
    Rectangle,
    Ellipse,
    RoundedRect,
}

/// A shape layer (background bar, accent rule, etc.).
#[derive(Clone, Debug, PartialEq)]
pub struct GtShapeLayer {
    pub name: String,
    pub kind: GtShapeKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub fill: [f32; 4],
    pub corner_radius: f32,
}

impl Default for GtShapeLayer {
    fn default() -> Self {
        Self {
            name: "Shape".to_string(),
            kind: GtShapeKind::Rectangle,
            x: 0.0,
            y: 0.0,
            width: 400.0,
            height: 100.0,
            fill: [0.0, 0.0, 0.0, 0.6],
            corner_radius: 0.0,
        }
    }
}

/// One layer in a template's layer stack (drawn back-to-front).
#[derive(Clone, Debug, PartialEq)]
pub enum GtLayer {
    Text(GtTextLayer),
    Shape(GtShapeLayer),
}

impl GtLayer {
    pub fn name(&self) -> &str {
        match self {
            GtLayer::Text(t) => &t.name,
            GtLayer::Shape(s) => &s.name,
        }
    }
}

// ============================================================================
// Exposed editable properties
// ============================================================================

/// What kind of value an exposed property holds (drives the editor control).
#[derive(Clone, Debug, PartialEq)]
pub enum GtPropValue {
    Text(String),
    Color([f32; 4]),
    Number(f32),
    Boolean(bool),
}

/// Which layer field an exposed property targets. Instantiation/editing writes
/// the property's value back into this field on the live layer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GtPropTarget {
    /// `text` of the text layer at `layer_idx`.
    TextContent { layer_idx: usize },
    /// `font_size` of the text layer at `layer_idx`.
    FontSize { layer_idx: usize },
    /// `color` of the text layer at `layer_idx`.
    TextColor { layer_idx: usize },
    /// `fill` of the shape layer at `layer_idx`.
    ShapeFill { layer_idx: usize },
}

/// An exposed ("Essential") editable property: a user-facing `label`, the
/// `target` field it maps to, and its current `value`.
#[derive(Clone, Debug, PartialEq)]
pub struct GtExposedProp {
    pub label: String,
    pub target: GtPropTarget,
    pub value: GtPropValue,
}

// ============================================================================
// Template + instance
// ============================================================================

/// A motion-graphics template: a layer stack plus the subset of fields the
/// author chose to expose as editable Essential Properties.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicsTemplate {
    pub name: String,
    pub layers: Vec<GtLayer>,
    pub exposed: Vec<GtExposedProp>,
    /// Default on-timeline duration (seconds) when instantiated.
    pub default_duration: f32,
}

impl GraphicsTemplate {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), layers: Vec::new(), exposed: Vec::new(), default_duration: 5.0 }
    }

    /// A ready-made "Lower Third" template: a background bar + a title line, with
    /// the title text, size, color, and the bar's fill exposed as editable
    /// properties.
    pub fn lower_third() -> Self {
        let mut t = Self::new("Lower Third");
        t.default_duration = 5.0;
        // Layer 0: background bar (shape).
        t.layers.push(GtLayer::Shape(GtShapeLayer {
            name: "Bar".to_string(),
            kind: GtShapeKind::Rectangle,
            x: 100.0,
            y: 800.0,
            width: 700.0,
            height: 120.0,
            fill: [0.05, 0.05, 0.05, 0.75],
            corner_radius: 0.0,
        }));
        // Layer 1: title text.
        t.layers.push(GtLayer::Text(GtTextLayer {
            name: "Title".to_string(),
            text: "Name".to_string(),
            font_family: "Arial".to_string(),
            font_size: 64.0,
            color: [1.0, 1.0, 1.0, 1.0],
            x: 130.0,
            y: 820.0,
            bold: true,
            italic: false,
        }));
        // Exposed Essential Properties.
        t.exposed.push(GtExposedProp {
            label: "Title Text".to_string(),
            target: GtPropTarget::TextContent { layer_idx: 1 },
            value: GtPropValue::Text("Name".to_string()),
        });
        t.exposed.push(GtExposedProp {
            label: "Title Size".to_string(),
            target: GtPropTarget::FontSize { layer_idx: 1 },
            value: GtPropValue::Number(64.0),
        });
        t.exposed.push(GtExposedProp {
            label: "Title Color".to_string(),
            target: GtPropTarget::TextColor { layer_idx: 1 },
            value: GtPropValue::Color([1.0, 1.0, 1.0, 1.0]),
        });
        t.exposed.push(GtExposedProp {
            label: "Bar Color".to_string(),
            target: GtPropTarget::ShapeFill { layer_idx: 0 },
            value: GtPropValue::Color([0.05, 0.05, 0.05, 0.75]),
        });
        t
    }

    /// Expose a layer field as an editable property, seeding its value from the
    /// targeted layer's current field. Returns the index of the new property, or
    /// `None` when the target layer/field doesn't exist or has the wrong type.
    pub fn expose(&mut self, label: impl Into<String>, target: GtPropTarget) -> Option<usize> {
        let value = self.read_target(target)?;
        self.exposed.push(GtExposedProp { label: label.into(), target, value });
        Some(self.exposed.len() - 1)
    }

    /// Read the current value of a property's `target` field off the live layers.
    fn read_target(&self, target: GtPropTarget) -> Option<GtPropValue> {
        match target {
            GtPropTarget::TextContent { layer_idx } => match self.layers.get(layer_idx)? {
                GtLayer::Text(t) => Some(GtPropValue::Text(t.text.clone())),
                _ => None,
            },
            GtPropTarget::FontSize { layer_idx } => match self.layers.get(layer_idx)? {
                GtLayer::Text(t) => Some(GtPropValue::Number(t.font_size)),
                _ => None,
            },
            GtPropTarget::TextColor { layer_idx } => match self.layers.get(layer_idx)? {
                GtLayer::Text(t) => Some(GtPropValue::Color(t.color)),
                _ => None,
            },
            GtPropTarget::ShapeFill { layer_idx } => match self.layers.get(layer_idx)? {
                GtLayer::Shape(s) => Some(GtPropValue::Color(s.fill)),
                _ => None,
            },
        }
    }

    /// Instantiate this template at `start` for `duration` seconds, baking the
    /// exposed-property *values* into a fresh copy of the layer stack. The result
    /// is a concrete graphics clip ready to drop on the timeline.
    pub fn instantiate(&self, start: f32, duration: f32) -> GraphicsInstance {
        let mut layers = self.layers.clone();
        // Apply each exposed property's value onto the cloned layers.
        for prop in &self.exposed {
            apply_prop(&mut layers, prop.target, &prop.value);
        }
        GraphicsInstance {
            template_name: self.name.clone(),
            layers,
            values: self
                .exposed
                .iter()
                .map(|p| (p.label.clone(), p.target, p.value.clone()))
                .collect(),
            start: start.max(0.0),
            duration: duration.max(super::MIN_DUR),
        }
    }
}

/// Write one exposed-property value onto the matching layer field. Wrong-type or
/// out-of-range targets are ignored.
fn apply_prop(layers: &mut [GtLayer], target: GtPropTarget, value: &GtPropValue) {
    match (target, value) {
        (GtPropTarget::TextContent { layer_idx }, GtPropValue::Text(s)) => {
            if let Some(GtLayer::Text(t)) = layers.get_mut(layer_idx) { t.text = s.clone(); }
        }
        (GtPropTarget::FontSize { layer_idx }, GtPropValue::Number(v)) => {
            if let Some(GtLayer::Text(t)) = layers.get_mut(layer_idx) { t.font_size = v.clamp(1.0, 999.0); }
        }
        (GtPropTarget::TextColor { layer_idx }, GtPropValue::Color(c)) => {
            if let Some(GtLayer::Text(t)) = layers.get_mut(layer_idx) { t.color = *c; }
        }
        (GtPropTarget::ShapeFill { layer_idx }, GtPropValue::Color(c)) => {
            if let Some(GtLayer::Shape(s)) = layers.get_mut(layer_idx) { s.fill = *c; }
        }
        _ => {}
    }
}

/// A concrete graphics clip created from a template: the baked layer stack, the
/// per-instance editable values (`label`, `target`, `value`), and its timeline
/// placement.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphicsInstance {
    pub template_name: String,
    pub layers: Vec<GtLayer>,
    pub values: Vec<(String, GtPropTarget, GtPropValue)>,
    pub start: f32,
    pub duration: f32,
}

impl GraphicsInstance {
    pub fn end(&self) -> f32 {
        self.start + self.duration
    }

    /// Set the value of the editable property `label` on this instance and re-bake
    /// it onto the baked layers. Returns `true` if a property with that label
    /// existed.
    pub fn set_value(&mut self, label: &str, value: GtPropValue) -> bool {
        let Some(vi) = self.values.iter().position(|(l, _, _)| l == label) else {
            return false;
        };
        let target = self.values[vi].1;
        self.values[vi].2 = value.clone();
        apply_prop(&mut self.layers, target, &value);
        true
    }
}

// ============================================================================
// App integration
// ============================================================================

impl App {
    /// Apply the graphics-template actions. Routed from `mod.rs`.
    pub(crate) fn apply_graphics_templates(&mut self, action: Action) {
        match action {
            Action::AddGraphicsTemplate(t) => {
                self.graphics_templates.push(t);
            }
            Action::RemoveGraphicsTemplate(idx) => {
                if idx < self.graphics_templates.len() {
                    self.graphics_templates.remove(idx);
                }
            }
            Action::ExposeTemplateProp { template_idx, label, target } => {
                if let Some(t) = self.graphics_templates.get_mut(template_idx) {
                    t.expose(label, target);
                }
            }
            Action::SetTemplatePropValue { template_idx, prop_idx, value } => {
                if let Some(t) = self.graphics_templates.get_mut(template_idx) {
                    if let Some(p) = t.exposed.get_mut(prop_idx) {
                        p.value = value;
                    }
                }
            }
            Action::InstantiateGraphicsTemplate { template_idx, start, duration } => {
                if let Some(t) = self.graphics_templates.get(template_idx) {
                    let inst = t.instantiate(start, duration);
                    self.graphics_instances.push(inst);
                    self.host.mark_dirty();
                }
            }
            Action::SetGraphicsInstanceValue { instance_idx, label, value } => {
                if let Some(inst) = self.graphics_instances.get_mut(instance_idx) {
                    inst.set_value(&label, value);
                    self.host.mark_dirty();
                }
            }
            Action::RemoveGraphicsInstance(idx) => {
                if idx < self.graphics_instances.len() {
                    self.graphics_instances.remove(idx);
                    self.host.mark_dirty();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn lower_third_exposes_four_props() {
        let t = GraphicsTemplate::lower_third();
        assert_eq!(t.layers.len(), 2);
        assert_eq!(t.exposed.len(), 4);
        // The exposed text matches the title layer's current text.
        let p = &t.exposed[0];
        assert_eq!(p.label, "Title Text");
        assert!(matches!(p.target, GtPropTarget::TextContent { layer_idx: 1 }));
        assert_eq!(p.value, GtPropValue::Text("Name".to_string()));
    }

    #[test]
    fn expose_seeds_from_layer_and_rejects_wrong_type() {
        let mut t = GraphicsTemplate::new("T");
        t.layers.push(GtLayer::Text(GtTextLayer { font_size: 48.0, ..Default::default() }));
        // Expose the font size → seeded from the live field.
        let idx = t.expose("Size", GtPropTarget::FontSize { layer_idx: 0 }).unwrap();
        assert_eq!(t.exposed[idx].value, GtPropValue::Number(48.0));
        // A shape-fill target over a text layer is rejected.
        assert!(t.expose("Bad", GtPropTarget::ShapeFill { layer_idx: 0 }).is_none());
        // An out-of-range layer is rejected.
        assert!(t.expose("Bad2", GtPropTarget::TextContent { layer_idx: 9 }).is_none());
    }

    #[test]
    fn instantiate_bakes_exposed_values_into_layers() {
        let mut t = GraphicsTemplate::lower_third();
        // Author edits the exposed title text + size before instantiating.
        t.exposed[0].value = GtPropValue::Text("Jane Doe".to_string());
        t.exposed[1].value = GtPropValue::Number(80.0);
        let inst = t.instantiate(3.0, 6.0);
        assert_eq!(inst.template_name, "Lower Third");
        assert_eq!(inst.start, 3.0);
        assert_eq!(inst.duration, 6.0);
        assert_eq!(inst.end(), 9.0);
        // The baked text layer carries the edited values.
        match &inst.layers[1] {
            GtLayer::Text(txt) => {
                assert_eq!(txt.text, "Jane Doe");
                assert_eq!(txt.font_size, 80.0);
            }
            _ => panic!("layer 1 should be text"),
        }
        // The instance also records the per-instance values.
        assert_eq!(inst.values.len(), 4);
    }

    #[test]
    fn instantiate_clamps_duration_and_start() {
        let t = GraphicsTemplate::lower_third();
        let inst = t.instantiate(-5.0, 0.0);
        assert_eq!(inst.start, 0.0);
        assert!(inst.duration >= super::super::MIN_DUR);
    }

    #[test]
    fn font_size_prop_is_clamped_when_baked() {
        let mut t = GraphicsTemplate::lower_third();
        t.exposed[1].value = GtPropValue::Number(5000.0);
        let inst = t.instantiate(0.0, 5.0);
        match &inst.layers[1] {
            GtLayer::Text(txt) => assert_eq!(txt.font_size, 999.0),
            _ => panic!(),
        }
    }

    #[test]
    fn instance_set_value_rebakes_layer() {
        let t = GraphicsTemplate::lower_third();
        let mut inst = t.instantiate(0.0, 5.0);
        assert!(inst.set_value("Title Text", GtPropValue::Text("Edited".to_string())));
        match &inst.layers[1] {
            GtLayer::Text(txt) => assert_eq!(txt.text, "Edited"),
            _ => panic!(),
        }
        // Unknown label → false, no change.
        assert!(!inst.set_value("Nope", GtPropValue::Number(1.0)));
    }

    #[test]
    fn action_add_and_instantiate_into_timeline() {
        let mut app = App::new();
        assert_eq!(app.graphics_templates.len(), 0);
        app.apply(Action::AddGraphicsTemplate(GraphicsTemplate::lower_third()));
        assert_eq!(app.graphics_templates.len(), 1);
        // Edit the exposed title text via an action.
        app.apply(Action::SetTemplatePropValue {
            template_idx: 0,
            prop_idx: 0,
            value: GtPropValue::Text("Hello".to_string()),
        });
        // Instantiate into the timeline graphics clip list.
        assert_eq!(app.graphics_instances.len(), 0);
        app.apply(Action::InstantiateGraphicsTemplate { template_idx: 0, start: 1.0, duration: 4.0 });
        assert_eq!(app.graphics_instances.len(), 1);
        let inst = &app.graphics_instances[0];
        assert_eq!(inst.start, 1.0);
        match &inst.layers[1] {
            GtLayer::Text(txt) => assert_eq!(txt.text, "Hello"),
            _ => panic!(),
        }
    }

    #[test]
    fn action_expose_prop_and_remove() {
        let mut app = App::new();
        let mut t = GraphicsTemplate::new("Custom");
        t.layers.push(GtLayer::Shape(GtShapeLayer::default()));
        app.apply(Action::AddGraphicsTemplate(t));
        app.apply(Action::ExposeTemplateProp {
            template_idx: 0,
            label: "Fill".to_string(),
            target: GtPropTarget::ShapeFill { layer_idx: 0 },
        });
        assert_eq!(app.graphics_templates[0].exposed.len(), 1);
        // Remove the template.
        app.apply(Action::RemoveGraphicsTemplate(0));
        assert_eq!(app.graphics_templates.len(), 0);
        // Out-of-range remove is a no-op.
        app.apply(Action::RemoveGraphicsTemplate(99));
    }

    #[test]
    fn action_set_instance_value() {
        let mut app = App::new();
        app.apply(Action::AddGraphicsTemplate(GraphicsTemplate::lower_third()));
        app.apply(Action::InstantiateGraphicsTemplate { template_idx: 0, start: 0.0, duration: 5.0 });
        app.apply(Action::SetGraphicsInstanceValue {
            instance_idx: 0,
            label: "Title Text".to_string(),
            value: GtPropValue::Text("Updated".to_string()),
        });
        match &app.graphics_instances[0].layers[1] {
            GtLayer::Text(txt) => assert_eq!(txt.text, "Updated"),
            _ => panic!(),
        }
    }

    #[test]
    fn remove_graphics_instance() {
        let mut app = App::new();
        app.apply(Action::AddGraphicsTemplate(GraphicsTemplate::lower_third()));
        app.apply(Action::InstantiateGraphicsTemplate { template_idx: 0, start: 0.0, duration: 5.0 });
        app.apply(Action::InstantiateGraphicsTemplate { template_idx: 0, start: 5.0, duration: 5.0 });
        assert_eq!(app.graphics_instances.len(), 2);
        app.apply(Action::RemoveGraphicsInstance(0));
        assert_eq!(app.graphics_instances.len(), 1);
        assert_eq!(app.graphics_instances[0].start, 5.0);
    }
}
