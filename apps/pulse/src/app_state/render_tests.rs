//! Tests for the render-queue / export `apply_render` dispatcher, included from
//! `render.rs` via `#[path]` so the dispatch impl and its tests live in separate
//! files (workspace size rule). `super` here is the `render` module, exactly as
//! when the tests were inline.

    use super::*;

    #[test]
    fn test_brainstorm_toggle() {
        let mut app = App::new();
        assert!(!app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(!app.brainstorm.open);
    }

    #[test]
    fn test_brainstorm_generate() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 6 });
        assert_eq!(app.brainstorm.variations.len(), 6);
    }

    #[test]
    fn test_brainstorm_grid() {
        let mut app = App::new();
        app.apply(Action::SetBrainstormGrid { cols: 3, rows: 2 });
        assert_eq!(app.brainstorm.grid_cols, 3);
        assert_eq!(app.brainstorm.grid_rows, 2);
    }

    #[test]
    fn test_brainstorm_select() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 4 });
        app.apply(Action::SelectBrainstormVariation(2));
        assert!(app.brainstorm.variations[2].selected);
        assert!(!app.brainstorm.variations[0].selected);
    }

    #[test]
    fn test_brainstorm_apply() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 3 });
        app.apply(Action::ApplyBrainstormVariation(0));
        assert!(app.brainstorm.variations.is_empty());
        assert!(!app.brainstorm.open);
    }

    #[test]
    fn test_brainstorm_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetBrainstormVariationCount(0));
        assert_eq!(app.brainstorm_variation_count, 1);
        app.apply(Action::SetBrainstormVariationCount(20));
        assert_eq!(app.brainstorm_variation_count, 9);
    }

    #[test]
    fn test_brainstorm_compare() {
        let mut app = App::new();
        assert!(app.brainstorm_comparison.is_none());
        app.apply(Action::CompareBrainstormVariations { a: 1, b: 3 });
        assert_eq!(app.brainstorm_comparison, Some((1, 3)));
    }

    #[test]
    fn test_brainstorm_lock() {
        let mut app = App::new();
        assert!(app.brainstorm_locked.is_empty());
        app.apply(Action::LockBrainstormVariation(2));
        assert_eq!(app.brainstorm_locked.len(), 3);
        assert!(app.brainstorm_locked[2]);
        app.apply(Action::LockBrainstormVariation(2));
        assert!(!app.brainstorm_locked[2]);
    }

    #[test]
    fn test_pre_render_start() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        assert!(matches!(app.pre_render_status, PreRenderStatus::Rendering { .. }));
    }

    #[test]
    fn test_pre_render_progress() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::SetPreRenderProgress { frames_done: 5, total: 30 });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Rendering { frames_done: 5, total: 30 }
        );
    }

    #[test]
    fn test_pre_render_complete() {
        let mut app = App::new();
        let dir = std::path::PathBuf::from("/tmp/pulse_test_cache");
        app.apply(Action::PreRenderComplete { frame_count: 30, cache_dir: dir.clone() });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Done { frame_count: 30, cache_dir: dir }
        );
    }

    #[test]
    fn test_pre_render_clear() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::ClearPreRenderCache);
        assert_eq!(app.pre_render_status, PreRenderStatus::NotStarted);
        assert!(app.pre_render_cache_dir.is_none());
        assert!(!app.use_pre_render);
    }

    #[test]
    fn test_pre_render_toggle_use() {
        let mut app = App::new();
        assert!(!app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(!app.use_pre_render);
    }

    #[test]
    fn test_audio_start_freq_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioStartFreq(0.0));
        assert!((app.audio_spectrum_config.start_freq - 1.0).abs() < 1e-3);
        app.apply(Action::SetAudioStartFreq(30000.0));
        assert!((app.audio_spectrum_config.start_freq - 22000.0).abs() < 1e-3);
    }

    #[test]
    fn test_audio_frequency_bands_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioFrequencyBands(0));
        assert_eq!(app.audio_spectrum_config.frequency_bands, 2);
        app.apply(Action::SetAudioFrequencyBands(9999));
        assert_eq!(app.audio_spectrum_config.frequency_bands, 1024);
    }

    #[test]
    fn test_audio_thickness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioThickness(0.0));
        assert!((app.audio_spectrum_config.thickness - 0.1).abs() < 1e-3);
        app.apply(Action::SetAudioThickness(200.0));
        assert!((app.audio_spectrum_config.thickness - 100.0).abs() < 1e-3);
    }

    #[test]
    fn test_apply_audio_effect_sets_layer() {
        let mut app = App::new();
        assert!(app.audio_spectrum_layer.is_none());
        app.apply(Action::ApplyAudioSpectrumEffect { layer_id: 3 });
        assert_eq!(app.audio_spectrum_layer, Some(3));
    }

    #[test]
    fn test_add_remove_render_item() {
        let mut app = App::new();
        assert!(app.render_queue_items.is_empty());
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert_eq!(app.render_queue_items.len(), 1);
        app.apply(Action::RemoveRenderQueueItem(0));
        assert!(app.render_queue_items.is_empty());
    }

    #[test]
    fn test_start_stop_render_queue() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert!(!app.render_in_progress);
        app.apply(Action::StartRenderQueue);
        assert!(app.render_in_progress);
        assert_eq!(app.render_active_idx, Some(0));
        app.apply(Action::StopRenderQueue);
        assert!(!app.render_in_progress);
        assert_eq!(app.render_active_idx, None);
    }

    #[test]
    fn test_render_item_complete_sets_done() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Queued);
        app.apply(Action::RenderQueueItemComplete { idx: 0 });
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Done);
        assert!((app.render_queue_items[0].progress - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_skip_render_item() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        app.apply(Action::SkipRenderItem(0));
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Skipped);
    }

    #[test]
    fn test_duplicate_render_item_oob_no_panic() {
        let mut app = App::new();
        app.apply(Action::DuplicateRenderItem(99));
        assert!(app.render_queue_items.is_empty());
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        app.apply(Action::DuplicateRenderItem(0));
        assert_eq!(app.render_queue_items.len(), 2);
    }

    #[test]
    fn test_set_render_output_path() {
        let mut app = App::new();
        // Default seeded path.
        assert_eq!(app.render_output_path, std::path::PathBuf::from("output.mp4"));
        app.apply(Action::SetRenderOutputPath(std::path::PathBuf::from(
            "/exports/final_v2.mov",
        )));
        assert_eq!(
            app.render_output_path,
            std::path::PathBuf::from("/exports/final_v2.mov")
        );
    }
