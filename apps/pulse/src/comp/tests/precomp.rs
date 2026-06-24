use super::*;

// --- Precomps (nested compositions) -------------------------------------

#[test]
fn precomp_layer_serde_round_trips() {
    // A precomp layer (target comp id + time offset) survives a JSON round-trip,
    // including the new LayerKind::Precomp variant and its PrecompLayer block.
    let mut layer = PulseLayer::of_kind(LayerKind::Precomp, "Nested", [0.5, 0.5, 0.5, 1.0]);
    layer.precomp = PrecompLayer {
        source: Some(7),
        time_offset: -0.5,
    };
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(back.kind, LayerKind::Precomp);
    assert!(back.has_precomp());
    assert_eq!(back.precomp.source, Some(7));
    assert!((back.precomp.time_offset - (-0.5)).abs() < 1e-6);
}

#[test]
fn precomp_serde_defaults_for_old_files() {
    // A layer block from a pre-precomp `.pulse` (no `precomp` field, no `kind`)
    // loads as a solid with an unwired precomp (source None, offset 0).
    let json = r#"{"name":"L","color":[1.0,0.0,0.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert_eq!(layer.kind, LayerKind::Solid);
    assert_eq!(layer.precomp.source, None);
    assert_eq!(layer.precomp.time_offset, 0.0);
    assert!(!layer.has_precomp());
}

#[test]
fn old_single_comp_json_loads_as_comp() {
    // An old single-comp `.pulse` (a bare Comp, no `id`/`name`) still deserializes
    // into a Comp directly, with id/name serde-defaulted.
    let json = r#"{"width":640,"height":480,"duration":2.0,"fps":24.0,"layers":[]}"#;
    let comp: Comp = serde_json::from_str(json).unwrap();
    assert_eq!(comp.id, 0);
    assert!(comp.name.is_empty());
    assert_eq!(comp.width, 640);
    assert_eq!(comp.fps, 24.0);
    // And wraps cleanly into a one-comp project with a minted id.
    let project = Project::from_comp(comp);
    assert_eq!(project.comps.len(), 1);
    assert_eq!(project.comps[0].id, 1, "from_comp mints an id for an id-less comp");
}

#[test]
fn project_serde_round_trips_with_precomp() {
    // A two-comp project — comp A holding a precomp layer referencing comp B —
    // round-trips through JSON: both comps and the reference survive.
    let mut a = Comp::empty_like("A", &Comp::new());
    a.id = 1;
    let mut pc = PulseLayer::of_kind(LayerKind::Precomp, "PC", [0.5; 4]);
    pc.precomp = PrecompLayer::to(2);
    a.layers.push(pc);
    let mut b = Comp::empty_like("B", &Comp::new());
    b.id = 2;

    let project = Project {
        comps: vec![a, b],
        active: 0,
        next_id: 3,
        presets: Vec::new(),
    };
    let json = serde_json::to_string(&project).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back.comps.len(), 2);
    assert_eq!(back.comps[0].layers[0].precomp.source, Some(2));
    assert_eq!(back.comp_by_id(2).map(|c| c.name.clone()), Some("B".to_string()));
}

#[test]
fn project_mints_unique_ids() {
    // mint_id never reuses or collides with a live comp id, even when next_id
    // lags behind (e.g. a hand-edited file).
    let mut p = Project::new(); // comp id 1, next_id 2
    let id_a = p.mint_id();
    let id_b = p.mint_id();
    assert_ne!(id_a, id_b);
    assert!(id_a >= 2 && id_b > id_a);
    // Force next_id to lag behind a live id, then mint: it must skip past it.
    p.next_id = 1;
    p.comps.push({
        let mut c = Comp::empty_like("X", &Comp::new());
        c.id = 50;
        c
    });
    let id_c = p.mint_id();
    assert!(id_c > 50, "mint_id skips past the highest live id, got {id_c}");
}

#[test]
fn push_comp_assigns_a_fresh_id() {
    let mut p = Project::new();
    let id = p.push_comp(Comp::empty_like("New", &Comp::new()));
    assert!(id >= 2);
    assert_eq!(p.comps.last().unwrap().id, id);
    assert!(p.comp_by_id(id).is_some());
    // The active comp (index 0) is unchanged and distinct.
    assert_ne!(p.active().id, id);
}

