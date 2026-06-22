#[cfg(test)]
mod tests_batch5_effects {
    use super::super::*;
    use crate::app_state::effects_chain::{Batch5Effect, CellPatternKind, GradientEffectKind};

    #[test]
    fn test_add_motion_blur_effect() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 45.0, distance: 10.0 });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert_eq!(stack.len(), 1);
        if let Batch5Effect::MotionBlur { angle, distance } = &stack[0] {
            assert!((angle - 45.0).abs() < 1e-5);
            assert!((distance - 10.0).abs() < 1e-5);
        } else {
            panic!("expected MotionBlur");
        }
    }

    #[test]
    fn test_set_motion_blur_updates_existing() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 0.0, distance: 0.0 });
        app.apply(Action::SetMotionBlur { layer_id: 0, angle: 90.0, distance: 20.0 });
        let stack = app.batch5_effects.get(&0).unwrap();
        if let Batch5Effect::MotionBlur { angle, distance } = &stack[0] {
            assert!((angle - 90.0).abs() < 1e-5);
            assert!((distance - 20.0).abs() < 1e-5);
        } else {
            panic!("expected MotionBlur");
        }
    }

    #[test]
    fn test_add_glow_effect() {
        let mut app = App::new();
        app.apply(Action::AddGlowEffect { layer_id: 1, threshold: 0.5, radius: 5.0, intensity: 1.0 });
        let stack = app.batch5_effects.get(&1).unwrap();
        assert_eq!(stack.len(), 1);
        if let Batch5Effect::Glow { threshold, radius, intensity, .. } = &stack[0] {
            assert!((threshold - 0.5).abs() < 1e-5);
            assert!((radius - 5.0).abs() < 1e-5);
            assert!((intensity - 1.0).abs() < 1e-5);
        } else {
            panic!("expected Glow");
        }
    }

    #[test]
    fn test_add_cc_repe_tile_effect() {
        let mut app = App::new();
        app.apply(Action::AddCcRepeTileEffect {
            layer_id: 0,
            expand_right: 10.0,
            expand_left: 5.0,
            expand_up: 3.0,
            expand_down: 7.0,
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        if let Batch5Effect::CcRepeTile { expand_right, expand_left, expand_up, expand_down, .. } = &stack[0] {
            assert!((expand_right - 10.0).abs() < 1e-5);
            assert!((expand_left - 5.0).abs() < 1e-5);
            assert!((expand_up - 3.0).abs() < 1e-5);
            assert!((expand_down - 7.0).abs() < 1e-5);
        } else {
            panic!("expected CcRepeTile");
        }
    }

    #[test]
    fn test_add_posterize_time() {
        let mut app = App::new();
        app.apply(Action::AddPosterizeTime { layer_id: 0, frame_rate: 6.0 });
        let stack = app.batch5_effects.get(&0).unwrap();
        if let Batch5Effect::PosterizeTime { frame_rate } = &stack[0] {
            assert!((frame_rate - 6.0).abs() < 1e-5);
        } else {
            panic!("expected PosterizeTime");
        }
    }

    #[test]
    fn test_posterize_time_clamps_to_min_1() {
        let mut app = App::new();
        app.apply(Action::AddPosterizeTime { layer_id: 0, frame_rate: 0.0 });
        if let Batch5Effect::PosterizeTime { frame_rate } = &app.batch5_effects[&0][0] {
            assert!(*frame_rate >= 1.0, "frame_rate should be clamped to minimum 1.0");
        }
    }

    #[test]
    fn test_add_cell_pattern() {
        let mut app = App::new();
        app.apply(Action::AddCellPattern {
            layer_id: 0,
            pattern: CellPatternKind::Bubbles,
            size: 50.0,
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert_eq!(stack.len(), 1);
        assert!(matches!(stack[0], Batch5Effect::CellPattern { .. }));
    }

    #[test]
    fn test_add_checkerboard() {
        let mut app = App::new();
        app.apply(Action::AddCheckerboard {
            layer_id: 0,
            size: 32.0,
            color_a: "#000000".to_string(),
            color_b: "#ffffff".to_string(),
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert!(matches!(stack[0], Batch5Effect::Checkerboard { .. }));
    }

    #[test]
    fn test_add_gradient_effect() {
        let mut app = App::new();
        app.apply(Action::AddGradientEffect {
            layer_id: 0,
            kind: GradientEffectKind::Linear,
            start_color: "#ff0000".to_string(),
            end_color: "#0000ff".to_string(),
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert!(matches!(stack[0], Batch5Effect::Gradient { .. }));
    }

    #[test]
    fn test_add_grid_effect() {
        let mut app = App::new();
        app.apply(Action::AddGridEffect {
            layer_id: 0,
            size: (100.0, 100.0),
            border: 2.0,
            color: "#ffffff".to_string(),
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert!(matches!(stack[0], Batch5Effect::Grid { .. }));
    }

    #[test]
    fn test_add_stroke_effect() {
        let mut app = App::new();
        app.apply(Action::AddStrokeEffect {
            layer_id: 0,
            color: "#ff0000".to_string(),
            brush_size: 3.0,
        });
        let stack = app.batch5_effects.get(&0).unwrap();
        assert!(matches!(stack[0], Batch5Effect::Stroke { .. }));
    }

    #[test]
    fn test_remove_batch5_effect() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 45.0, distance: 10.0 });
        app.apply(Action::AddGlowEffect { layer_id: 0, threshold: 0.5, radius: 5.0, intensity: 1.0 });
        assert_eq!(app.batch5_effects[&0].len(), 2);
        app.apply(Action::RemoveBatch5Effect { layer_id: 0, effect_idx: 0 });
        assert_eq!(app.batch5_effects[&0].len(), 1);
        assert!(matches!(app.batch5_effects[&0][0], Batch5Effect::Glow { .. }));
    }

    #[test]
    fn test_multiple_layers_get_separate_stacks() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 0.0, distance: 5.0 });
        app.apply(Action::AddGlowEffect { layer_id: 1, threshold: 0.5, radius: 3.0, intensity: 0.8 });
        assert_eq!(app.batch5_effects.get(&0).map(|v| v.len()), Some(1));
        assert_eq!(app.batch5_effects.get(&1).map(|v| v.len()), Some(1));
    }

    #[test]
    fn test_remove_out_of_range_noop() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 45.0, distance: 10.0 });
        app.apply(Action::RemoveBatch5Effect { layer_id: 0, effect_idx: 99 });
        assert_eq!(app.batch5_effects[&0].len(), 1);
    }

    #[test]
    fn test_effects_undoable() {
        let mut app = App::new();
        app.apply(Action::AddMotionBlurEffect { layer_id: 0, angle: 45.0, distance: 10.0 });
        assert!(app.can_undo());
    }

    #[test]
    fn test_add_gradient_radial() {
        let mut app = App::new();
        app.apply(Action::AddGradientEffect {
            layer_id: 0,
            kind: GradientEffectKind::Radial,
            start_color: "#ffff00".to_string(),
            end_color: "#000000".to_string(),
        });
        if let Batch5Effect::Gradient { kind, .. } = &app.batch5_effects[&0][0] {
            assert!(matches!(kind, GradientEffectKind::Radial));
        } else {
            panic!("expected Gradient");
        }
    }
}

