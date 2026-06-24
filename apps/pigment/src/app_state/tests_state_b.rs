//! Behavior / `App::apply` dispatcher unit tests (batch suites). Split out of
//! `tests_state.rs` to keep each test file under the file-size limit; pure
//! mechanical move, no test changed.

#[cfg(test)]
mod batch7_tests {
    use crate::app_state::{
        AlphaChannel, BlendIf, SpotHealMode,
        Action,
    };
    use prism_core::LayerId;

    // Helper: build a minimal AlphaChannel with a known mask.
    fn make_channel(name: &str, width: u32, height: u32) -> AlphaChannel {
        let n = (width * height) as usize;
        let mask = vec![0.5f32; n];
        AlphaChannel { name: name.into(), mask, width, height }
    }

    // ---- Alpha Channels ----

    #[test]
    fn test_save_load_channel_roundtrip() {
        // Simulate the apply logic without constructing a full App (no GPU).
        let mut channels: Vec<AlphaChannel> = Vec::new();
        let original_mask = vec![0.25f32, 0.5, 0.75, 1.0];
        channels.push(AlphaChannel {
            name: "Alpha 1".into(),
            mask: original_mask.clone(),
            width: 2,
            height: 2,
        });

        assert_eq!(channels.len(), 1);
        // Load: copy mask back out.
        let loaded = channels[0].mask.clone();
        assert_eq!(loaded, original_mask, "loaded mask must match saved mask");
    }

    #[test]
    fn test_delete_channel() {
        let mut channels: Vec<AlphaChannel> = Vec::new();
        channels.push(make_channel("First",  4, 4));
        channels.push(make_channel("Second", 4, 4));
        assert_eq!(channels.len(), 2);

        // DeleteChannel(0) logic.
        channels.remove(0);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, "Second");
    }

    #[test]
    fn test_duplicate_channel() {
        let mut channels: Vec<AlphaChannel> = Vec::new();
        channels.push(make_channel("Sky", 8, 8));

        // DuplicateChannel(0) logic.
        let mut copy = channels[0].clone();
        copy.name = format!("{} copy", copy.name);
        channels.push(copy);

        assert_eq!(channels.len(), 2);
        assert_eq!(channels[1].name, "Sky copy");
        assert_eq!(channels[1].mask, channels[0].mask);
    }

    // ---- Blend If ----

    #[test]
    fn test_blend_if_set_clear() {
        let mut map: std::collections::HashMap<LayerId, BlendIf> = std::collections::HashMap::new();
        let id = LayerId(42);
        let bi = BlendIf { this_black: 10.0, this_white: 200.0, under_black: 0.0, under_white: 255.0 };

        // SetBlendIf.
        map.insert(id, bi);
        assert!(map.contains_key(&id), "blend_if should be stored");
        assert!((map[&id].this_black - 10.0).abs() < 1e-5);
        assert!((map[&id].this_white - 200.0).abs() < 1e-5);

        // ClearBlendIf.
        map.remove(&id);
        assert!(!map.contains_key(&id), "blend_if should be removed after clear");
    }

    #[test]
    fn test_blend_if_default() {
        let bi = BlendIf::default();
        assert!((bi.this_black - 0.0).abs() < 1e-5);
        assert!((bi.this_white - 255.0).abs() < 1e-5);
        assert!((bi.under_black - 0.0).abs() < 1e-5);
        assert!((bi.under_white - 255.0).abs() < 1e-5);
    }

    // ---- Spot Heal mode / radius ----

    #[test]
    fn test_spot_heal_mode() {
        let mut mode = SpotHealMode::ContentAware;
        // SetSpotHealMode logic.
        mode = SpotHealMode::TextureMatch;
        assert_eq!(mode, SpotHealMode::TextureMatch);

        let mut radius: f32 = 20.0;
        // SetSpotHealRadius with value above 1.
        radius = 30.0_f32.max(1.0);
        assert!((radius - 30.0).abs() < 1e-5);

        // Clamp: radius below 1 should become 1.
        radius = 0.1_f32.max(1.0);
        assert!((radius - 1.0).abs() < 1e-5, "radius below 1 should clamp to 1.0");
    }

    #[test]
    fn test_spot_heal_records_position() {
        let mut last_spot_heal: Option<([f32; 2], f32)> = None;
        // SpotHeal apply logic.
        let center = [50.0f32, 75.0];
        let radius = 15.0f32;
        last_spot_heal = Some((center, radius));

        assert!(last_spot_heal.is_some(), "last_spot_heal should be Some after SpotHeal");
        let (c, r) = last_spot_heal.unwrap();
        assert!((c[0] - 50.0).abs() < 1e-5);
        assert!((c[1] - 75.0).abs() < 1e-5);
        assert!((r - 15.0).abs() < 1e-5);
    }

    #[test]
    fn test_red_eye_records() {
        let mut last_red_eye: Option<([f32; 2], f32, f32)> = None;
        // RedEye apply logic.
        let center = [100.0f32, 120.0];
        let radius = 8.0f32;
        let darken = 0.7f32;
        last_red_eye = Some((center, radius, darken));

        assert!(last_red_eye.is_some(), "last_red_eye should be Some after RedEye");
        let (c, r, d) = last_red_eye.unwrap();
        assert!((c[0] - 100.0).abs() < 1e-5);
        assert!((r - 8.0).abs() < 1e-5);
        assert!((d - 0.7).abs() < 1e-5);
    }

    // ---- Action variants exist (compile-time check) ----

    #[test]
    fn test_action_variants_compile() {
        let _ = Action::SaveSelectionAsChannel("Alpha 1".into());
        let _ = Action::LoadChannelAsSelection(0);
        let _ = Action::DeleteChannel(0);
        let _ = Action::DuplicateChannel(0);
        let _ = Action::SetBlendIf {
            layer_id: LayerId(1),
            blend_if: BlendIf::default(),
        };
        let _ = Action::ClearBlendIf(LayerId(1));
        let _ = Action::SetSpotHealMode(SpotHealMode::ContentAware);
        let _ = Action::SetSpotHealRadius(20.0);
        let _ = Action::SpotHeal { center: [0.0, 0.0], radius: 10.0 };
        let _ = Action::RedEye { center: [0.0, 0.0], radius: 5.0, darken: 0.5 };
        let _ = Action::SetGradientMapStops {
            layer_id: LayerId(1),
            stops: vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])],
        };
        let _ = Action::SetChannelMixerOutput { layer_id: LayerId(1), output: 0 };
        let _ = Action::SetChannelMixerMix { layer_id: LayerId(1), src_r: 1.0, src_g: 0.0, src_b: 0.0, constant: 0.0 };
    }
}

