//! The `App::new()` constructor: boots the canvas host and seeds all tool /
//! brush / view state to the egui app's defaults.
//!
//! Split out of `app_state/mod.rs` as a pure mechanical refactor (no behavior
//! change). The only edit is rewriting the constructor's `self::<submodule>`
//! paths to `super::<submodule>` since it now sits one module level deeper.

use super::*;

impl App {
    /// Build the shared state: boot the host (which also yields the document)
    /// and seed tool/brush/view to the egui app's defaults.
    pub fn new() -> Self {
        let (host, doc) = CanvasHost::new();
        let (nav_w, nav_h) = (host.doc_w.max(1) as f32, host.doc_h.max(1) as f32);
        Self {
            host,
            doc,
            // egui default tool is Brush.
            active: Tool::Brush,
            brush: Brush::default(),
            swatches: Vec::new(),
            view: ViewTransform::default(),
            stroke_last: None,
            stroke_residual: 0.0,
            sel_drag_start: None,
            lasso_points: Vec::new(),
            sel_base: Vec::new(),
            sel_mode: CombineMode::Replace,
            xform_drag_start: None,
            xform_translate: [0.0, 0.0],
            xform_scale: 1.0,
            xform_rotation_deg: 0.0,
            xform_skew_x_deg: 0.0,
            xform_skew_y_deg: 0.0,
            grad_drag_start: None,
            shape_drag_start: None,
            last_drag: None,
            // egui defaults (`pigment-app/src/app/mod.rs`).
            fill_tolerance: 0.1,
            fill_contiguous: true,
            gradient_dither: true,
            clone_source: None,
            clone_offset: [0.0, 0.0],
            text_edit: None,
            text_size: 48.0,
            selection_generation: 0,
            masked_layers: HashSet::new(),
            edit_mask: false,
            channel_visibility: [true; 4],
            bg_color: [1.0, 1.0, 1.0, 1.0],
            smart_objects: HashMap::new(),
            history_labels: Vec::new(),
            pen_path: Vec::new(),
            pen_closed: false,
            dragging_curve_point: None,
            hovered_curve_point: None,
            curves_canvas_bounds: None,
            // Wave 11
            layer_styles: HashMap::new(),
            style_panel_open: false,
            style_panel_layer: None,
            clipping_masks: HashSet::new(),
            fg_hue: 0.0,
            fg_saturation: 0.0,
            fg_value: 0.0,
            status_message: None,
            // Wave 12
            dodge_size: 20.0,
            dodge_strength: 0.3,
            smudge_strength: 0.5,
            liquify_mode: LiquifyMode::Warp,
            crop_rect: None,
            // Wave 13
            guides_h: Vec::new(),
            guides_v: Vec::new(),
            guides_visible: true,
            fill_layers: HashMap::new(),
            recent_files: Vec::new(),
            canvas_rotation_deg: 0.0,
            color_mode: ColorMode::Rgb,
            active_menu: None,
            cursor_blink_on: false,
            cursor_blink_tick: 0,
            snap_to_grid: false,
            grid_size: 16.0,
            slices: Vec::new(),
            next_slice_id: 0,
            slice_drag_start: None,
            smart_filters: HashMap::new(),
            filter_gallery_open: false,
            camera_raw_open: false,
            camera_raw_params: {
                let mut m = HashMap::new();
                for k in [
                    // Basic
                    "exposure","contrast","highlights","shadows","whites","blacks",
                    "clarity","vibrance","saturation",
                    // Detail
                    "sharpening_amount","sharpening_radius","noise_luminance",
                    // HSL hue per channel
                    "hue_red","hue_orange","hue_yellow","hue_green","hue_aqua","hue_blue","hue_purple","hue_magenta",
                    // HSL saturation per channel
                    "sat_red","sat_orange","sat_yellow","sat_green","sat_aqua","sat_blue","sat_purple","sat_magenta",
                    // HSL luminance per channel
                    "lum_red","lum_orange","lum_yellow","lum_green","lum_aqua","lum_blue","lum_purple","lum_magenta",
                ] {
                    m.insert(k, 0.0f32);
                }
                m
            },
            color_profile: ColorProfile::Srgb,
            soft_proof: SoftProofMode::Off,
            export_presets: vec![ExportPreset::jpeg_90(), ExportPreset::png_lossless()],
            histogram_channel: HistogramChannel::Luminosity,
            panel_detached: HashMap::new(),
            panel_positions: HashMap::new(),
            panel_visibility: {
                let mut m = HashMap::new();
                for name in ["Layers", "Color", "Adjustments", "Histogram", "Channels", "History", "Tool Options"] {
                    m.insert(name.to_string(), true);
                }
                m
            },
            // Load saved preferences on startup; non-fatal if file is absent.
            prefs: {
                let p = AppPrefs::load();
                log::info!("prefs loaded from {:?}", AppPrefs::path());
                p
            },
            lens_barrel: 0.0,
            lens_pincushion: 0.0,
            lens_vignette: 0.0,
            lens_correction_open: false,
            camera_raw_section_basic: true,
            camera_raw_section_detail: false,
            camera_raw_section_hsl: false,
            // Batch 4: Autosave
            autosave_interval_secs: 300,
            last_autosave: None,
            autosave_restore_pending: autosave_path().map(|p| p.exists()).unwrap_or(false),
            // Batch 4: Heal
            heal_radius: 20,
            heal_mode: HealMode::Normal,
            // Batch 4: Print
            show_print_dialog: false,
            print_paper_size: "A4".to_string(),
            print_landscape: false,
            print_scale_mode: "Fit to Page".to_string(),
            print_color_space: "sRGB".to_string(),
            // Batch 4: Plugins
            plugin_registry: crate::plugin::PluginRegistry::default(),
            plugin_params: "{}".to_string(),
            // Batch 4: Snapshots
            snapshots: Vec::new(),
            // Batch 5: Layer Comps
            layer_comps: Vec::new(),
            layer_comps_panel_open: false,
            active_comp_idx: None,
            // Batch 5: Pattern Stamp
            pattern_library: Vec::new(),
            active_pattern_idx: None,
            pattern_stamp_scale: 1.0,
            pattern_stamp_aligned: true,
            // Batch 5: Match Color
            match_color_dialog_open: false,
            match_color_source: None,
            match_color_fade: 100.0,
            // Batch 5: Vanishing Point
            vanishing_planes: Vec::new(),
            active_vanishing_plane: None,
            vanishing_tool_mode: VanishingToolMode::DefiningPlane,
            vanishing_point_open: false,
            // Batch 5: Focus Area
            focus_area_threshold: 0.5,
            focus_area_sensitivity: 0.5,
            // Batch 6: Select Subject
            select_subject_threshold: 0.5,
            select_subject_feather: 1.0,
            // Batch 6: Artboards
            artboards: Vec::new(),
            active_artboard: None,
            artboards_panel_open: false,
            next_artboard_id: 1,
            // Batch 6: Apply Image
            apply_image_dialog_open: false,
            apply_image_params: ApplyImageParams {
                source_layer: LayerId(0),
                source_channel: ApplyImageChannel::Rgb,
                target_layer: LayerId(0),
                blend_mode: BlendMode::Normal,
                opacity: 1.0,
                invert_source: false,
                mask_layer: None,
            },
            // Batch 6: Soft Proof (expanded)
            soft_proof_enabled: false,
            soft_proof_settings: SoftProofSettings::default(),
            // Batch 7: Alpha Channels
            alpha_channels: Vec::new(),
            // Batch 7: Blend If
            blend_if: std::collections::HashMap::new(),
            // Batch 7: Spot Heal / Red Eye
            spot_heal_mode: SpotHealMode::ContentAware,
            spot_heal_radius: 20.0,
            last_spot_heal: None,
            last_red_eye: None,
            // Batch 4 extended: HDR Tone Mapping
            tone_map_preview: false,
            last_tone_map: None,
            // Batch 4 extended: Neural Filters
            neural_filters: Vec::new(),
            neural_filters_panel_open: false,
            last_neural_apply_count: 0,
            // Batch 4 extended: Layer Group depth
            collapsed_groups: std::collections::HashSet::new(),
            last_duplicated_group: None,
            // Batch 4 extended: Print Layout
            print_layout: PrintLayout::default(),
            print_preview_page: 0,
            // Batch 5 (new): Content-Aware Crop
            ca_crop_config: ContentAwareCropConfig::default(),
            last_ca_crop_rect: None,
            // Batch 5 (new): Sky Replacement
            sky_replace_config: SkyReplaceConfig::default(),
            sky_replace_panel_open: false,
            sky_replaced: false,
            // Batch 5 (new): Liquify Depth
            liquify_tool: LiquifyTool::Forward,
            liquify_brush_size: 100.0,
            liquify_brush_pressure: 50.0,
            liquify_brush_density: 50.0,
            liquify_strokes: Vec::new(),
            liquify_frozen_mask: Vec::new(),
            liquify_show_mesh: false,
            liquify_mesh: LiquifyMesh { width: 0, height: 0, subdivisions: 4 },
            liquify_smart_radius: false,
            liquify_session: None,
            // Batch 5 (new): Select Subject (AI stub)
            select_subject_mode: SelectSubjectMode::Device,
            last_select_subject: None,
            select_subject_refine: false,
            select_and_mask_open: false,
            // Batch 6: Layer Effects Suite
            layer_effects: std::collections::HashMap::new(),
            fx_panel_open: false,
            fx_target_layer: None,
            fx_clipboard: None,
            // Batch 6: Match Color (new)
            match_color_config: MatchColorConfig::default(),
            last_match_color_applied: false,
            // Batch 6: Camera Raw Filter (new)
            camera_raw_config: CameraRawConfig::default(),
            camera_raw_panel_open: false,
            camera_raw_applied: false,
            // Batch 6: HDR Merge
            hdr_merge_config: HdrMergeConfig::default(),
            hdr_merge_panel_open: false,
            hdr_merge_result: None,
            // New Feature: SmartObject (rich)
            smart_object_list: Vec::new(),
            smart_object_counter: 0,
            // New Feature: AdvancedMasking
            select_mask_config: SelectMaskConfig::new(),
            select_mask_open: false,
            // New Feature: GenerativeFill
            generative_fill_results: Vec::new(),
            generative_fill_prompt: String::new(),
            // New Feature: Basic3DLayer
            layer_3d_props: Vec::new(),
            active_3d_layer: None,
            // Batch 8: Extended shapes
            extended_shapes: std::collections::HashMap::new(),
            // Batch 8: Extended layer styles
            satin_effects: std::collections::HashMap::new(),
            color_overlays: std::collections::HashMap::new(),
            gradient_overlays: std::collections::HashMap::new(),
            pattern_overlays: std::collections::HashMap::new(),
            style_clipboard_satin: None,
            style_clipboard_color_overlay: None,
            style_clipboard_gradient_overlay: None,
            style_clipboard_pattern_overlay: None,
            // Batch 8: PSD export config
            psd_export_config: crate::app_state::shapes::PsdExportConfig::default(),
            last_psd_export_path: None,

            // New Feature: rich Smart Objects
            embedded_smart_objects: Vec::new(),
            embedded_so_counter: 0,
            // New Feature: actions / batch automation
            automation: super::automation::AutomationState::default(),
            // New Feature: scripting sandbox
            script_source: String::new(),
            script_log: Vec::new(),
            // New Feature: rich preferences
            preferences: super::prefs::PigmentPreferences::default(),
            preferences_path: None,
            // New Feature: per-artboard export metadata
            artboard_export_configs: std::collections::HashMap::new(),
            last_artboard_export_plan: Vec::new(),
            // New: native PSD serializer
            psd_export_rle: true,
            last_psd_native_path: None,
            // New: guides / rulers / smart guides
            guide_state: super::guides::GuideState::default(),
            // New: color management
            color_management: super::color_management::ColorManagement::default(),
            // New: keyboard shortcuts remap
            shortcut_map: super::shortcuts::ShortcutMap::defaults(),
            // New: navigator + multi-doc tabs
            doc_tabs: super::navigator::DocTabs::default(),
            navigator: super::navigator::NavigatorView::fit(nav_w, nav_h),
        }
    }
}