#[cfg(test)]
mod tests_batch5_motion_paths {
    use super::super::*;

    #[test]
    fn test_create_motion_path() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        assert_eq!(app.motion_paths.len(), 1);
        assert_eq!(app.motion_paths[0].layer_id, 0);
        assert_eq!(app.motion_paths[0].id, 0);
        assert_eq!(app.next_motion_path_id, 1);
    }

    #[test]
    fn test_add_motion_path_point() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 1.0, x: 100.0, y: 200.0 });
        assert_eq!(app.motion_paths[0].points.len(), 1);
        assert!((app.motion_paths[0].points[0].x - 100.0).abs() < 1e-5);
        assert!((app.motion_paths[0].points[0].y - 200.0).abs() < 1e-5);
    }

    #[test]
    fn test_motion_path_points_sorted_by_time() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 2.0, x: 200.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 1.0, x: 100.0, y: 0.0 });
        let pts = &app.motion_paths[0].points;
        assert!((pts[0].time_s - 0.0).abs() < 1e-5);
        assert!((pts[1].time_s - 1.0).abs() < 1e-5);
        assert!((pts[2].time_s - 2.0).abs() < 1e-5);
    }

    #[test]
    fn test_remove_motion_path_point() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 1.0, x: 100.0, y: 100.0 });
        assert_eq!(app.motion_paths[0].points.len(), 2);
        app.apply(Action::RemoveMotionPathPoint { path_id: 0, index: 0 });
        assert_eq!(app.motion_paths[0].points.len(), 1);
        assert!((app.motion_paths[0].points[0].time_s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_motion_path_point() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::SetMotionPathPoint { path_id: 0, index: 0, x: 50.0, y: 75.0 });
        assert!((app.motion_paths[0].points[0].x - 50.0).abs() < 1e-5);
        assert!((app.motion_paths[0].points[0].y - 75.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_auto_orient() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        assert!(!app.motion_paths[0].auto_orient);
        app.apply(Action::SetAutoOrient { path_id: 0, enabled: true });
        assert!(app.motion_paths[0].auto_orient);
    }

    #[test]
    fn test_delete_motion_path() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::CreateMotionPath { layer_id: 1 });
        assert_eq!(app.motion_paths.len(), 2);
        app.apply(Action::DeleteMotionPath { path_id: 0 });
        assert_eq!(app.motion_paths.len(), 1);
        assert_eq!(app.motion_paths[0].id, 1);
    }

    #[test]
    fn test_sample_motion_path_linear_midpoint() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 2.0, x: 200.0, y: 100.0 });
        let pos = app.sample_motion_path(0, 1.0).unwrap();
        assert!((pos.0 - 100.0).abs() < 1e-3, "x midpoint, got {}", pos.0);
        assert!((pos.1 - 50.0).abs() < 1e-3, "y midpoint, got {}", pos.1);
    }

    #[test]
    fn test_sample_motion_path_clamps_before_start() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 1.0, x: 100.0, y: 50.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 2.0, x: 200.0, y: 100.0 });
        let pos = app.sample_motion_path(0, 0.0).unwrap();
        assert!((pos.0 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn test_sample_motion_path_clamps_after_end() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 1.0, x: 100.0, y: 50.0 });
        let pos = app.sample_motion_path(0, 99.0).unwrap();
        assert!((pos.0 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn test_sample_motion_path_hold_easing() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 0.0, x: 0.0, y: 0.0 });
        app.apply(Action::AddMotionPathPoint { path_id: 0, time_s: 2.0, x: 200.0, y: 0.0 });
        app.apply(Action::SetMotionPathEasing { path_id: 0, index: 1, easing: MotionEasing::Hold });
        let pos = app.sample_motion_path(0, 1.0).unwrap();
        assert!((pos.0 - 0.0).abs() < 1e-3, "hold easing = no movement until last keyframe");
    }

    #[test]
    fn test_sample_empty_path_returns_none() {
        let mut app = App::new();
        app.apply(Action::CreateMotionPath { layer_id: 0 });
        assert!(app.sample_motion_path(0, 1.0).is_none());
    }

    #[test]
    fn test_sample_nonexistent_path_returns_none() {
        let app = App::new();
        assert!(app.sample_motion_path(999, 0.0).is_none());
    }
}