#[test]
fn precompose_wraps_layer_into_new_comp() {
    // Model-level analogue of the app's pre-compose: a project starts with one
    // comp holding a content layer; pre-compose moves that layer into a new comp
    // and replaces it in the host with a precomp referencing the new comp.
    let mut p = Project::new();
    let host_id = p.active().id;
    // The content layer we'll wrap (index 0 of the active comp's demo).
    let content_name = p.comps[0].layers[0].name.clone();

    // Build the nested comp and move the layer into it.
    let mut nested = Comp::empty_like(format!("{content_name} Comp"), &p.comps[0]);
    let wrapped = p.comps[0].layers[0].clone();
    nested.layers.push(wrapped);
    let new_id = p.push_comp(nested);

    // Replace the host layer with a precomp referencing the new comp.
    let mut precomp = PulseLayer::of_kind(LayerKind::Precomp, content_name.clone(), [0.5; 4]);
    precomp.precomp = PrecompLayer::to(new_id);
    p.comps[0].layers[0] = precomp;

    // The host's layer is now a precomp pointing at the new comp...
    let host = p.comp_by_id(host_id).unwrap();
    assert_eq!(host.layers[0].kind, LayerKind::Precomp);
    assert_eq!(host.layers[0].precomp.source, Some(new_id));
    // ...and the new comp holds the original content.
    let made = p.comp_by_id(new_id).unwrap();
    assert_eq!(made.layers.len(), 1);
    assert_eq!(made.layers[0].name, content_name);
    assert_ne!(new_id, host_id, "the precomp target is a distinct comp");
}

// --- Expressions on properties ---------------------------------------------

#[test]
fn expression_overrides_keyframed_value() {
    // `time * 2` ignores the keyframes and is a pure function of time.
    let mut track = Track::default();
    track.set_key(0.0, 100.0); // keyframed value the expression replaces
    track.expression = Some("time * 2".to_string());
    for &t in &[0.0_f32, 1.0, 2.5, 4.0] {
        let got = track.sample_expr(t, 0.0, ExprCtx::at(t, 0.0));
        assert!((got - t * 2.0).abs() < 1e-4, "t={t} got={got}");
    }
}

#[test]
fn expression_value_sees_keyframed_sample() {
    // `value + 10` offsets the *keyframed* value at each time, proving the
    // keyframed sample is exposed to the script as `value`.
    let mut track = Track::default();
    track.set_key(0.0, 0.0);
    track.set_key(2.0, 20.0); // linear ramp 0 -> 20
    track.expression = Some("value + 10".to_string());
    // Midpoint keyframed value is 10, so the expression yields 20.
    let got = track.sample_expr(1.0, 0.0, ExprCtx::at(1.0, 0.0));
    assert!((got - 20.0).abs() < 1e-4, "got={got}");
}

#[test]
fn malformed_expression_falls_back_to_keyframed_value() {
    // A syntax error must not panic and must fall back to the keyframed value.
    let mut track = Track::default();
    track.set_key(0.0, 42.0);
    track.expression = Some("this is not valid $#@".to_string());
    let got = track.sample_expr(0.0, 0.0, ExprCtx::at(0.0, 0.0));
    assert_eq!(got, 42.0, "malformed expression should fall back");
}

#[test]
fn empty_expression_is_keyframed() {
    let mut track = Track::default();
    track.set_key(0.0, 5.0);
    track.expression = Some("   ".to_string()); // whitespace-only = no expression
    assert!(!track.has_expression());
    assert_eq!(track.sample_expr(0.0, 0.0, ExprCtx::at(0.0, 0.0)), 5.0);
}

#[test]
fn expression_serde_round_trips() {
    let mut layer = PulseLayer::new("Expr", [1.0; 4]);
    layer.x.set_key(0.0, 1.0);
    layer.x.expression = Some("value + wiggle(2, 30)".to_string());
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert_eq!(
        back.x.expression.as_deref(),
        Some("value + wiggle(2, 30)"),
        "expression must survive a serde round-trip"
    );
    // A property without an expression deserializes as None (back-compat: the
    // field is skipped when empty).
    assert!(back.opacity.expression.is_none());
}