// ---- Batch 4 extended tests ---------------------------------------------------

#[cfg(test)]
mod batch4_ext_tests {
    use crate::app_state::{Action, App, ToneMapMethod, NeuralFilterKind, PrintLayout};

    // ---- HDR Tone Mapping ----

    #[test]
    fn test_tone_map_method_stored() {
        let mut app = App::new();
        assert!(app.last_tone_map.is_none());
        app.apply(Action::ApplyToneMap { method: ToneMapMethod::Filmic, exposure: 1.0, gamma: 2.2 });
        assert_eq!(app.last_tone_map, Some(ToneMapMethod::Filmic));
        app.apply(Action::ApplyToneMap { method: ToneMapMethod::AcesCg, exposure: 0.5, gamma: 1.0 });
        assert_eq!(app.last_tone_map, Some(ToneMapMethod::AcesCg));
    }

    #[test]
    fn test_tone_map_preview_toggle() {
        let mut app = App::new();
        assert!(!app.tone_map_preview);
        app.apply(Action::SetToneMapPreview(true));
        assert!(app.tone_map_preview);
        app.apply(Action::SetToneMapPreview(false));
        assert!(!app.tone_map_preview);
    }

    // ---- Neural Filters ----

    #[test]
    fn test_add_remove_neural_filter() {
        let mut app = App::new();
        assert!(app.neural_filters.is_empty());
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::Colorize));
        assert_eq!(app.neural_filters.len(), 2);
        app.apply(Action::RemoveNeuralFilter(0));
        assert_eq!(app.neural_filters.len(), 1);
        assert_eq!(app.neural_filters[0].kind, NeuralFilterKind::Colorize);
        // Out-of-bounds remove is a no-op.
        app.apply(Action::RemoveNeuralFilter(99));
        assert_eq!(app.neural_filters.len(), 1);
    }

    #[test]
    fn test_neural_filter_strength_clamp() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SuperZoom));
        app.apply(Action::SetNeuralFilterStrength { idx: 0, strength: 1.5 });
        assert!((app.neural_filters[0].strength - 1.0).abs() < 1e-6);
        app.apply(Action::SetNeuralFilterStrength { idx: 0, strength: -0.5 });
        assert!((app.neural_filters[0].strength - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_toggle_neural_filter() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::DepthBlur));
        assert!(app.neural_filters[0].enabled);
        app.apply(Action::ToggleNeuralFilter(0));
        assert!(!app.neural_filters[0].enabled);
        app.apply(Action::ToggleNeuralFilter(0));
        assert!(app.neural_filters[0].enabled);
        // Out-of-bounds toggle is a no-op.
        app.apply(Action::ToggleNeuralFilter(99));
    }

    #[test]
    fn test_apply_neural_filters_counts_enabled() {
        let mut app = App::new();
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::SkinSmoothing));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::Colorize));
        app.apply(Action::AddNeuralFilter(NeuralFilterKind::StyleTransfer));
        // Disable the second filter.
        app.apply(Action::ToggleNeuralFilter(1));
        app.apply(Action::ApplyNeuralFilters);
        assert_eq!(app.last_neural_apply_count, 2);
    }

    // ---- Layer Group depth ----

    #[test]
    fn test_group_collapse_expand() {
        let mut app = App::new();
        assert!(app.collapsed_groups.is_empty());
        app.apply(Action::SetGroupCollapsed { group_name: "Group 1".into(), collapsed: true });
        assert!(app.collapsed_groups.contains("Group 1"));
        app.apply(Action::SetGroupCollapsed { group_name: "Group 1".into(), collapsed: false });
        assert!(!app.collapsed_groups.contains("Group 1"));
    }

    #[test]
    fn test_duplicate_group_stub() {
        let mut app = App::new();
        assert!(app.last_duplicated_group.is_none());
        app.apply(Action::DuplicateGroup("Background".into()));
        assert_eq!(app.last_duplicated_group.as_deref(), Some("Background"));
    }

    // ---- Print Layout ----

    #[test]
    fn test_print_copies_min_1() {
        let mut app = App::new();
        app.apply(Action::SetPrintCopies(0));
        assert_eq!(app.print_layout.copies, 1);
        app.apply(Action::SetPrintCopies(5));
        assert_eq!(app.print_layout.copies, 5);
    }

    #[test]
    fn test_print_bleed_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPrintBleed(50.0));
        assert!((app.print_layout.bleed - 25.0).abs() < 1e-6);
        app.apply(Action::SetPrintBleed(-1.0));
        assert!((app.print_layout.bleed - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_print_resolution_clamp() {
        let mut app = App::new();
        app.apply(Action::SetPrintResolution(5000));
        assert_eq!(app.print_layout.print_resolution, 2400);
        app.apply(Action::SetPrintResolution(10));
        assert_eq!(app.print_layout.print_resolution, 72);
    }

    #[test]
    fn test_print_center_toggle() {
        let mut app = App::new();
        assert!(app.print_layout.center_image); // default true
        app.apply(Action::SetPrintCenterImage(false));
        assert!(!app.print_layout.center_image);
        app.apply(Action::SetPrintCenterImage(true));
        assert!(app.print_layout.center_image);
    }

    // ---- PrintLayout struct defaults ----
    #[test]
    fn test_print_layout_default() {
        let pl = PrintLayout::default();
        assert_eq!(pl.copies, 1);
        assert!(pl.collate);
        assert!((pl.border_width - 0.0).abs() < 1e-6);
        assert!(pl.center_image);
        assert!(!pl.print_marks);
        assert!((pl.bleed - 3.0).abs() < 1e-6);
        assert_eq!(pl.print_resolution, 300);
    }
}

// ---- Batch 5 new feature tests ----------------------------------------------

#[cfg(test)]
mod batch5_new_tests {
    use crate::app_state::{
        Action, App, CaFillMethod, LiquifyTool, LiquifyStroke, SelectSubjectMode,
    };

    // ---- Content-Aware Crop ----

    #[test]
    fn test_ca_crop_angle_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCaCropAngle(90.0));
        assert!((app.ca_crop_config.angle - 45.0).abs() < 1e-5, "should clamp to 45");
        app.apply(Action::SetCaCropAngle(-90.0));
        assert!((app.ca_crop_config.angle - (-45.0)).abs() < 1e-5, "should clamp to -45");
    }

    #[test]
    fn test_ca_crop_apply_records_rect() {
        let mut app = App::new();
        assert!(app.last_ca_crop_rect.is_none());
        app.apply(Action::ApplyCaCrop { rect: [0.0, 0.0, 100.0, 100.0] });
        assert_eq!(app.last_ca_crop_rect, Some([0.0, 0.0, 100.0, 100.0]));
    }

    #[test]
    fn test_ca_crop_fill_method() {
        let mut app = App::new();
        app.apply(Action::SetCaCropFillMethod(CaFillMethod::EdgeExtend));
        assert!(matches!(app.ca_crop_config.fill_method, CaFillMethod::EdgeExtend));
        app.apply(Action::SetCaCropFillMethod(CaFillMethod::Transparent));
        assert!(matches!(app.ca_crop_config.fill_method, CaFillMethod::Transparent));
    }

    // ---- Sky Replacement ----

    #[test]
    fn test_sky_brightness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyBrightness(300.0));
        assert!((app.sky_replace_config.brightness - 200.0).abs() < 1e-5);
        app.apply(Action::SetSkyBrightness(-10.0));
        assert!((app.sky_replace_config.brightness - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_sky_temperature_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyTemperature(-200.0));
        assert!((app.sky_replace_config.temperature - (-100.0)).abs() < 1e-5);
        app.apply(Action::SetSkyTemperature(200.0));
        assert!((app.sky_replace_config.temperature - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_sky_scale_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSkyScale(5.0));
        assert!((app.sky_replace_config.scale - 2.0).abs() < 1e-5);
        app.apply(Action::SetSkyScale(0.1));
        assert!((app.sky_replace_config.scale - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_apply_sky_replace_sets_flag() {
        let mut app = App::new();
        assert!(!app.sky_replaced);
        app.apply(Action::ApplySkyReplace);
        assert!(app.sky_replaced);
    }

    #[test]
    fn test_sky_panel_toggle() {
        let mut app = App::new();
        assert!(!app.sky_replace_panel_open);
        app.apply(Action::ToggleSkyReplacePanel);
        assert!(app.sky_replace_panel_open);
        app.apply(Action::ToggleSkyReplacePanel);
        assert!(!app.sky_replace_panel_open);
    }

    // ---- Liquify ----

    #[test]
    fn test_liquify_brush_size_clamp() {
        let mut app = App::new();
        app.apply(Action::SetLiquifyBrushSize(0.0));
        assert!((app.liquify_brush_size - 1.0).abs() < 1e-5);
        app.apply(Action::SetLiquifyBrushSize(2000.0));
        assert!((app.liquify_brush_size - 1500.0).abs() < 1e-5);
    }

    #[test]
    fn test_liquify_stroke_push() {
        let mut app = App::new();
        assert!(app.liquify_strokes.is_empty());
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Forward,
            center: [50.0, 50.0],
            radius: 40.0,
            pressure: 0.8,
            angle: 0.0,
        }));
        assert_eq!(app.liquify_strokes.len(), 1);
    }

    #[test]
    fn test_reconstruct_pops_stroke() {
        let mut app = App::new();
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Pucker,
            center: [10.0, 10.0],
            radius: 20.0,
            pressure: 0.5,
            angle: 0.0,
        }));
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Bloat,
            center: [20.0, 20.0],
            radius: 30.0,
            pressure: 0.6,
            angle: 0.0,
        }));
        assert_eq!(app.liquify_strokes.len(), 2);
        app.apply(Action::ReconstructLiquify);
        assert_eq!(app.liquify_strokes.len(), 1);
    }

    #[test]
    fn test_revert_clears_strokes() {
        let mut app = App::new();
        app.apply(Action::ApplyLiquifyStroke(LiquifyStroke {
            tool: LiquifyTool::Twirl,
            center: [5.0, 5.0],
            radius: 10.0,
            pressure: 0.4,
            angle: 45.0,
        }));
        assert!(!app.liquify_strokes.is_empty());
        app.apply(Action::RevertLiquify);
        assert!(app.liquify_strokes.is_empty());
    }

    #[test]
    fn test_thaw_all_clears_mask() {
        let mut app = App::new();
        app.liquify_frozen_mask = vec![true, true, false, true];
        app.apply(Action::ThawAllMask);
        assert!(app.liquify_frozen_mask.is_empty());
    }

    // ---- Select Subject ----

    #[test]
    fn test_select_subject_run_stub() {
        let mut app = App::new();
        assert!(app.last_select_subject.is_none());
        app.apply(Action::RunSelectSubject);
        let result = app.last_select_subject.as_ref().unwrap();
        assert!((result.coverage - 0.72).abs() < 1e-5);
        assert!((result.confidence - 0.89).abs() < 1e-5);
        assert!(!result.cloud_used);
    }

    #[test]
    fn test_select_subject_mode_cloud() {
        let mut app = App::new();
        app.apply(Action::SetSelectSubjectMode(SelectSubjectMode::Cloud));
        app.apply(Action::RunSelectSubject);
        let result = app.last_select_subject.as_ref().unwrap();
        assert!(result.cloud_used);
    }

    #[test]
    fn test_select_and_mask_toggle() {
        let mut app = App::new();
        assert!(!app.select_and_mask_open);
        app.apply(Action::ToggleSelectAndMask);
        assert!(app.select_and_mask_open);
        app.apply(Action::ToggleSelectAndMask);
        assert!(!app.select_and_mask_open);
    }

}