#[cfg(test)]
mod tests_batch5_shape_groups {
    use super::super::*;
    use crate::app_state::shape_groups::{MergeMode, ShapeItemKind, ShapeGroupTransform};

    #[test]
    fn test_add_shape_group() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "Group 1".to_string() });
        assert_eq!(app.shape_groups.len(), 1);
        assert_eq!(app.shape_groups[0].name, "Group 1");
        assert_eq!(app.shape_groups[0].id, 0);
    }

    #[test]
    fn test_add_shape_item_rectangle() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::AddShapeItemToGroup {
            group_id: 0,
            kind: ShapeItemKind::Rectangle { size: (100.0, 50.0), roundness: 0.0, position: (0.0, 0.0) },
        });
        assert_eq!(app.shape_groups[0].items.len(), 1);
        assert!(matches!(app.shape_groups[0].items[0], ShapeItemKind::Rectangle { .. }));
    }

    #[test]
    fn test_remove_shape_item_from_group() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::AddShapeItemToGroup {
            group_id: 0,
            kind: ShapeItemKind::Ellipse { size: (50.0, 50.0), position: (0.0, 0.0) },
        });
        app.apply(Action::AddShapeItemToGroup {
            group_id: 0,
            kind: ShapeItemKind::Rectangle { size: (100.0, 50.0), roundness: 0.0, position: (0.0, 0.0) },
        });
        assert_eq!(app.shape_groups[0].items.len(), 2);
        app.apply(Action::RemoveShapeItemFromGroup { group_id: 0, item_index: 0 });
        assert_eq!(app.shape_groups[0].items.len(), 1);
        assert!(matches!(app.shape_groups[0].items[0], ShapeItemKind::Rectangle { .. }));
    }

    #[test]
    fn test_set_shape_group_transform() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        let tf = ShapeGroupTransform {
            position: (50.0, 75.0),
            rotation: 45.0,
            ..ShapeGroupTransform::default()
        };
        app.apply(Action::SetShapeGroupTransform { group_id: 0, transform: tf });
        assert!((app.shape_groups[0].transform.position.0 - 50.0).abs() < 1e-5);
        assert!((app.shape_groups[0].transform.rotation - 45.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_shape_star() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::SetShapeStar {
            layer_id: 0,
            group_id: 0,
            points: 5,
            inner_radius: 30.0,
            outer_radius: 60.0,
        });
        assert_eq!(app.shape_groups[0].items.len(), 1);
        if let ShapeItemKind::Star { points, inner_radius, outer_radius, .. } = &app.shape_groups[0].items[0] {
            assert_eq!(*points, 5);
            assert!((inner_radius - 30.0).abs() < 1e-5);
            assert!((outer_radius - 60.0).abs() < 1e-5);
        } else {
            panic!("expected Star");
        }
    }

    #[test]
    fn test_add_repeater_to_group() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::AddRepeaterToGroup { layer_id: 0, group_id: 0, copies: 4 });
        if let ShapeItemKind::Repeater { copies, .. } = &app.shape_groups[0].items[0] {
            assert_eq!(*copies, 4);
        } else {
            panic!("expected Repeater");
        }
    }

    #[test]
    fn test_add_trim_path_clamped() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::AddTrimPath { layer_id: 0, group_id: 0, start: -0.5, end: 1.5 });
        if let ShapeItemKind::Trim { start, end, .. } = &app.shape_groups[0].items[0] {
            assert!((start - 0.0).abs() < 1e-5, "start clamped to 0");
            assert!((end - 1.0).abs() < 1e-5, "end clamped to 1");
        } else {
            panic!("expected Trim");
        }
    }

    #[test]
    fn test_add_merge_shapes() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::AddMergeShapes { layer_id: 0, group_id: 0, mode: MergeMode::Add });
        if let ShapeItemKind::Merge { mode } = &app.shape_groups[0].items[0] {
            assert!(matches!(mode, MergeMode::Add));
        } else {
            panic!("expected Merge");
        }
    }

    #[test]
    fn test_delete_shape_group() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G1".to_string() });
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G2".to_string() });
        assert_eq!(app.shape_groups.len(), 2);
        app.apply(Action::DeleteShapeGroup { group_id: 0 });
        assert_eq!(app.shape_groups.len(), 1);
        assert_eq!(app.shape_groups[0].name, "G2");
    }

    #[test]
    fn test_group_ids_auto_increment() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "A".to_string() });
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "B".to_string() });
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "C".to_string() });
        assert_eq!(app.shape_groups[0].id, 0);
        assert_eq!(app.shape_groups[1].id, 1);
        assert_eq!(app.shape_groups[2].id, 2);
    }

    #[test]
    fn test_remove_item_out_of_range_noop() {
        let mut app = App::new();
        app.apply(Action::AddShapeGroup { layer_id: 0, name: "G".to_string() });
        app.apply(Action::RemoveShapeItemFromGroup { group_id: 0, item_index: 99 });
        assert_eq!(app.shape_groups[0].items.len(), 0);
    }
}

