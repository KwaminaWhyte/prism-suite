use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // A. Variable Fonts & OpenType
    // =====================================================================

    #[test]
    fn test_set_variable_axis_adds_entry() {
        let mut app = App::new();
        app.apply(Action::SetVariableAxis {
            shape_id: 0,
            axis_tag: "wght".into(),
            value: 700.0,
        });
        let axes = app.variable_axis_values.get(&0).unwrap();
        assert_eq!(axes.len(), 1);
        assert_eq!(axes[0].axis_tag, "wght");
        assert!((axes[0].value - 700.0).abs() < 0.001);
    }

    #[test]
    fn test_set_variable_axis_replaces_same_tag() {
        let mut app = App::new();
        app.apply(Action::SetVariableAxis { shape_id: 1, axis_tag: "wght".into(), value: 400.0 });
        app.apply(Action::SetVariableAxis { shape_id: 1, axis_tag: "wght".into(), value: 900.0 });
        let axes = app.variable_axis_values.get(&1).unwrap();
        assert_eq!(axes.len(), 1);
        assert!((axes[0].value - 900.0).abs() < 0.001);
    }

    #[test]
    fn test_reset_variable_axes() {
        let mut app = App::new();
        app.apply(Action::SetVariableAxis { shape_id: 2, axis_tag: "wdth".into(), value: 75.0 });
        app.apply(Action::ResetVariableAxes { shape_id: 2 });
        assert!(app.variable_axis_values.get(&2).is_none());
    }

    #[test]
    fn test_set_opentype_feature_ligatures() {
        let mut app = App::new();
        app.apply(Action::SetOpenTypeFeature {
            shape_id: 0,
            feature: "ligatures".into(),
            enabled: false,
        });
        assert!(!app.opentype_features[&0].ligatures);
    }

    #[test]
    fn test_set_stylistic_set_clamps_to_1_20() {
        let mut app = App::new();
        app.apply(Action::SetStylisticSet { shape_id: 0, set: Some(25) });
        assert_eq!(app.opentype_features[&0].stylistic_set, Some(20));
        app.apply(Action::SetStylisticSet { shape_id: 0, set: Some(0) });
        // 0 clamped to 1
        assert_eq!(app.opentype_features[&0].stylistic_set, Some(1));
    }

    #[test]
    fn test_set_stylistic_set_none_clears() {
        let mut app = App::new();
        app.apply(Action::SetStylisticSet { shape_id: 0, set: Some(5) });
        app.apply(Action::SetStylisticSet { shape_id: 0, set: None });
        assert_eq!(app.opentype_features[&0].stylistic_set, None);
    }

    #[test]
    fn test_apply_all_small_caps_sets_c2sc() {
        let mut app = App::new();
        // First enable smcp, then apply c2sc — should set all_small_caps and clear small_caps.
        app.apply(Action::SetOpenTypeFeature {
            shape_id: 0,
            feature: "smcp".into(),
            enabled: true,
        });
        app.apply(Action::ApplyAllSmallCaps { shape_id: 0 });
        let f = &app.opentype_features[&0];
        assert!(f.all_small_caps);
        assert!(!f.small_caps);
    }

    #[test]
    fn test_multiple_opentype_features_independent() {
        let mut app = App::new();
        app.apply(Action::SetOpenTypeFeature { shape_id: 0, feature: "frac".into(), enabled: true });
        app.apply(Action::SetOpenTypeFeature { shape_id: 0, feature: "zero".into(), enabled: true });
        app.apply(Action::SetOpenTypeFeature { shape_id: 0, feature: "tnum".into(), enabled: true });
        let f = &app.opentype_features[&0];
        assert!(f.fractions);
        assert!(f.slashed_zero);
        assert!(f.tabular_figures);
        assert!(f.proportional_figures); // default is true and we haven't toggled it
    }

    // =====================================================================
    // B. Character & Paragraph Panel
    // =====================================================================

    #[test]
    fn test_set_character_tracking() {
        let mut app = App::new();
        app.apply(Action::SetCharacterTracking { shape_id: 0, tracking: 0.05 });
        assert!((app.char_styles[&0].tracking - 0.05).abs() < 0.001);
    }

    #[test]
    fn test_set_baseline_shift() {
        let mut app = App::new();
        app.apply(Action::SetBaselineShift { shape_id: 1, shift: -4.0 });
        assert!((app.char_styles[&1].baseline_shift - (-4.0)).abs() < 0.001);
    }

    #[test]
    fn test_horizontal_scale_clamps() {
        let mut app = App::new();
        app.apply(Action::SetHorizontalScale { shape_id: 0, scale: 5000.0 });
        assert!((app.char_styles[&0].horizontal_scale - 1000.0).abs() < 0.001);
        app.apply(Action::SetHorizontalScale { shape_id: 0, scale: 0.0 });
        assert!((app.char_styles[&0].horizontal_scale - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_vertical_scale_clamps() {
        let mut app = App::new();
        app.apply(Action::SetVerticalScale { shape_id: 0, scale: -10.0 });
        assert!((app.char_styles[&0].vertical_scale - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_set_underline_strikethrough() {
        let mut app = App::new();
        app.apply(Action::SetUnderline { shape_id: 0, underline: true });
        app.apply(Action::SetStrikethrough { shape_id: 0, strikethrough: true });
        assert!(app.char_styles[&0].underline);
        assert!(app.char_styles[&0].strikethrough);
    }

    #[test]
    fn test_paragraph_alignment() {
        let mut app = App::new();
        app.apply(Action::SetParagraphAlignment { shape_id: 0, alignment: ParaAlignment::Justify });
        assert_eq!(app.para_styles[&0].alignment, ParaAlignment::Justify);
    }

    #[test]
    fn test_paragraph_spacing() {
        let mut app = App::new();
        app.apply(Action::SetParagraphSpacing { shape_id: 0, before: 12.0, after: 6.0 });
        assert!((app.para_styles[&0].space_before - 12.0).abs() < 0.001);
        assert!((app.para_styles[&0].space_after - 6.0).abs() < 0.001);
    }

    #[test]
    fn test_first_line_indent() {
        let mut app = App::new();
        app.apply(Action::SetFirstLineIndent { shape_id: 0, indent: 24.0 });
        assert!((app.para_styles[&0].first_line_indent - 24.0).abs() < 0.001);
    }

    #[test]
    fn test_hyphenation() {
        let mut app = App::new();
        app.apply(Action::SetHyphenation { shape_id: 0, enabled: true });
        assert!(app.para_styles[&0].hyphenation);
    }

    #[test]
    fn test_tab_stops_add_and_remove() {
        let mut app = App::new();
        app.apply(Action::AddTabStop { shape_id: 0, position: 36.0 });
        app.apply(Action::AddTabStop { shape_id: 0, position: 72.0 });
        assert_eq!(app.para_styles[&0].tab_stops.len(), 2);
        app.apply(Action::RemoveTabStop { shape_id: 0, position: 36.0 });
        let stops = &app.para_styles[&0].tab_stops;
        assert_eq!(stops.len(), 1);
        assert!((stops[0] - 72.0).abs() < 0.001);
    }

    #[test]
    fn test_tab_stops_no_duplicate() {
        let mut app = App::new();
        app.apply(Action::AddTabStop { shape_id: 0, position: 48.0 });
        app.apply(Action::AddTabStop { shape_id: 0, position: 48.0 });
        assert_eq!(app.para_styles[&0].tab_stops.len(), 1);
    }

    // =====================================================================
    // C. Blend Tool
    // =====================================================================

    #[test]
    fn test_make_blend_creates_entry() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 1,
            spacing: BlendSpacing::SpecifiedSteps(10),
        });
        assert_eq!(app.blends.len(), 1);
        assert_eq!(app.blends[0].shape_ids, vec![0, 1]);
        assert_eq!(app.next_blend_id, 1);
    }

    #[test]
    fn test_release_blend_removes_entry() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 2,
            spacing: BlendSpacing::SmoothColor,
        });
        let id = app.blends[0].id;
        app.apply(Action::ReleaseBlend { blend_id: id });
        assert!(app.blends.is_empty());
    }

    #[test]
    fn test_set_blend_spacing() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 1,
            spacing: BlendSpacing::SpecifiedSteps(5),
        });
        let id = app.blends[0].id;
        app.apply(Action::SetBlendSpacing {
            blend_id: id,
            spacing: BlendSpacing::SpecifiedDistance(20.0),
        });
        assert!(matches!(app.blends[0].spacing, BlendSpacing::SpecifiedDistance(_)));
    }

    #[test]
    fn test_set_blend_orientation() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 1,
            spacing: BlendSpacing::SmoothColor,
        });
        let id = app.blends[0].id;
        app.apply(Action::SetBlendOrientation { blend_id: id, orientation: BlendOrientation::AlignToPath });
        assert_eq!(app.blends[0].orientation, BlendOrientation::AlignToPath);
    }

    #[test]
    fn test_replace_blend_spine() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 1,
            spacing: BlendSpacing::SpecifiedSteps(3),
        });
        let id = app.blends[0].id;
        app.apply(Action::ReplaceBlendSpine { blend_id: id, path_id: 5 });
        assert_eq!(app.blends[0].spine_path_id, Some(5));
    }

    #[test]
    fn test_reverse_blend() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 3,
            shape_id_b: 7,
            spacing: BlendSpacing::SpecifiedSteps(5),
        });
        let id = app.blends[0].id;
        app.apply(Action::ReverseBlend { blend_id: id });
        assert_eq!(app.blends[0].shape_ids, vec![7, 3]);
    }

    #[test]
    fn test_expand_blend_removes() {
        let mut app = App::new();
        app.apply(Action::MakeBlend {
            shape_id_a: 0,
            shape_id_b: 1,
            spacing: BlendSpacing::SpecifiedSteps(5),
        });
        let id = app.blends[0].id;
        app.apply(Action::ExpandBlend { blend_id: id });
        assert!(app.blends.is_empty());
    }

    // =====================================================================
    // D. 3D Effects
    // =====================================================================

    #[test]
    fn test_apply_3d_extrude() {
        let mut app = App::new();
        let cfg = Extrude3D { depth: 80.0, ..Default::default() };
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: cfg });
        assert!(app.extrude_3d.contains_key(&0));
        assert!((app.extrude_3d[&0].depth - 80.0).abs() < 0.001);
    }

    #[test]
    fn test_apply_3d_extrude_removes_revolve() {
        let mut app = App::new();
        app.apply(Action::Apply3DRevolve { shape_id: 0, config: Revolve3D::default() });
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        assert!(!app.revolve_3d.contains_key(&0));
        assert!(app.extrude_3d.contains_key(&0));
    }

    #[test]
    fn test_update_3d_extrude_depth() {
        let mut app = App::new();
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        app.apply(Action::Update3DExtrude {
            shape_id: 0,
            depth: Some(200.0),
            rotate_x: None,
            rotate_y: None,
            rotate_z: None,
        });
        assert!((app.extrude_3d[&0].depth - 200.0).abs() < 0.001);
    }

    #[test]
    fn test_update_3d_extrude_clamps_depth() {
        let mut app = App::new();
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        app.apply(Action::Update3DExtrude {
            shape_id: 0,
            depth: Some(9999.0),
            rotate_x: None,
            rotate_y: None,
            rotate_z: None,
        });
        assert!((app.extrude_3d[&0].depth - 2000.0).abs() < 0.001);
    }

    #[test]
    fn test_remove_3d_effect() {
        let mut app = App::new();
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        app.apply(Action::Remove3DEffect { shape_id: 0 });
        assert!(!app.extrude_3d.contains_key(&0));
    }

    #[test]
    fn test_apply_3d_revolve() {
        let mut app = App::new();
        let cfg = Revolve3D { angle: 180.0, ..Default::default() };
        app.apply(Action::Apply3DRevolve { shape_id: 1, config: cfg });
        assert!(app.revolve_3d.contains_key(&1));
        assert!((app.revolve_3d[&1].angle - 180.0).abs() < 0.001);
    }

    #[test]
    fn test_update_3d_revolve_clamps_angle() {
        let mut app = App::new();
        app.apply(Action::Apply3DRevolve { shape_id: 0, config: Revolve3D::default() });
        app.apply(Action::Update3DRevolve { shape_id: 0, angle: Some(500.0), offset: None });
        assert!((app.revolve_3d[&0].angle - 360.0).abs() < 0.001);
    }

    #[test]
    fn test_set_3d_lighting_clamps() {
        let mut app = App::new();
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        app.apply(Action::Set3DLighting { shape_id: 0, intensity: 150.0, ambient: -10.0 });
        assert!((app.extrude_3d[&0].light_intensity - 100.0).abs() < 0.001);
        assert!((app.extrude_3d[&0].ambient_light - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_set_3d_perspective_clamps() {
        let mut app = App::new();
        app.apply(Action::Apply3DExtrude { shape_id: 0, config: Extrude3D::default() });
        app.apply(Action::Set3DPerspective { shape_id: 0, degrees: 200.0 });
        assert!((app.extrude_3d[&0].perspective - 160.0).abs() < 0.001);
    }

    // =====================================================================
    // E. PDF Export State
    // =====================================================================

    #[test]
    fn test_set_pdf_standard() {
        let mut app = App::new();
        app.apply(Action::SetPdfStandard(PdfStandard::PdfX4));
        assert_eq!(app.pdf_export_config.standard, PdfStandard::PdfX4);
    }

    #[test]
    fn test_set_pdf_compatibility() {
        let mut app = App::new();
        app.apply(Action::SetPdfCompatibility(PdfCompatibility::Pdf17));
        assert_eq!(app.pdf_export_config.compatibility, PdfCompatibility::Pdf17);
    }

    #[test]
    fn test_set_pdf_embed_fonts() {
        let mut app = App::new();
        app.apply(Action::SetPdfEmbedFonts(false));
        assert!(!app.pdf_export_config.embed_fonts);
    }

    #[test]
    fn test_set_pdf_flatten_transparency() {
        let mut app = App::new();
        app.apply(Action::SetPdfFlattenTransparency(true));
        assert!(app.pdf_export_config.flatten_transparency);
    }

    #[test]
    fn test_set_pdf_color_space() {
        let mut app = App::new();
        app.apply(Action::SetPdfColorSpace(PdfColorSpace::Cmyk));
        assert_eq!(app.pdf_export_config.color_space, PdfColorSpace::Cmyk);
    }

    #[test]
    fn test_set_pdf_bleed_enables_flag() {
        let mut app = App::new();
        app.apply(Action::SetPdfBleed { top: 5.0, bottom: 5.0, left: 5.0, right: 5.0 });
        assert!(app.pdf_export_config.include_bleed);
        assert!((app.pdf_export_config.bleed_top - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_set_pdf_marks() {
        let mut app = App::new();
        app.apply(Action::SetPdfMarks(PdfMarks {
            trim: true,
            bleed: true,
            reg: false,
            color_bars: false,
            page_info: false,
        }));
        assert!(app.pdf_export_config.marks.trim);
        assert!(app.pdf_export_config.marks.bleed);
    }

    #[test]
    fn test_set_pdf_password_sets_require_flag() {
        let mut app = App::new();
        app.apply(Action::SetPdfPassword {
            user: "secret".into(),
            owner: "admin".into(),
        });
        assert!(app.pdf_export_config.require_password);
        assert_eq!(app.pdf_export_config.user_password, "secret");
    }

    #[test]
    fn test_set_pdf_permissions() {
        let mut app = App::new();
        app.apply(Action::SetPdfPermissions { printing: false, editing: false, copying: false });
        assert!(!app.pdf_export_config.allow_printing);
        assert!(!app.pdf_export_config.allow_editing);
        assert!(!app.pdf_export_config.allow_copying);
    }

    #[test]
    fn test_export_as_pdf_sets_status() {
        let mut app = App::new();
        app.apply(Action::ExportAsPdf { path: "/tmp/out.pdf".into() });
        assert!(app.status_message.is_some());
        assert!(app.status_message.as_ref().unwrap().0.contains("out.pdf"));
    }
}
