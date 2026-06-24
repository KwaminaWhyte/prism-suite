use super::*;

// --- Animation presets --------------------------------------------------------

/// Build a layer carrying a representative slice of animatable state: every
/// effect stack populated (from each family's `defaults()`), a generate fill +
/// evolution keys, and keyframes/expression across several transform tracks.
fn rigged_layer() -> PulseLayer {
    let mut l = PulseLayer::new("Source", [0.2, 0.4, 0.6, 1.0]);
    l.effects.push(Effect::defaults()[0]);
    l.effects.push(Effect::defaults()[3]);
    l.spatial_effects.push(SpatialEffect::defaults()[0]);
    l.distort_effects.push(DistortEffect::defaults()[1]);
    l.key_effects.push(KeyEffect::defaults()[4]);
    l.stylize_effects.push(StylizeEffect::defaults()[0]);
    l.generate = Some(GenerateEffect::defaults()[0]);
    l.generate_evolution.set_key(0.0, 0.0);
    l.generate_evolution.set_key(5.0, 6.0);
    // Position animation with an ease, plus a rotation expression.
    l.x.set_key(0.0, -100.0);
    l.x.set_key(2.0, 100.0);
    l.x.set_interp(0.0, Interp::Ease(Ease::EASY));
    l.scale.set_key(0.0, 0.5);
    l.scale.set_key(1.0, 1.5);
    l.rotation.expression = Some("value + time * 90.0".to_string());
    l
}

#[test]
fn preset_capture_then_apply_reproduces_state() {
    let src = rigged_layer();
    let preset = AnimationPreset::capture("Move + grade", &src);

    // Apply onto a fresh, empty layer of a different kind/name/color.
    let mut dst = PulseLayer::of_kind(LayerKind::Solid, "Target", [1.0, 1.0, 1.0, 1.0]);
    preset.apply(&mut dst);

    // Effect stacks reproduced exactly.
    assert_eq!(dst.effects, src.effects);
    assert_eq!(dst.spatial_effects, src.spatial_effects);
    assert_eq!(dst.distort_effects, src.distort_effects);
    assert_eq!(dst.key_effects, src.key_effects);
    assert_eq!(dst.stylize_effects, src.stylize_effects);
    assert_eq!(dst.generate, src.generate);
    assert_eq!(dst.generate_evolution, src.generate_evolution);

    // Captured transform tracks reproduced (values, times, easing, expression).
    assert_eq!(dst.x, src.x);
    assert_eq!(dst.scale, src.scale);
    assert_eq!(dst.rotation, src.rotation);
    assert_eq!(dst.rotation.expression.as_deref(), Some("value + time * 90.0"));

    // Identity/wiring fields untouched by the preset.
    assert_eq!(dst.name, "Target");
    assert_eq!(dst.color, [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn preset_skips_empty_tracks_and_apply_leaves_them_untouched() {
    // A preset that only animates Position.
    let mut src = PulseLayer::new("Slide", [0.0; 4]);
    src.x.set_key(0.0, 0.0);
    src.x.set_key(1.0, 50.0);
    let preset = AnimationPreset::capture("Slide", &src);

    // Only the X track was captured (Scale etc. were empty).
    assert_eq!(preset.tracks.len(), 1);
    let pt: &PresetTrack = &preset.tracks[0];
    assert_eq!(pt.prop, PropTag::X);

    // A target with its own Scale animation keeps it after apply (uncaptured
    // property is left untouched), while X is overwritten by the preset.
    let mut dst = PulseLayer::new("Target", [0.0; 4]);
    dst.scale.set_key(0.0, 2.0);
    dst.x.set_key(0.0, 999.0); // pre-existing X, should be replaced
    preset.apply(&mut dst);

    assert_eq!(dst.x, src.x); // X replaced
    assert_eq!(dst.scale.keys.len(), 1); // Scale preserved
    assert_eq!(dst.scale.keys[0].value, 2.0);
}

#[test]
fn preset_apply_replaces_effect_stacks() {
    // Source has one Levels effect; target already has two unrelated effects.
    let mut src = PulseLayer::new("Src", [0.0; 4]);
    src.effects.push(Effect::defaults()[2]);
    let preset = AnimationPreset::capture("Look", &src);

    let mut dst = PulseLayer::new("Dst", [0.0; 4]);
    dst.effects.push(Effect::defaults()[0]);
    dst.effects.push(Effect::defaults()[1]);
    preset.apply(&mut dst);

    // The whole stack is replaced by the captured one (not merged/appended).
    assert_eq!(dst.effects, src.effects);
    assert_eq!(dst.effects.len(), 1);
}

#[test]
fn preset_serde_round_trip() {
    let preset = AnimationPreset::capture("RT", &rigged_layer());
    let json = serde_json::to_string(&preset).unwrap();
    let back: AnimationPreset = serde_json::from_str(&json).unwrap();
    assert_eq!(back, preset);
}

#[test]
fn project_round_trips_presets() {
    let mut project = Project::new();
    project
        .presets
        .push(AnimationPreset::capture("In project", &rigged_layer()));
    let json = serde_json::to_string(&project).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.presets, project.presets);
}

#[test]
fn legacy_project_loads_with_empty_presets() {
    // A project JSON with no `presets` field (a pre-presets `.pulse` file).
    let legacy = r#"{"comps":[{"id":1,"name":"C","width":100,"height":100,
        "duration":1.0,"fps":30.0,"layers":[]}],"active":0,"next_id":2}"#;
    let project: Project = serde_json::from_str(legacy).unwrap();
    assert!(project.presets.is_empty());
}