#[cfg(test)]
mod tests_batch5_audio_mixer {
    use super::super::*;

    #[test]
    fn test_add_audio_bus() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "Main".to_string() });
        assert_eq!(app.audio_buses.len(), 1);
        assert_eq!(app.audio_buses[0].name, "Main");
        assert!((app.audio_buses[0].volume - 1.0).abs() < 1e-5);
        assert!((app.audio_buses[0].pan - 0.0).abs() < 1e-5);
        assert!(!app.audio_buses[0].muted);
        assert!(!app.audio_buses[0].solo);
    }

    #[test]
    fn test_remove_audio_bus() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::AddAudioBus { name: "B".to_string() });
        let id_b = app.audio_buses[1].id;
        app.apply(Action::RemoveAudioBus { bus_id: 0 });
        assert_eq!(app.audio_buses.len(), 1);
        assert_eq!(app.audio_buses[0].id, id_b);
    }

    #[test]
    fn test_set_bus_volume_clamped() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SetBusVolume { bus_id: 0, volume: 1.5 });
        assert!((app.audio_buses[0].volume - 1.0).abs() < 1e-5, "clamped to 1.0");
        app.apply(Action::SetBusVolume { bus_id: 0, volume: -0.5 });
        assert!((app.audio_buses[0].volume - 0.0).abs() < 1e-5, "clamped to 0.0");
        app.apply(Action::SetBusVolume { bus_id: 0, volume: 0.75 });
        assert!((app.audio_buses[0].volume - 0.75).abs() < 1e-5);
    }

    #[test]
    fn test_set_bus_pan_clamped() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SetBusPan { bus_id: 0, pan: 2.0 });
        assert!((app.audio_buses[0].pan - 1.0).abs() < 1e-5, "clamped to 1.0");
        app.apply(Action::SetBusPan { bus_id: 0, pan: -2.0 });
        assert!((app.audio_buses[0].pan - (-1.0)).abs() < 1e-5, "clamped to -1.0");
    }

    #[test]
    fn test_mute_bus() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::MuteBus { bus_id: 0, muted: true });
        assert!(app.audio_buses[0].muted);
        app.apply(Action::MuteBus { bus_id: 0, muted: false });
        assert!(!app.audio_buses[0].muted);
    }

    #[test]
    fn test_solo_bus() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SoloBus { bus_id: 0, solo: true });
        assert!(app.audio_buses[0].solo);
    }

    #[test]
    fn test_add_bus_send() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::AddAudioBus { name: "B".to_string() });
        app.apply(Action::AddBusSend { from_id: 0, to_id: 1, level: 0.5 });
        assert_eq!(app.audio_buses[0].sends.len(), 1);
        assert_eq!(app.audio_buses[0].sends[0].0, 1);
        assert!((app.audio_buses[0].sends[0].1 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_add_bus_send_replaces_existing() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::AddAudioBus { name: "B".to_string() });
        app.apply(Action::AddBusSend { from_id: 0, to_id: 1, level: 0.3 });
        app.apply(Action::AddBusSend { from_id: 0, to_id: 1, level: 0.9 });
        assert_eq!(app.audio_buses[0].sends.len(), 1, "duplicate sends replaced");
        assert!((app.audio_buses[0].sends[0].1 - 0.9).abs() < 1e-5);
    }

    #[test]
    fn test_remove_bus_send() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::AddAudioBus { name: "B".to_string() });
        app.apply(Action::AddAudioBus { name: "C".to_string() });
        app.apply(Action::AddBusSend { from_id: 0, to_id: 1, level: 0.5 });
        app.apply(Action::AddBusSend { from_id: 0, to_id: 2, level: 0.5 });
        app.apply(Action::RemoveBusSend { from_id: 0, to_id: 1 });
        assert_eq!(app.audio_buses[0].sends.len(), 1);
        assert_eq!(app.audio_buses[0].sends[0].0, 2);
    }

    #[test]
    fn test_set_bus_eq_clamped() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SetBusEq { bus_id: 0, low: 15.0, mid: -20.0, high: 6.0 });
        assert!((app.audio_buses[0].eq_low - 12.0).abs() < 1e-5, "low clamped to 12");
        assert!((app.audio_buses[0].eq_mid - (-12.0)).abs() < 1e-5, "mid clamped to -12");
        assert!((app.audio_buses[0].eq_high - 6.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_bus_compressor() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SetBusCompressor { bus_id: 0, threshold: -24.0, ratio: 8.0 });
        assert!((app.audio_buses[0].compressor_threshold - (-24.0)).abs() < 1e-5);
        assert!((app.audio_buses[0].compressor_ratio - 8.0).abs() < 1e-5);
    }

    #[test]
    fn test_set_bus_compressor_ratio_min_1() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::SetBusCompressor { bus_id: 0, threshold: -18.0, ratio: 0.5 });
        assert!(app.audio_buses[0].compressor_ratio >= 1.0, "ratio clamped to min 1.0");
    }

    #[test]
    fn test_set_master_volume() {
        let mut app = App::new();
        app.apply(Action::SetMasterVolume(0.8));
        assert!((app.master_volume - 0.8).abs() < 1e-5);
        app.apply(Action::SetMasterVolume(3.0));
        assert!((app.master_volume - 2.0).abs() < 1e-5, "clamped to 2.0");
        app.apply(Action::SetMasterVolume(-1.0));
        assert!((app.master_volume - 0.0).abs() < 1e-5, "clamped to 0.0");
    }

    #[test]
    fn test_set_master_pan() {
        let mut app = App::new();
        app.apply(Action::SetMasterPan(0.5));
        assert!((app.master_pan - 0.5).abs() < 1e-5);
        app.apply(Action::SetMasterPan(2.0));
        assert!((app.master_pan - 1.0).abs() < 1e-5, "clamped to 1.0");
    }

    #[test]
    fn test_bus_ids_auto_increment() {
        let mut app = App::new();
        app.apply(Action::AddAudioBus { name: "A".to_string() });
        app.apply(Action::AddAudioBus { name: "B".to_string() });
        app.apply(Action::AddAudioBus { name: "C".to_string() });
        assert_eq!(app.audio_buses[0].id, 0);
        assert_eq!(app.audio_buses[1].id, 1);
        assert_eq!(app.audio_buses[2].id, 2);
    }
}