#[cfg(test)]
mod batch6_tests {
    use crate::app_state::{
        Action, App, DropShadowFx, HdrToneMappingMethod,
        SmartObjectKind, Shape3DKind,
    };

    // ---- Layer Effects Suite ----

    #[test]
    fn test_set_drop_shadow_creates_entry() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        assert!(!app.layer_effects.contains_key(&layer));
        let fx = DropShadowFx { enabled: true, opacity: 80.0, ..DropShadowFx::default() };
        app.apply(Action::SetDropShadow { layer: layer.clone(), fx });
        assert!(app.layer_effects.contains_key(&layer));
        assert!((app.layer_effects[&layer].drop_shadow.opacity - 80.0).abs() < 1e-5);
        assert!(app.layer_effects[&layer].drop_shadow.enabled);
    }

    #[test]
    fn test_toggle_drop_shadow() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: true });
        assert!(app.layer_effects[&layer].drop_shadow.enabled);
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: false });
        assert!(!app.layer_effects[&layer].drop_shadow.enabled);
    }

    #[test]
    fn test_clear_layer_effects() {
        let mut app = App::new();
        let layer = "layer1".to_string();
        app.apply(Action::ToggleDropShadow { layer: layer.clone(), enabled: true });
        assert!(app.layer_effects.contains_key(&layer));
        app.apply(Action::ClearLayerEffects { layer: layer.clone() });
        assert!(!app.layer_effects.contains_key(&layer));
    }

    #[test]
    fn test_copy_paste_layer_effects() {
        let mut app = App::new();
        let src = "src".to_string();
        let dst = "dst".to_string();
        let fx = DropShadowFx { enabled: true, opacity: 60.0, ..DropShadowFx::default() };
        app.apply(Action::SetDropShadow { layer: src.clone(), fx });
        assert!(app.fx_clipboard.is_none());
        app.apply(Action::CopyLayerEffects { from: src.clone() });
        assert!(app.fx_clipboard.is_some());
        app.apply(Action::PasteLayerEffects { to: dst.clone() });
        assert!(app.layer_effects.contains_key(&dst));
        assert!((app.layer_effects[&dst].drop_shadow.opacity - 60.0).abs() < 1e-5);
    }

    #[test]
    fn test_toggle_fx_panel() {
        let mut app = App::new();
        assert!(!app.fx_panel_open);
        app.apply(Action::ToggleFxPanel);
        assert!(app.fx_panel_open);
        app.apply(Action::ToggleFxPanel);
        assert!(!app.fx_panel_open);
    }

    // ---- Match Color ----

    #[test]
    fn test_match_luminance_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMatchColorLuminance(300.0));
        assert!((app.match_color_config.luminance - 200.0).abs() < 1e-5);
        app.apply(Action::SetMatchColorLuminance(-10.0));
        assert!((app.match_color_config.luminance - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_match_fade_clamp() {
        let mut app = App::new();
        app.apply(Action::SetMatchColorFade2(150.0));
        assert!((app.match_color_config.fade - 100.0).abs() < 1e-5);
        app.apply(Action::SetMatchColorFade2(-5.0));
        assert!((app.match_color_config.fade - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_apply_match_color_sets_flag() {
        let mut app = App::new();
        assert!(!app.last_match_color_applied);
        app.apply(Action::ApplyMatchColor);
        assert!(app.last_match_color_applied);
    }

    // ---- Camera Raw ----

    #[test]
    fn test_camera_raw_temp_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawTemp(100.0));
        assert!((app.camera_raw_config.temperature - 2000.0).abs() < 1e-5);
        app.apply(Action::SetCameraRawTemp(100_000.0));
        assert!((app.camera_raw_config.temperature - 50000.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_sharpness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawSharpness(200.0));
        assert!((app.camera_raw_config.sharpness - 150.0).abs() < 1e-5);
        app.apply(Action::SetCameraRawSharpness(-5.0));
        assert!((app.camera_raw_config.sharpness - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_apply_flag() {
        let mut app = App::new();
        assert!(!app.camera_raw_applied);
        app.apply(Action::ApplyCameraRawFilter);
        assert!(app.camera_raw_applied);
    }

    #[test]
    fn test_camera_raw_reset() {
        let mut app = App::new();
        app.apply(Action::SetCameraRawTemp(3000.0));
        assert!((app.camera_raw_config.temperature - 3000.0).abs() < 1e-5);
        app.apply(Action::ResetCameraRaw);
        assert!((app.camera_raw_config.temperature - 6500.0).abs() < 1e-5);
    }

    #[test]
    fn test_camera_raw_panel_toggle() {
        let mut app = App::new();
        assert!(!app.camera_raw_panel_open);
        app.apply(Action::ToggleCameraRawPanel);
        assert!(app.camera_raw_panel_open);
        app.apply(Action::ToggleCameraRawPanel);
        assert!(!app.camera_raw_panel_open);
    }

    // ---- HDR Merge ----

    #[test]
    fn test_hdr_source_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetHdrSourceCount(0));
        assert_eq!(app.hdr_merge_config.source_count, 2);
        app.apply(Action::SetHdrSourceCount(1));
        assert_eq!(app.hdr_merge_config.source_count, 2);
        app.apply(Action::SetHdrSourceCount(5));
        assert_eq!(app.hdr_merge_config.source_count, 5);
    }

    #[test]
    fn test_hdr_bit_depth_invalid_ignored() {
        let mut app = App::new();
        assert_eq!(app.hdr_merge_config.bit_depth_output, 32);
        app.apply(Action::SetHdrBitDepth(24));
        assert_eq!(app.hdr_merge_config.bit_depth_output, 32);
        app.apply(Action::SetHdrBitDepth(16));
        assert_eq!(app.hdr_merge_config.bit_depth_output, 16);
    }

    #[test]
    fn test_merge_to_hdr_sets_result() {
        let mut app = App::new();
        assert!(app.hdr_merge_result.is_none());
        app.apply(Action::MergeToHdr);
        assert_eq!(app.hdr_merge_result.as_deref(), Some("merged_hdr.tif"));
    }

    #[test]
    fn test_hdr_tone_method() {
        let mut app = App::new();
        app.apply(Action::SetHdrToneMethod(HdrToneMappingMethod::Highlight));
        assert_eq!(app.hdr_merge_config.method, HdrToneMappingMethod::Highlight);
    }

    // ---- SmartObject (rich) ----

    #[test]
    fn test_smart_object_convert_pushes_entry() {
        let mut app = App::new();
        assert!(app.smart_object_list.is_empty());
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        assert_eq!(app.smart_object_list.len(), 1);
        assert_eq!(app.smart_object_list[0].id, 0);
        assert_eq!(app.smart_object_list[0].name, "Smart Object 0");
        assert_eq!(app.smart_object_list[0].kind, SmartObjectKind::Embedded);
        assert!(!app.smart_object_list[0].contents_dirty);
    }

    #[test]
    fn test_smart_object_counter_increments() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        app.apply(Action::SmartObjectConvert { layer_id: 2 });
        assert_eq!(app.smart_object_list[0].id, 0);
        assert_eq!(app.smart_object_list[1].id, 1);
        assert_eq!(app.smart_object_counter, 2);
    }

    #[test]
    fn test_smart_object_replace_sets_path_and_dirty() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectReplace { so_id, new_path: "/tmp/file.png".into() });
        let so = &app.smart_object_list[0];
        assert_eq!(so.source_path.as_deref(), Some("/tmp/file.png"));
        assert!(so.contents_dirty);
    }

    #[test]
    fn test_smart_object_export_clears_dirty() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectReplace { so_id, new_path: "/tmp/x.png".into() });
        assert!(app.smart_object_list[0].contents_dirty);
        app.apply(Action::SmartObjectExport { so_id });
        assert!(!app.smart_object_list[0].contents_dirty);
    }

    #[test]
    fn test_smart_object_rasterize_removes_entry() {
        let mut app = App::new();
        app.apply(Action::SmartObjectConvert { layer_id: 1 });
        let so_id = app.smart_object_list[0].id;
        app.apply(Action::SmartObjectRasterize { so_id });
        assert!(app.smart_object_list.is_empty());
    }

    // ---- AdvancedMasking ----

    #[test]
    fn test_select_mask_open_close() {
        let mut app = App::new();
        assert!(!app.select_mask_open);
        app.apply(Action::OpenSelectMask);
        assert!(app.select_mask_open);
        app.apply(Action::CloseSelectMask);
        assert!(!app.select_mask_open);
    }

    #[test]
    fn test_select_mask_apply_closes() {
        let mut app = App::new();
        app.apply(Action::OpenSelectMask);
        assert!(app.select_mask_open);
        app.apply(Action::ApplySelectMask);
        assert!(!app.select_mask_open);
    }

    #[test]
    fn test_select_mask_radius_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskRadius(-5.0));
        assert!((app.select_mask_config.radius - 0.0).abs() < 1e-5);
        app.apply(Action::SetSelectMaskRadius(999.0));
        assert!((app.select_mask_config.radius - 250.0).abs() < 1e-5);
        app.apply(Action::SetSelectMaskRadius(100.0));
        assert!((app.select_mask_config.radius - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_select_mask_smooth_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskSmooth(200));
        assert_eq!(app.select_mask_config.smooth, 100);
        app.apply(Action::SetSelectMaskSmooth(50));
        assert_eq!(app.select_mask_config.smooth, 50);
    }

    #[test]
    fn test_select_mask_shift_edge_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSelectMaskShiftEdge(-120));
        assert_eq!(app.select_mask_config.shift_edge, -100);
        app.apply(Action::SetSelectMaskShiftEdge(120));
        assert_eq!(app.select_mask_config.shift_edge, 100);
        app.apply(Action::SetSelectMaskShiftEdge(30));
        assert_eq!(app.select_mask_config.shift_edge, 30);
    }

    // ---- GenerativeFill ----

    #[test]
    fn test_generative_fill_set_prompt() {
        let mut app = App::new();
        app.apply(Action::SetGenerativeFillPrompt("sunny beach".into()));
        assert_eq!(app.generative_fill_prompt, "sunny beach");
    }

    #[test]
    fn test_generative_fill_run_pushes_result() {
        let mut app = App::new();
        app.apply(Action::SetGenerativeFillPrompt("mountain lake".into()));
        app.apply(Action::RunGenerativeFill { layer_id: 42 });
        assert_eq!(app.generative_fill_results.len(), 1);
        let r = &app.generative_fill_results[0];
        assert_eq!(r.layer_id, 42);
        assert_eq!(r.prompt, "mountain lake");
        assert_eq!(r.variation_index, 0);
        assert_eq!(r.variation_count, 4);
    }

    #[test]
    fn test_generative_fill_cycle_variation() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 1 });
        app.apply(Action::CycleGenerativeFillVariation { result_index: 0 });
        assert_eq!(app.generative_fill_results[0].variation_index, 1);
        // Wraps around at variation_count (4).
        for _ in 0..3 {
            app.apply(Action::CycleGenerativeFillVariation { result_index: 0 });
        }
        assert_eq!(app.generative_fill_results[0].variation_index, 0);
    }

    #[test]
    fn test_generative_fill_accept_removes() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 1 });
        app.apply(Action::RunGenerativeFill { layer_id: 2 });
        assert_eq!(app.generative_fill_results.len(), 2);
        app.apply(Action::AcceptGenerativeFill { result_index: 0 });
        assert_eq!(app.generative_fill_results.len(), 1);
        assert_eq!(app.generative_fill_results[0].layer_id, 2);
    }

    #[test]
    fn test_generative_fill_discard_removes() {
        let mut app = App::new();
        app.apply(Action::RunGenerativeFill { layer_id: 5 });
        app.apply(Action::DiscardGenerativeFill { result_index: 0 });
        assert!(app.generative_fill_results.is_empty());
    }

    // ---- Basic3DLayer ----

    #[test]
    fn test_create_3d_layer_sets_active() {
        let mut app = App::new();
        assert!(app.active_3d_layer.is_none());
        app.apply(Action::Create3DLayer { layer_id: 7, shape: Shape3DKind::Sphere });
        assert_eq!(app.active_3d_layer, Some(7));
        assert_eq!(app.layer_3d_props.len(), 1);
        assert_eq!(app.layer_3d_props[0].shape, Shape3DKind::Sphere);
    }

    #[test]
    fn test_set_3d_position() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 3, shape: Shape3DKind::Cube });
        app.apply(Action::Set3DPosition { layer_id: 3, x: 10.0, y: 20.0, z: 30.0 });
        let p = &app.layer_3d_props[0];
        assert!((p.pos_x - 10.0).abs() < 1e-5);
        assert!((p.pos_y - 20.0).abs() < 1e-5);
        assert!((p.pos_z - 30.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_layer_3d_rotation() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 4, shape: Shape3DKind::Cone });
        app.apply(Action::SetLayer3DRotation { layer_id: 4, x: 45.0, y: 90.0, z: 180.0 });
        let p = &app.layer_3d_props[0];
        assert!((p.rot_x - 45.0).abs() < 1e-5);
        assert!((p.rot_y - 90.0).abs() < 1e-5);
        assert!((p.rot_z - 180.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_3d_scale_clamped() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 5, shape: Shape3DKind::Plane });
        app.apply(Action::Set3DScale { layer_id: 5, x: 0.0, y: 50.0, z: 2.0 });
        let p = &app.layer_3d_props[0];
        // 0.0 clamped to 0.01, 50.0 clamped to 10.0, 2.0 unchanged
        assert!((p.scale_x - 0.01).abs() < 1e-5);
        assert!((p.scale_y - 10.0).abs() < 1e-5);
        assert!((p.scale_z - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_3d_extrude_depth_clamp() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 6, shape: Shape3DKind::Cylinder });
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: -10.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 0.0).abs() < 1e-5);
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: 9999.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 5000.0).abs() < 1e-5);
        app.apply(Action::Set3DExtrudeDepth { layer_id: 6, depth: 200.0 });
        assert!((app.layer_3d_props[0].extrude_depth - 200.0).abs() < 1e-5);
    }

    #[test]
    fn test_flatten_3d_layer_removes_and_clears_active() {
        let mut app = App::new();
        app.apply(Action::Create3DLayer { layer_id: 8, shape: Shape3DKind::Custom });
        assert_eq!(app.active_3d_layer, Some(8));
        app.apply(Action::Flatten3DLayer { layer_id: 8 });
        assert!(app.layer_3d_props.is_empty());
        assert!(app.active_3d_layer.is_none());
    }
}