#[test]
fn project_with_layers_keyframes_and_preset_round_trips_via_bytes() {
    // The acceptance bar for File ▸ Open: a project carrying a rigged layer (with
    // effects + keyframes + an expression) *and* a saved animation preset must
    // survive serialize → deserialize → re-serialize unchanged. `Project`/`Comp`
    // don't derive `PartialEq`, so we compare the canonical JSON of the loaded
    // project against the original's — a byte-exact document round-trip.
    let mut comp = Comp::new();
    comp.id = 1;
    comp.layers.push(rigged_layer());
    let mut project = Project {
        comps: vec![comp],
        active: 0,
        next_id: 2,
        presets: vec![AnimationPreset::capture("Saved", &rigged_layer())],
    };
    // A second comp so the comp tree (not just one comp) round-trips.
    let mut b = Comp::empty_like("B", &Comp::new());
    b.id = 2;
    project.comps.push(b);
    project.next_id = 3;

    let bytes = serde_json::to_vec(&project).unwrap();
    let loaded: Project = serde_json::from_slice(&bytes).expect("deserialize round-trip");

    assert_eq!(loaded.comps.len(), 2);
    assert_eq!(loaded.presets.len(), 1);
    assert_eq!(loaded.next_id, 3);
    assert_eq!(loaded.comps[0].layers[0].x, project.comps[0].layers[0].x);
    assert_eq!(loaded.presets, project.presets);
    // Whole-document equality via canonical serialization (no PartialEq needed).
    assert_eq!(
        serde_json::to_string(&loaded).unwrap(),
        serde_json::to_string(&project).unwrap(),
        "project document round-trips byte-for-byte"
    );
}

#[test]
fn malformed_project_json_returns_err_not_panic() {
    // The Open path must never panic on a bad file — it returns Err and leaves the
    // current project intact. Exercise the same deserialize the loader uses.
    assert!(serde_json::from_slice::<Project>(b"not json at all {{{").is_err());
    assert!(serde_json::from_slice::<Project>(b"").is_err());
    // Structurally valid JSON but the wrong shape (missing required `comps`).
    assert!(serde_json::from_slice::<Project>(br#"{"active":0}"#).is_err());
}

#[test]
fn preset_capture_is_deterministic() {
    let src = rigged_layer();
    let a = AnimationPreset::capture("Same", &src);
    let b = AnimationPreset::capture("Same", &src);
    assert_eq!(a, b);
    // Apply is deterministic too.
    let mut d1 = PulseLayer::new("d", [0.0; 4]);
    let mut d2 = PulseLayer::new("d", [0.0; 4]);
    a.apply(&mut d1);
    a.apply(&mut d2);
    assert_eq!(serde_json::to_string(&d1).unwrap(), serde_json::to_string(&d2).unwrap());
}
