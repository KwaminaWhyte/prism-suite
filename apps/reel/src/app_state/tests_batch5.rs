#[cfg(test)]
mod tests_batch5 {
    use crate::app_state::{App, Action};
    use crate::app_state::reel_project::{
        AudioCodecB5, BinColor, ExportCategory, ExportContainer, ExportPresetB5,
        HistogramChannel, ParadeType, ScopeColorspace, ScopeKind, ScopeLayout,
        VectorscopeType, VideoCodecB5, WaveformType,
    };
    use crate::app_state::timeline::{CubeDirection, PagePeelDirection, SlideDirection, TransitionKind, Transition};

    // ==================== A. Lumetri Scopes ====================

    #[test]
    fn test_toggle_scopes_panel() {
        let mut app = App::new();
        assert!(!app.scopes_config.enabled);
        app.apply(Action::ToggleScopesPanel);
        assert!(app.scopes_config.enabled);
        app.apply(Action::ToggleScopesPanel);
        assert!(!app.scopes_config.enabled);
    }

    #[test]
    fn test_set_scope_kind() {
        let mut app = App::new();
        app.apply(Action::SetScopeKind(ScopeKind::Histogram));
        assert_eq!(app.scopes_config.scope_kind, ScopeKind::Histogram);
        app.apply(Action::SetScopeKind(ScopeKind::All));
        assert_eq!(app.scopes_config.scope_kind, ScopeKind::All);
    }

    #[test]
    fn test_set_scope_layout() {
        let mut app = App::new();
        app.apply(Action::SetScopeLayout(ScopeLayout::FourUp));
        assert_eq!(app.scopes_config.layout, ScopeLayout::FourUp);
    }

    #[test]
    fn test_set_waveform_type() {
        let mut app = App::new();
        app.apply(Action::SetWaveformType(WaveformType::Rgb));
        assert_eq!(app.scopes_config.waveform_type, WaveformType::Rgb);
    }

    #[test]
    fn test_set_scope_intensity_clamp() {
        let mut app = App::new();
        // Below min → clamped to 10
        app.apply(Action::SetScopeIntensity(0.0));
        assert_eq!(app.scopes_config.intensity, 10.0);
        // Above max → clamped to 300
        app.apply(Action::SetScopeIntensity(999.0));
        assert_eq!(app.scopes_config.intensity, 300.0);
        // Normal
        app.apply(Action::SetScopeIntensity(150.0));
        assert_eq!(app.scopes_config.intensity, 150.0);
    }

    #[test]
    fn test_set_scope_colorspace() {
        let mut app = App::new();
        app.apply(Action::SetScopeColorspace(ScopeColorspace::Rec2020));
        assert_eq!(app.scopes_config.colorspace, ScopeColorspace::Rec2020);
    }

    #[test]
    fn test_set_scope_show_clipping() {
        let mut app = App::new();
        app.apply(Action::SetScopeShowClipping(true));
        assert!(app.scopes_config.show_clipping);
    }

    #[test]
    fn test_set_histogram_channel() {
        let mut app = App::new();
        app.apply(Action::SetHistogramChannel(HistogramChannel::Blue));
        assert_eq!(app.scopes_config.histogram_channel, HistogramChannel::Blue);
    }

    #[test]
    fn test_scopes_default_intensity() {
        let app = App::new();
        assert_eq!(app.scopes_config.intensity, 75.0);
    }

    // ==================== B. Project Bins ====================

    #[test]
    fn test_create_bin() {
        let mut app = App::new();
        let initial = app.project_bins.len();
        app.apply(Action::CreateBin { name: "Interviews".into(), parent_id: None });
        assert_eq!(app.project_bins.len(), initial + 1);
        assert_eq!(app.project_bins.last().unwrap().name, "Interviews");
    }