#[test]
fn missing_expression_field_defaults_to_none() {
    // A pre-expression track (no `expression` key) deserializes as None.
    let json = r#"{"keys":[{"t":0.0,"value":1.0}]}"#;
    let track: Track = serde_json::from_str(json).unwrap();
    assert!(track.expression.is_none());
    assert!(!track.has_expression());
}

#[test]
fn comp_layer_value_is_expression_aware() {
    // Through the comp-level sampler, an expression on a transform property
    // resolves with the comp's fps/duration/index context.
    let mut comp = Comp::new();
    // Put a deterministic expression on layer 0's rotation: `index * 90 + time`.
    comp.layers[0].rotation.expression = Some("index * 90 + time".to_string());
    let got = comp.layer_value(0, Prop::Rotation, 5.0);
    assert!((got - (0.0 * 90.0 + 5.0)).abs() < 1e-4, "got={got}");
    // The opacity sampler (no expression) is unaffected.
    let op = comp.layer_opacity(0, 0.0);
    assert!((0.0..=1.0).contains(&op));
}

// --- Time remapping ----------------------------------------------------

#[test]
fn time_remap_serde_defaults_to_disabled() {
    // A pre-time-remap layer (no `time_remap` field) loads with the remap off and
    // an empty track, so old projects sample their source at the comp time.
    let json = r#"{"name":"F","kind":"Footage","color":[0.5,0.5,0.5,1.0],"visible":true,
        "footage":{"source":null},
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}"#;
    let layer: PulseLayer = serde_json::from_str(json).unwrap();
    assert!(!layer.time_remap.enabled);
    assert!(!layer.time_remap.is_active());
    assert!(layer.time_remap.track.keys.is_empty());
}

#[test]
fn enabled_time_remap_layer_serde_round_trips() {
    // A footage layer with an enabled, keyed time-remap curve survives a JSON
    // round-trip (enable flag + the remap track's keys).
    let mut layer = PulseLayer::of_kind(LayerKind::Footage, "Plate", [0.5, 0.5, 0.5, 1.0]);
    layer.time_remap.enabled = true;
    layer.time_remap.track.set_key(0.0, 4.0);
    layer.time_remap.track.set_key(4.0, 0.0); // reverse ramp
    let json = serde_json::to_string(&layer).unwrap();
    let back: PulseLayer = serde_json::from_str(&json).unwrap();
    assert!(back.time_remap.enabled);
    assert!(back.time_remap.is_active());
    assert_eq!(back.time_remap.track.keys.len(), 2);
    assert!((back.time_remap.track.sample(1.0, 1.0) - 3.0).abs() < 1e-4);
}

#[test]
fn comp_layer_source_time_identity_when_off() {
    // With no remap, the comp's source-time sampler is the identity: source time
    // == comp time, so footage/precomp sampling is unchanged.
    let mut comp = Comp::new();
    comp.layers[0].kind = LayerKind::Footage; // any layer; remap off by default
    for &t in &[0.0, 1.0, 2.5, 5.0] {
        assert!((comp.layer_source_time(0, t) - t).abs() < 1e-6);
    }
}

#[test]
fn comp_layer_source_time_follows_active_remap() {
    // An active reversing remap drives the comp-level source time: r(t) = dur - t.
    let dur = 5.0_f32;
    let mut comp = Comp::new();
    comp.layers[0].kind = LayerKind::Footage;
    comp.layers[0].time_remap.enabled = true;
    comp.layers[0].time_remap.track.set_key(0.0, dur);
    comp.layers[0].time_remap.track.set_key(dur, 0.0);
    for &t in &[0.0, 1.0, 2.5, 5.0] {
        assert!((comp.layer_source_time(0, t) - (dur - t)).abs() < 1e-4, "t={t}");
    }
}

// --- Markers / work area -----------------------------------------------------

#[test]
fn comp_navigation_spans_comp_and_layer_markers() {
    // next/prev consider both the comp's own markers and the selected layer's.
    let mut comp = Comp::new();
    comp.markers = vec![Marker::at(1.0), Marker::at(4.0)];
    comp.layers[0].markers = vec![Marker::at(2.0)];
    // With layer 0 selected, the layer marker at 2.0 is in the set.
    assert_eq!(comp.next_marker(0.5, Some(0)), Some(1.0));
    assert_eq!(comp.next_marker(1.0, Some(0)), Some(2.0)); // the layer marker
    assert_eq!(comp.prev_marker(3.0, Some(0)), Some(2.0));
    assert_eq!(comp.next_marker(4.0, Some(0)), None);
    // With no layer selected, only comp markers count (the 2.0 layer marker is gone).
    assert_eq!(comp.next_marker(1.0, None), Some(4.0));
    assert_eq!(comp.prev_marker(3.0, None), Some(1.0));
}

