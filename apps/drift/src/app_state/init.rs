//! App constructor — `App::new()` lives here to keep mod.rs under 1000 lines.

use std::collections::HashMap;
use super::{
    App, DriftTool, DriftDocument, GridConfig, RulerConfig, ExportConfig, OnionSkinConfig,
    Scene, DriftCamera, PublishConfig, ExportQueue, StageSettings, ScriptConsole,
    AiBackend, BeatSyncConfig, Projection3D, AppHistory, PenToolState,
    JsRuntimeConfig, WsLivePreviewConfig, CollabSession, MultiTrackMixConfig,
    RhaiRuntimeConfig,
};

impl App {
    pub fn new() -> Self {
        let doc = DriftDocument::new();
        let out = doc.duration_frames;
        let mut app = Self {
            active_tool: DriftTool::Select,
            document: doc,
            grid: GridConfig::new(),
            rulers: RulerConfig::new(),
            layers: Vec::new(),
            layer_counter: 0,
            active_layer: None,
            keyframes: Vec::new(),
            keyframe_counter: 0,
            transforms: HashMap::new(),
            current_frame: 0,
            playing: false,
            loop_playback: false,
            in_point: 0,
            out_point: out,
            layer_rigs: Vec::new(),
            rig_bone_counter: 0,
            ik_chains: Vec::new(),
            next_chain_id: 0,
            bone_springs: Vec::new(),
            ai_motion_prompt: String::new(),
            ai_motion_results: Vec::new(),
            ai_lipsync_jobs: Vec::new(),
            ai_interpolation_queue: Vec::new(),
            export_config: ExportConfig::new(),
            export_in_progress: false,
            state_machines: Vec::new(),
            state_machine_counter: 0,
            anim_state_counter: 0,
            // Phase 2
            vector_paths: Vec::new(),
            next_path_id: 0,
            symbols: Vec::new(),
            symbol_instances: Vec::new(),
            next_symbol_id: 0,
            next_instance_id: 0,
            tweens: Vec::new(),
            next_tween_id: 0,
            onion_skin: OnionSkinConfig::new(),
            // Phase 2: Scenes — always start with one default scene.
            scenes: vec![Scene {
                id: 0,
                name: "Scene 1".to_string(),
                duration_frames: 240,
                layer_ids: Vec::new(),
            }],
            active_scene_id: 0,
            next_scene_id: 1,
            // Phase 2: Frame labels
            frame_labels: Vec::new(),
            next_label_id: 0,
            // Phase 2: Library
            library_folders: Vec::new(),
            library_search: String::new(),
            next_folder_id: 0,
            symbol_folder_map: Vec::new(),
            // Phase 3: Swap sets
            swap_sets: Vec::new(),
            next_swap_set_id: 0,
            next_swap_item_id: 0,
            // Phase 3: Mesh Warp
            mesh_warps: HashMap::new(),
            next_warp_id: 0,
            // Phase 3: Bone Weights
            bone_weights: HashMap::new(),
            weight_painting_active: false,
            active_weight_bone: None,
            weight_brush_radius: 20.0,
            // Phase 3: Easing Curves (seeded below)
            easing_curves: Vec::new(),
            next_easing_id: 0,
            keyframe_easing_map: HashMap::new(),
            // Phase 3: Audio / Lip Sync
            audio_tracks: Vec::new(),
            next_audio_track_id: 0,
            lip_sync_data: Vec::new(),
            // Phase 3: Behaviors
            behaviors: Vec::new(),
            next_behavior_id: 0,
            // Batch 6 — A: 3D Layer Transforms
            layer_3d: HashMap::new(),
            global_vanishing_point: (0.5, 0.5),
            global_projection_3d: Projection3D::Perspective,
            // Batch 6 — B: Camera
            camera: DriftCamera::new(),
            camera_keyframes: Vec::new(),
            next_camera_kf_id: 0,
            camera_enabled: false,
            // Batch 6 — C: Mocap
            mocap_imports: Vec::new(),
            next_mocap_id: 0,
            // Batch 6 — D: Publish / Export Queue
            publish_config: PublishConfig::new(),
            export_queue: ExportQueue::new(),
            // Batch 6 — E: Stage
            stage: StageSettings::new(),
            scene_properties: HashMap::new(),
            // Batch 7 — A: Scripting
            scripts: Vec::new(),
            next_script_id: 0,
            script_console: ScriptConsole::new(),
            script_execution_enabled: true,
            // Batch 7 — B: Facial Capture
            capture_sessions: Vec::new(),
            next_capture_id: 0,
            capture_mappings: Vec::new(),
            live_preview_enabled: false,
            // Batch 7 — C: AI Motion
            ai_motion_requests: Vec::new(),
            next_ai_motion_id: 0,
            ai_interpolation_requests: Vec::new(),
            next_interp_id: 0,
            ai_style_transfers: Vec::new(),
            next_style_id: 0,
            ai_model_path: None,
            ai_compute_backend: AiBackend::Cpu,
            // Batch 7 — D: Advanced Tweening
            motion_guides: Vec::new(),
            next_guide_id: 0,
            property_tweens: Vec::new(),
            next_prop_tween_id: 0,
            // Batch 7 — E: Beat Sync
            audio_markers: Vec::new(),
            next_marker_id: 0,
            beat_sync: BeatSyncConfig::new(),
            sync_groups: Vec::new(),
            next_sync_group_id: 0,
            // Batch 8 — Deformation
            pin_anchors: vec![],
            deform_layers: vec![],
            stretch_squash: vec![],
            next_anchor_id: 1,
            next_deform_id: 1,
            // Batch 8 — Masking
            layer_masks: vec![],
            clipping_groups: vec![],
            next_mask_id: 1,
            next_clip_group_id: 1,
            // Batch 8 — Text Layers / SVG Import
            text_layers: vec![],
            svg_import_jobs: vec![],
            next_text_layer_id: 1,
            next_svg_job_id: 1,
            // Batch 8 — Lottie
            lottie_export_config: None,
            lottie_exporting: false,
            lottie_import_jobs: Vec::new(),
            next_lottie_job_id: 1,
            // Batch 8 — Layer Blend Modes
            layer_blend_configs: Vec::new(),
            // Batch 8 — Plugin API
            loaded_plugins: Vec::new(),
            extension_panels: Vec::new(),
            next_ext_panel_id: 1,
            // History + document lifecycle
            history: AppHistory::default(),
            document_path: None,
            document_dirty: false,
            // Batch 8
            sm_runtimes: Vec::new(),
            export_presets: Vec::new(),
            next_preset_id: 1,
            render_batches: Vec::new(),
            next_batch_id: 1,
            next_batch_job_id: 1,
            // Batch 8: AI Motion Extensions
            motion_smooth_jobs: Vec::new(),
            next_ms_job_id: 1,
            ease_suggest_jobs: Vec::new(),
            next_ease_job_id: 1,
            inbetween_jobs: Vec::new(),
            next_inbetween_id: 1,
            expr_transfer_jobs: Vec::new(),
            next_expr_transfer_id: 1,
            mfv_jobs: Vec::new(),
            next_mfv_id: 1,
            char_pack_exports: Vec::new(),
            next_char_pack_id: 1,
            // Batch 8 — Drawing Tools
            pen_tool: PenToolState::default(),
            pencil_strokes: Vec::new(),
            next_pencil_id: 1,
            active_pencil_stroke: None,
            selection_gizmo: None,
            // Batch 8 — Nested Timelines
            nested_timelines: Vec::new(),
            next_nested_timeline_id: 1,
            // Batch 8: ONNX inference stubs
            drift_onnx_models: Vec::new(),
            onnx_registry: super::ModelRegistry::new(),
            animatediff_jobs: Vec::new(),
            next_animatediff_id: 1,
            filmrife_jobs: Vec::new(),
            next_filmrife_id: 1,
            phoneme_jobs: Vec::new(),
            next_phoneme_job_id: 1,
            style_jobs: Vec::new(),
            next_style_job_id: 1,
            bg_gen_jobs: Vec::new(),
            next_bg_gen_id: 1,
            ai_script_jobs: Vec::new(),
            next_ai_script_id: 1,
            // Batch 8: Lottie / export / web / collab
            lottie_builds: Vec::new(),
            next_lottie_build_id: 1,
            lottie_imports: Vec::new(),
            next_lottie_import_id: 1,
            media_export_jobs: Vec::new(),
            next_media_export_id: 1,
            js_runtime: JsRuntimeConfig::default(),
            web_publish_jobs: Vec::new(),
            next_web_publish_id: 1,
            ws_live_preview: WsLivePreviewConfig::default(),
            collab_session: CollabSession::default(),
            // Batch 8: Audio ops, waveform peaks, mix config, Rhai runtime, sync markers, AS3
            audio_clip_ops: Vec::new(),
            waveform_peaks: Vec::new(),
            mix_config: MultiTrackMixConfig::default(),
            rhai_runtime: RhaiRuntimeConfig::default(),
            audio_sync_markers: Vec::new(),
            next_sync_marker_id: 0,
            as3_jobs: Vec::new(),
            next_as3_job_id: 0,
        };
        app.seed_easing_curves();
        app
    }
}