    #[test]
    fn test_rename_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Old Name".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        app.apply(Action::RenameBin { bin_id, name: "New Name".into() });
        let bin = app.project_bins.iter().find(|b| b.id == bin_id).unwrap();
        assert_eq!(bin.name, "New Name");
    }

    #[test]
    fn test_delete_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Temp".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        let before = app.project_bins.len();
        app.apply(Action::DeleteBin { bin_id });
        assert_eq!(app.project_bins.len(), before - 1);
        assert!(!app.project_bins.iter().any(|b| b.id == bin_id));
    }

    #[test]
    fn test_set_bin_color() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Colored".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        app.apply(Action::SetBinColor { bin_id, color: BinColor::Red });
        let bin = app.project_bins.iter().find(|b| b.id == bin_id).unwrap();
        assert_eq!(bin.color_label, BinColor::Red);
    }

    #[test]
    fn test_toggle_bin_expanded() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Toggle".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        let initial_expanded = app.project_bins.last().unwrap().expanded;
        app.apply(Action::ToggleBinExpanded { bin_id });
        let new_expanded = app.project_bins.iter().find(|b| b.id == bin_id).unwrap().expanded;
        assert_ne!(initial_expanded, new_expanded);
    }

    #[test]
    fn test_import_media_no_bin() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/footage/clip.mp4".into(), bin_id: None });
        assert_eq!(app.media_items.len(), 1);
        assert_eq!(app.media_items[0].name, "clip.mp4");
    }

    #[test]
    fn test_import_media_to_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Bin".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        app.apply(Action::ImportMedia2 { path: "/footage/scene.mp4".into(), bin_id: Some(bin_id) });
        let item_id = app.media_items[0].id;
        let bin = app.project_bins.iter().find(|b| b.id == bin_id).unwrap();
        assert!(bin.item_ids.contains(&item_id));
    }

    #[test]
    fn test_remove_media() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/footage/rm.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::RemoveMedia { item_id });
        assert!(app.media_items.is_empty());
    }

    #[test]
    fn test_move_media_to_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Dest".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        app.apply(Action::ImportMedia2 { path: "/footage/move.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::MoveMediaToBin { item_id, bin_id: Some(bin_id) });
        let bin = app.project_bins.iter().find(|b| b.id == bin_id).unwrap();
        assert!(bin.item_ids.contains(&item_id));
    }

    #[test]
    fn test_relink_media() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/old/path.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::SetMediaOffline { item_id, offline: true });
        assert!(app.media_items[0].offline);
        app.apply(Action::RelinkMedia { item_id, new_path: "/new/path.mp4".into() });
        let item = app.media_items.iter().find(|m| m.id == item_id).unwrap();
        assert_eq!(item.path, "/new/path.mp4");
        assert!(!item.offline);
    }

    #[test]
    fn test_attach_detach_proxy() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/footage/hi.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::AttachMediaProxy { item_id, proxy_path: "/proxies/hi_proxy.mp4".into() });
        assert!(app.media_items[0].proxy_path.is_some());
        app.apply(Action::DetachMediaProxy { item_id });
        assert!(app.media_items[0].proxy_path.is_none());
    }

    #[test]
    fn test_set_media_log_note() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/footage/note.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::SetMediaLogNote { item_id, note: "Best take".into() });
        assert_eq!(app.media_items[0].log_note, "Best take");
    }

    #[test]
    fn test_set_project_search() {
        let mut app = App::new();
        app.apply(Action::SetProjectSearch("interview".into()));
        assert_eq!(app.project_search_query, "interview");
    }

    // ==================== C. Transitions ====================

    #[test]
    fn test_add_zoom_transition() {
        let mut app = App::new();
        // No clips/cuts means the add_transition stub is a no-op — no panic is the goal.
        app.apply(Action::AddZoomTransition { clip_id: 0, grow: true });
    }

    #[test]
    fn test_add_page_peel_transition() {
        let mut app = App::new();
        app.apply(Action::AddPagePeelTransition {
            clip_id: 0,
            direction: PagePeelDirection::TopLeft,
        });
    }

    #[test]
    fn test_add_cube_transition() {
        let mut app = App::new();
        app.apply(Action::AddCubeTransition { clip_id: 0, direction: CubeDirection::Left });
    }

    #[test]
    fn test_add_wipe_transition() {
        let mut app = App::new();
        app.apply(Action::AddWipeTransition {
            clip_id: 0,
            direction: SlideDirection::Left,
            duration_s: 1.0,
        });
    }

    #[test]
    fn test_add_dip_transition() {
        let mut app = App::new();
        app.apply(Action::AddDipTransition { clip_id: 0, color: "#000000".into() });
    }

    #[test]
    fn test_transition_kind_labels() {
        // All new TransitionKind variants must have non-empty labels.
        let kinds: &[TransitionKind] = &[
            TransitionKind::DipToBlack,
            TransitionKind::DipToWhite,
            TransitionKind::AdditiveDissolve,
            TransitionKind::NonAdditiveDissolve,
            TransitionKind::RandomInvert,
            TransitionKind::Slide(SlideDirection::Left),
            TransitionKind::Zoom { grow: true },
            TransitionKind::PagePeel { direction: PagePeelDirection::TopLeft, softness: 0.1 },
            TransitionKind::Cube { direction: CubeDirection::Left, lighting: false },
        ];
        for k in kinds {
            assert!(!k.label().is_empty(), "label was empty for {:?}", k);
        }
    }

    #[test]
    fn test_transition_weights_new_kinds() {
        // Slide transition: weights at center point (p=0.5) should be (1,0) or (0,1).
        let t = Transition {
            kind: TransitionKind::Slide(SlideDirection::Left),
            from: 0, to: 1,
            center: 5.0,
            duration: 2.0,
        };
        let (a, b, overlay) = t.weights(5.0); // progress = 0.5
        assert!(a >= 0.0 && b >= 0.0, "weights must be non-negative");
        assert!(overlay.is_none());
        // At the very start: from=1, to=0
        let (a0, b0, _) = t.weights(4.0); // progress = 0.0
        assert_eq!(a0, 1.0);
        assert_eq!(b0, 0.0);
        // At the very end: from=0, to=1
        let (a1, b1, _) = t.weights(6.0); // progress = 1.0
        assert_eq!(a1, 0.0);
        assert_eq!(b1, 1.0);
    }

    #[test]
    fn test_dip_to_black_weights() {
        let t = Transition {
            kind: TransitionKind::DipToBlack,
            from: 0, to: 1,
            center: 5.0,
            duration: 2.0,
        };
        // p=0.25 → first half → from=1, overlay black at 0.5
        let (a, b, overlay) = t.weights(4.5);
        assert_eq!(a, 1.0);
        assert_eq!(b, 0.0);
        assert!(overlay.is_some());
        let (color, alpha) = overlay.unwrap();
        assert_eq!(color, [0.0f32, 0.0, 0.0, 1.0]);
        assert!((alpha - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_dip_to_white_weights() {
        let t = Transition {
            kind: TransitionKind::DipToWhite,
            from: 0, to: 1,
            center: 5.0,
            duration: 2.0,
        };
        let (a, _b, overlay) = t.weights(4.5);
        assert_eq!(a, 1.0);
        let (color, _) = overlay.unwrap();
        assert_eq!(color, [1.0f32, 1.0, 1.0, 1.0]);
    }

    // ==================== D. Export Presets B5 ====================

    #[test]
    fn test_builtin_presets_exist_on_new() {
        let app = App::new();
        assert_eq!(app.export_presets_b5.len(), 7);
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("YouTube 1080p")));
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("YouTube 4K")));
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("Twitter")));
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("ProRes 422")));
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("GIF")));
        assert!(app.export_presets_b5.iter().any(|p| p.name.contains("MP3")));
    }

    #[test]
    fn test_select_export_preset_b5() {
        let mut app = App::new();
        app.apply(Action::SelectExportPresetB5 { preset_id: 2 });
        assert_eq!(app.active_export_preset_b5, Some(2));
    }

    #[test]
    fn test_add_custom_export_preset() {
        let mut app = App::new();
        let before = app.export_presets_b5.len();
        let preset = ExportPresetB5 {
            id: 999, // will be overwritten
            name: "Custom 4K H265".into(),
            category: ExportCategory::H265,
            container: ExportContainer::Mkv,
            video_codec: VideoCodecB5::H265,
            audio_codec: AudioCodecB5::Aac,
            width: 3840, height: 2160, frame_rate: 24.0,
            video_bitrate_kbps: 20000, audio_bitrate_kbps: 320, audio_sample_rate: 48000,
            two_pass: true, hardware_encode: false, match_source: false,
        };
        app.apply(Action::AddCustomExportPresetB5 { preset });
        assert_eq!(app.export_presets_b5.len(), before + 1);
        assert_eq!(app.export_presets_b5.last().unwrap().name, "Custom 4K H265");
    }

    #[test]
    fn test_duplicate_export_preset() {
        let mut app = App::new();
        let original_id = app.export_presets_b5[0].id;
        let before = app.export_presets_b5.len();
        app.apply(Action::DuplicateExportPresetB5 { preset_id: original_id });
        assert_eq!(app.export_presets_b5.len(), before + 1);
        let dup = app.export_presets_b5.last().unwrap();
        assert!(dup.name.contains("copy"));
        assert_ne!(dup.id, original_id);
    }

    #[test]
    fn test_delete_custom_export_preset() {
        let mut app = App::new();
        let preset = ExportPresetB5 {
            id: 999,
            name: "Custom".into(),
            category: ExportCategory::Custom,
            container: ExportContainer::Mp4,
            video_codec: VideoCodecB5::H264,
            audio_codec: AudioCodecB5::Aac,
            width: 1920, height: 1080, frame_rate: 30.0,
            video_bitrate_kbps: 8000, audio_bitrate_kbps: 128, audio_sample_rate: 48000,
            two_pass: false, hardware_encode: false, match_source: false,
        };
        app.apply(Action::AddCustomExportPresetB5 { preset });
        let custom_id = app.export_presets_b5.last().unwrap().id;
        let before = app.export_presets_b5.len();
        app.apply(Action::DeleteCustomExportPresetB5 { preset_id: custom_id });
        assert_eq!(app.export_presets_b5.len(), before - 1);
    }

    #[test]
    fn test_cannot_delete_builtin_preset() {
        let mut app = App::new();
        let builtin_id = 1; // YouTube 1080p
        let before = app.export_presets_b5.len();
        app.apply(Action::DeleteCustomExportPresetB5 { preset_id: builtin_id });
        assert_eq!(app.export_presets_b5.len(), before); // unchanged
    }

    #[test]
    fn test_set_export_width_height_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportWidthB5(2560));
        app.apply(Action::SetExportHeightB5(1440));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert_eq!(p.width, 2560);
        assert_eq!(p.height, 1440);
    }

    #[test]
    fn test_set_export_two_pass_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportTwoPassB5(true));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert!(p.two_pass);
    }

    #[test]
    fn test_set_export_hardware_encode_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportHardwareEncodeB5(true));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert!(p.hardware_encode);
    }

    #[test]
    fn test_set_export_container_and_codec_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportContainerB5(ExportContainer::Mkv));
        app.apply(Action::SetExportVideoCodecB5(VideoCodecB5::H265));
        app.apply(Action::SetExportAudioCodecB5(AudioCodecB5::Ac3));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert_eq!(p.container, ExportContainer::Mkv);
        assert_eq!(p.video_codec, VideoCodecB5::H265);
        assert_eq!(p.audio_codec, AudioCodecB5::Ac3);
    }

    // ==================== E. Sequence Settings B5 ====================

    #[test]
    fn test_new_sequence_b5() {
        let mut app = App::new();
        let initial = app.sequences_b5.len();
        app.apply(Action::NewSequenceB5 { name: "Seq 02".into(), width: 1280, height: 720, frame_rate: 60.0 });
        assert_eq!(app.sequences_b5.len(), initial + 1);
        let seq = app.sequences_b5.last().unwrap();
        assert_eq!(seq.name, "Seq 02");
        assert_eq!(seq.width, 1280);
        assert_eq!(seq.height, 720);
        assert_eq!(seq.frame_rate, 60.0);
        assert_eq!(app.active_sequence_id, seq.id);
    }

    #[test]
    fn test_duplicate_sequence_b5() {
        let mut app = App::new();
        let orig_id = app.sequences_b5[0].id;
        let before = app.sequences_b5.len();
        app.apply(Action::DuplicateSequenceB5 { sequence_id: orig_id });
        assert_eq!(app.sequences_b5.len(), before + 1);
        let dup = app.sequences_b5.last().unwrap();
        assert!(dup.name.contains("copy"));
        assert_ne!(dup.id, orig_id);
    }

    #[test]
    fn test_delete_sequence_b5_blocked_when_only_one() {
        let mut app = App::new();
        assert_eq!(app.sequences_b5.len(), 1);
        let seq_id = app.sequences_b5[0].id;
        app.apply(Action::DeleteSequenceB5 { sequence_id: seq_id });
        assert_eq!(app.sequences_b5.len(), 1); // can't delete last sequence
    }

    #[test]
    fn test_delete_sequence_b5() {
        let mut app = App::new();
        app.apply(Action::NewSequenceB5 { name: "Extra".into(), width: 1920, height: 1080, frame_rate: 29.97 });
        assert_eq!(app.sequences_b5.len(), 2);
        let to_delete = app.sequences_b5[1].id;
        app.apply(Action::DeleteSequenceB5 { sequence_id: to_delete });
        assert_eq!(app.sequences_b5.len(), 1);
        assert!(!app.sequences_b5.iter().any(|s| s.id == to_delete));
    }

    #[test]
    fn test_set_active_sequence_b5() {
        let mut app = App::new();
        app.apply(Action::NewSequenceB5 { name: "B".into(), width: 1920, height: 1080, frame_rate: 25.0 });
        let new_id = app.sequences_b5.last().unwrap().id;
        app.apply(Action::SetActiveSequenceB5 { sequence_id: new_id });
        assert_eq!(app.active_sequence_id, new_id);
    }

    #[test]
    fn test_update_sequence_settings_b5() {
        let mut app = App::new();
        let seq_id = app.sequences_b5[0].id;
        app.apply(Action::UpdateSequenceSettingsB5 {
            sequence_id: seq_id,
            width: Some(2560),
            height: Some(1440),
            frame_rate: Some(50.0),
        });
        let seq = app.sequences_b5.iter().find(|s| s.id == seq_id).unwrap();
        assert_eq!(seq.width, 2560);
        assert_eq!(seq.height, 1440);
        assert_eq!(seq.frame_rate, 50.0);
        assert_eq!(seq.timebase, 50.0);
    }

    #[test]
    fn test_update_sequence_partial_settings_b5() {
        let mut app = App::new();
        let seq_id = app.sequences_b5[0].id;
        let original_frame_rate = app.sequences_b5[0].frame_rate;
        app.apply(Action::UpdateSequenceSettingsB5 {
            sequence_id: seq_id,
            width: Some(4096),
            height: None,
            frame_rate: None,
        });
        let seq = app.sequences_b5.iter().find(|s| s.id == seq_id).unwrap();
        assert_eq!(seq.width, 4096);
        assert_eq!(seq.frame_rate, original_frame_rate); // unchanged
    }

    #[test]
    fn test_delete_seq_resets_active() {
        let mut app = App::new();
        let orig_id = app.sequences_b5[0].id;
        app.apply(Action::NewSequenceB5 { name: "Second".into(), width: 1920, height: 1080, frame_rate: 30.0 });
        let second_id = app.sequences_b5.last().unwrap().id;
        app.apply(Action::SetActiveSequenceB5 { sequence_id: second_id });
        assert_eq!(app.active_sequence_id, second_id);
        app.apply(Action::DeleteSequenceB5 { sequence_id: second_id });
        // Active should now be the first sequence
        assert_eq!(app.active_sequence_id, orig_id);
    }

    #[test]
    fn test_initial_sequence_count() {
        let app = App::new();
        assert_eq!(app.sequences_b5.len(), 1);
        assert_eq!(app.sequences_b5[0].name, "Sequence 01");
    }

    #[test]
    fn test_rename_sequence() {
        let mut app = App::new();
        let id = app.sequences_b5[0].id;
        app.apply(Action::RenameSequence { sequence_id: id, name: "Main Edit".into() });
        assert_eq!(app.sequences_b5[0].name, "Main Edit");
        // Whitespace is trimmed.
        app.apply(Action::RenameSequence { sequence_id: id, name: "  Trimmed  ".into() });
        assert_eq!(app.sequences_b5[0].name, "Trimmed");
        // Blank names are ignored.
        app.apply(Action::RenameSequence { sequence_id: id, name: "   ".into() });
        assert_eq!(app.sequences_b5[0].name, "Trimmed");
        // Unknown id is a no-op (does not panic).
        app.apply(Action::RenameSequence { sequence_id: 99999, name: "Nope".into() });
        assert_eq!(app.sequences_b5[0].name, "Trimmed");
    }

    #[test]
    fn test_parade_and_vectorscope_type() {
        let mut app = App::new();
        app.apply(Action::SetParadeType(ParadeType::Yuv));
        assert_eq!(app.scopes_config.parade_type, ParadeType::Yuv);
        app.apply(Action::SetVectorscopeType(VectorscopeType::YuvDiamond));
        assert_eq!(app.scopes_config.vectorscope_type, VectorscopeType::YuvDiamond);
    }

    #[test]
    fn test_media_label_and_offline() {
        let mut app = App::new();
        app.apply(Action::ImportMedia2 { path: "/clips/a.mp4".into(), bin_id: None });
        let item_id = app.media_items[0].id;
        app.apply(Action::SetMediaLabel { item_id, label: BinColor::Green });
        assert_eq!(app.media_items[0].label, BinColor::Green);
        app.apply(Action::SetMediaOffline { item_id, offline: true });
        assert!(app.media_items[0].offline);
    }

    #[test]
    fn test_nested_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Parent".into(), parent_id: None });
        let parent_id = app.project_bins.last().unwrap().id;
        app.apply(Action::CreateBin { name: "Child".into(), parent_id: Some(parent_id) });
        let child = app.project_bins.last().unwrap();
        assert_eq!(child.parent_id, Some(parent_id));
    }

    #[test]
    fn test_remove_media_clears_bin_refs() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "B".into(), parent_id: None });
        let bin_id = app.project_bins.last().unwrap().id;
        app.apply(Action::ImportMedia2 { path: "/x.mp4".into(), bin_id: Some(bin_id) });
        let item_id = app.media_items[0].id;
        app.apply(Action::RemoveMedia { item_id });
        let bin = app.project_bins.iter().find(|b| b.id == bin_id).unwrap();
        assert!(!bin.item_ids.contains(&item_id));
    }

    #[test]
    fn test_move_media_clears_old_bin() {
        let mut app = App::new();
        app.apply(Action::CreateBin { name: "Src".into(), parent_id: None });
        let src_id = app.project_bins.last().unwrap().id;
        app.apply(Action::CreateBin { name: "Dst".into(), parent_id: None });
        let dst_id = app.project_bins.last().unwrap().id;
        app.apply(Action::ImportMedia2 { path: "/clip.mp4".into(), bin_id: Some(src_id) });
        let item_id = app.media_items[0].id;
        app.apply(Action::MoveMediaToBin { item_id, bin_id: Some(dst_id) });
        let src = app.project_bins.iter().find(|b| b.id == src_id).unwrap();
        assert!(!src.item_ids.contains(&item_id));
        let dst = app.project_bins.iter().find(|b| b.id == dst_id).unwrap();
        assert!(dst.item_ids.contains(&item_id));
    }

    #[test]
    fn test_export_frame_rate_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportFrameRateB5(60.0));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert_eq!(p.frame_rate, 60.0);
    }

    #[test]
    fn test_export_video_bitrate_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportVideoBitrateB5(50000));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert_eq!(p.video_bitrate_kbps, 50000);
    }

    #[test]
    fn test_export_audio_bitrate_b5() {
        let mut app = App::new();
        let preset_id = app.export_presets_b5[0].id;
        app.apply(Action::SelectExportPresetB5 { preset_id });
        app.apply(Action::SetExportAudioBitrateB5(256));
        let p = app.export_presets_b5.iter().find(|p| p.id == preset_id).unwrap();
        assert_eq!(p.audio_bitrate_kbps, 256);
    }

    #[test]
    fn test_scope_kind_vectorscope() {
        let mut app = App::new();
        app.apply(Action::SetScopeKind(ScopeKind::Vectorscope));
        assert_eq!(app.scopes_config.scope_kind, ScopeKind::Vectorscope);
    }

    #[test]
    fn test_scope_kind_parade() {
        let mut app = App::new();
        app.apply(Action::SetScopeKind(ScopeKind::Parade));
        assert_eq!(app.scopes_config.scope_kind, ScopeKind::Parade);
    }

    #[test]
    fn test_waveform_type_yuv() {
        let mut app = App::new();
        app.apply(Action::SetWaveformType(WaveformType::Yuv));
        assert_eq!(app.scopes_config.waveform_type, WaveformType::Yuv);
    }

    #[test]
    fn test_scope_layout_two_up() {
        let mut app = App::new();
        app.apply(Action::SetScopeLayout(ScopeLayout::TwoUp));
        assert_eq!(app.scopes_config.layout, ScopeLayout::TwoUp);
    }

    #[test]
    fn test_set_active_sequence_invalid_ignored() {
        let mut app = App::new();
        let original_active = app.active_sequence_id;
        app.apply(Action::SetActiveSequenceB5 { sequence_id: 9999 });
        // Should not change because 9999 doesn't exist
        assert_eq!(app.active_sequence_id, original_active);
    }

    #[test]
    fn test_media_browser_path() {
        let mut app = App::new();
        app.apply(Action::SetMediaBrowserPath2("/Volumes/Media".into()));
        assert_eq!(app.media_browser_path, "/Volumes/Media");
    }
}