#[test]
fn comp_navigation_ignores_other_layers_markers() {
    // Only the *selected* layer's markers join the comp's; another layer's don't.
    let mut comp = Comp::new();
    comp.markers.clear(); // drop the demo's comp marker to isolate layer markers
    comp.layers[0].markers = vec![Marker::at(1.0)];
    comp.layers[1].markers = vec![Marker::at(2.0)];
    assert_eq!(comp.next_marker(0.0, Some(0)), Some(1.0));
    // Selecting layer 0, layer 1's marker at 2.0 is not in the nav set.
    assert_eq!(comp.next_marker(1.0, Some(0)), None);
    // Selecting layer 1 instead surfaces its 2.0 marker.
    assert_eq!(comp.next_marker(1.0, Some(1)), Some(2.0));
}

#[test]
fn comp_clamped_work_area_stays_inside_timeline() {
    // A hand-edited / inverted work area is clamped to the comp's [0, duration].
    let mut comp = Comp::new();
    comp.duration = 5.0;
    comp.work_area = WorkArea { start: -2.0, end: 99.0 };
    let wa = comp.clamped_work_area();
    assert_eq!(wa, WorkArea { start: 0.0, end: 5.0 });
    // An inverted range collapses to an ordered (zero-length) area, never escapes.
    comp.work_area = WorkArea { start: 4.0, end: 1.0 };
    let wa = comp.clamped_work_area();
    assert_eq!(wa, WorkArea { start: 4.0, end: 4.0 });
}

#[test]
fn fresh_comp_work_area_spans_the_timeline() {
    let comp = Comp::new();
    assert!(comp.clamped_work_area().is_full(comp.duration));
}

#[test]
fn markers_and_work_area_serde_round_trip() {
    let mut comp = Comp::new();
    comp.markers = vec![{
        let mut m = Marker::at(1.5);
        m.label = "intro".to_string();
        m.duration = 0.5;
        m.color = [0.1, 0.2, 0.3];
        m
    }];
    comp.layers[0].markers = vec![Marker::at(2.0)];
    comp.work_area = WorkArea { start: 1.0, end: 3.0 };
    let json = serde_json::to_string(&comp).unwrap();
    let back: Comp = serde_json::from_str(&json).unwrap();
    assert_eq!(back.markers.len(), 1);
    assert_eq!(back.markers[0].label, "intro");
    assert!((back.markers[0].time - 1.5).abs() < 1e-6);
    assert!((back.markers[0].duration - 0.5).abs() < 1e-6);
    assert_eq!(back.markers[0].color, [0.1, 0.2, 0.3]);
    assert_eq!(back.layers[0].markers.len(), 1);
    assert_eq!(back.work_area, WorkArea { start: 1.0, end: 3.0 });
}

#[test]
fn markers_serde_default_to_empty_for_old_files() {
    // A pre-marker comp (no `markers` / `work_area` fields) loads with no markers
    // and an empty (full-on-clamp) work area.
    let json = r#"{"width":16,"height":16,"duration":2.0,"fps":30.0,
        "layers":[{"name":"L","color":[1.0,1.0,1.0,1.0],"visible":true,
        "x":{"keys":[]},"y":{"keys":[]},"scale":{"keys":[]},
        "rotation":{"keys":[]},"opacity":{"keys":[]}}]}"#;
    let comp: Comp = serde_json::from_str(json).unwrap();
    assert!(comp.markers.is_empty());
    assert!(comp.layers[0].markers.is_empty());
    // The stored serde-default WorkArea is the empty [0,0] range, but
    // `clamped_work_area` self-heals it to the whole timeline so a pre-work-area
    // project loops its full length (rather than a degenerate zero range).
    assert_eq!(comp.work_area, WorkArea { start: 0.0, end: 0.0 });
    assert_eq!(comp.clamped_work_area(), WorkArea { start: 0.0, end: 2.0 });
    assert!(comp.clamped_work_area().is_full(comp.duration));
}
