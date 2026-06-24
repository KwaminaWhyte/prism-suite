use super::*;

// --- Layer kinds --------------------------------------------------------

#[test]
fn only_solid_draws_own_pixels() {
    assert!(LayerKind::Solid.draws_own_pixels());
    assert!(!LayerKind::Null.draws_own_pixels());
    assert!(!LayerKind::Adjustment.draws_own_pixels());
}

#[test]
fn null_layer_creation_is_transform_only() {
    // A fresh Null is a real, unparented layer whose transform is live but which
    // draws nothing of its own — usable purely as a parent / pivot handle.
    let null = PulseLayer::of_kind(LayerKind::Null, "Null 1", [0.6, 0.6, 0.6, 1.0]);
    assert_eq!(null.kind, LayerKind::Null);
    assert!(!null.kind.draws_own_pixels());
    assert_eq!(null.parent, None);
    // Its transform is the usual identity-at-default and animatable like any layer.
    assert_eq!(null.transform(0.0).scale, 1.0);
    assert!(null.x.keys.is_empty());
}

#[test]
fn null_layerkind_serde_roundtrips() {
    // The new `Null` variant round-trips, and a pre-Null-variant layer (no `kind`
    // field at all) still deserializes — adding the variant doesn't break old files.
    let null = PulseLayer::of_kind(LayerKind::Null, "N", [0.6, 0.6, 0.6, 1.0]);
    let json = serde_json::to_string(&null).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, LayerKind::Null);

    let old = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(old).unwrap();
    assert_eq!(layer.kind, LayerKind::Solid);
}

#[test]
fn layer_kind_serde_defaults_to_solid() {
    // A pre-kind layer (no `kind`/`effects` fields) loads as a Solid with no
    // effects.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.kind, LayerKind::Solid);
    assert!(layer.effects.is_empty());
}

#[test]
fn generate_serde_defaults_to_none() {
    // A pre-generate layer (no `generate` field) loads with an empty generate slot.
    let json = r#"{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(layer.generate.is_none());
}

/// Pull the `evolution` out of a Fractal-Noise generate (test helper).
fn evo_of(g: GenerateEffect) -> f32 {
    match g {
        GenerateEffect::FractalNoise { evolution, .. } => evolution,
        other => panic!("expected Fractal Noise, got {}", other.label()),
    }
}

#[test]
fn generate_at_uses_static_evolution_when_track_empty() {
    // No evolution keys → generate_at returns the static field unchanged.
    let mut gen = GenerateEffect::defaults()[0];
    if let GenerateEffect::FractalNoise { evolution, .. } = &mut gen {
        *evolution = 3.0;
    }
    let mut layer = PulseLayer::new("L", [1.0; 4]);
    layer.generate = Some(gen);
    assert_eq!(evo_of(layer.generate_at(0.0).unwrap()), 3.0);
    assert_eq!(
        evo_of(layer.generate_at(5.0).unwrap()),
        3.0,
        "static evolution is constant over time"
    );
}

#[test]
fn generate_at_track_overrides_static_evolution() {
    // A keyed evolution track overrides the static field at the sampled time.
    let mut layer = PulseLayer::new("L", [1.0; 4]);
    layer.generate = Some(GenerateEffect::defaults()[0]);
    layer.generate_evolution.set_key(0.0, 0.0);
    layer.generate_evolution.set_key(2.0, 10.0);
    assert!((evo_of(layer.generate_at(0.0).unwrap()) - 0.0).abs() < 1e-5);
    let mid = evo_of(layer.generate_at(1.0).unwrap());
    assert!((mid - 5.0).abs() < 1e-4, "linear interp at midpoint, got {mid}");
    assert!((evo_of(layer.generate_at(2.0).unwrap()) - 10.0).abs() < 1e-4);
}

#[test]
fn generate_at_color_generator_ignores_evolution_track() {
    // A colour generator has no evolution axis, so a keyed evolution track is a
    // no-op for it (the generate is returned unchanged).
    let mut layer = PulseLayer::new("L", [1.0; 4]);
    let ramp = GenerateEffect::defaults()[1];
    layer.generate = Some(ramp);
    layer.generate_evolution.set_key(0.0, 0.0);
    layer.generate_evolution.set_key(2.0, 10.0);
    assert_eq!(layer.generate_at(1.0).unwrap(), ramp);
}

#[test]
fn generate_at_none_without_fill() {
    let layer = PulseLayer::new("L", [1.0; 4]);
    assert!(layer.generate_at(0.0).is_none());
}

#[test]
fn generate_layer_serde_round_trips() {
    // A layer with a Fractal Noise generate fill round-trips through serde.
    let mut layer = PulseLayer::new("L", [1.0, 1.0, 1.0, 1.0]);
    layer.generate = Some(GenerateEffect::FractalNoise {
        fractal_type: FractalType::Turbulent,
        contrast: 1.5,
        brightness: 0.1,
        scale: 64.0,
        scale_x: 1.2,
        scale_y: 0.8,
        complexity: 4,
        sub_influence: 0.5,
        sub_scaling: 2.5,
        evolution: 3.0,
        seed: 99,
        overflow: Overflow::Wrap,
        opacity: 0.7,
    });
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(layer.generate, back.generate);
}
