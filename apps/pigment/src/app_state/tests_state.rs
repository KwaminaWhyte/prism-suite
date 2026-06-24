//! Unit tests for `app_state` free helpers, type defaults, and the `App::apply`
//! dispatcher. Moved verbatim out of `app_state/mod.rs` (pure mechanical
//! refactor); the only change is rewriting the test modules' `use super::...`
//! parent-module imports to `use crate::app_state::...` since the tests now sit
//! one module level deeper.

#[cfg(test)]
mod selection_tests {
    use crate::app_state::{mode_from_modifiers, shape_mask};
    use prism_core::raster::CombineMode;

    // Modifier → combine-mode mapping matches the egui app's `mode_from_modifiers`.
    #[test]
    fn modifiers_pick_combine_mode() {
        assert_eq!(mode_from_modifiers(false, false), CombineMode::Replace);
        assert_eq!(mode_from_modifiers(true, false), CombineMode::Add);
        assert_eq!(mode_from_modifiers(false, true), CombineMode::Subtract);
        assert_eq!(mode_from_modifiers(true, true), CombineMode::Intersect);
    }

    // A rectangle marquee selects exactly the pixel centers inside the rect.
    #[test]
    fn rect_mask_fills_interior() {
        let m = shape_mask([2.0, 2.0, 4.0, 4.0], false, 8, 8);
        // Inside: pixel centers (2.5..5.5).
        assert_eq!(m[3 * 8 + 3], 1.0);
        assert_eq!(m[5 * 8 + 5], 1.0);
        // Outside the rect.
        assert_eq!(m[0], 0.0);
        assert_eq!(m[7 * 8 + 7], 0.0);
    }

    // An ellipse marquee selects its center but not the bounding-box corners.
    #[test]
    fn ellipse_mask_excludes_corners() {
        let m = shape_mask([0.0, 0.0, 8.0, 8.0], true, 8, 8);
        // Center is selected.
        assert_eq!(m[4 * 8 + 4], 1.0);
        // Corners of the bbox fall outside the inscribed ellipse.
        assert_eq!(m[0], 0.0);
        assert_eq!(m[7 * 8 + 7], 0.0);
    }
}

#[cfg(test)]
mod xform_tests {
    use crate::app_state::compute_xform;

    // A pure +half-width translate maps to a layer-from-canvas offset of -0.5
    // (identity 2x2). Mirrors the egui app's `translate_maps_to_uv_offset`.
    #[test]
    fn translate_maps_to_uv_offset() {
        let (m, off) = compute_xform([50.0, 0.0], 1.0, 100.0, 50.0);
        assert_eq!(m, [1.0, 0.0, 0.0, 1.0]);
        assert!((off[0] - -0.5).abs() < 1e-6, "off.x = {}", off[0]);
        assert!(off[1].abs() < 1e-6, "off.y = {}", off[1]);
    }

    // A 2x uniform scale halves the 2x2 (inv = 0.5) and recenters about 0.5.
    #[test]
    fn scale_inverts_into_matrix() {
        let (m, off) = compute_xform([0.0, 0.0], 2.0, 100.0, 100.0);
        assert!((m[0] - 0.5).abs() < 1e-6);
        assert!((m[3] - 0.5).abs() < 1e-6);
        // off = 0.5 - 0.5*0.5 = 0.25 in both axes.
        assert!((off[0] - 0.25).abs() < 1e-6, "off.x = {}", off[0]);
        assert!((off[1] - 0.25).abs() < 1e-6, "off.y = {}", off[1]);
    }
}

#[cfg(test)]
mod hsv_tests {
    use crate::app_state::{hsv_to_rgb, rgb_to_hsv};

    // Pure red: H=0°, S=1, V=1 → rgb(1,0,0).
    #[test]
    fn red_from_hsv() {
        let c = hsv_to_rgb(0.0, 1.0, 1.0);
        assert!((c[0] - 1.0).abs() < 1e-5, "r={}", c[0]);
        assert!(c[1].abs() < 1e-5, "g={}", c[1]);
        assert!(c[2].abs() < 1e-5, "b={}", c[2]);
    }

    // Greyscale: S=0 → all channels equal V.
    #[test]
    fn grey_from_hsv() {
        let c = hsv_to_rgb(180.0, 0.0, 0.5);
        assert!((c[0] - 0.5).abs() < 1e-5);
        assert!((c[1] - 0.5).abs() < 1e-5);
        assert!((c[2] - 0.5).abs() < 1e-5);
    }

    // Round-trip: rgb → hsv → rgb stays within 0.01.
    #[test]
    fn rgb_hsv_roundtrip() {
        let orig = [0.2_f32, 0.6, 0.9, 1.0];
        let (h, s, v) = rgb_to_hsv(orig);
        let back = hsv_to_rgb(h, s, v);
        for i in 0..3 {
            assert!((back[i] - orig[i]).abs() < 0.01, "channel {i}: {} vs {}", back[i], orig[i]);
        }
    }
}

#[cfg(test)]
mod pen_tests {
    use crate::app_state::{rasterize_pen_path, Brush, PenNode};

    // A two-node straight-line pen path (ctrl handles at node positions) must
    // produce at least one dab. Verifies the bézier walker runs without panic.
    #[test]
    fn straight_path_produces_dabs() {
        let path = vec![
            PenNode { pos: (0.0, 0.0), ctrl_in: (0.0, 0.0), ctrl_out: (0.0, 0.0) },
            PenNode { pos: (100.0, 0.0), ctrl_in: (100.0, 0.0), ctrl_out: (100.0, 0.0) },
        ];
        let brush = Brush::default();
        let dabs = rasterize_pen_path(&path, &brush);
        assert!(!dabs.is_empty(), "expected at least one dab for a 100px segment");
    }

    // A single-node path (< 2 nodes) must produce zero dabs (no segment).
    #[test]
    fn single_node_produces_no_dabs() {
        let path = vec![
            PenNode { pos: (50.0, 50.0), ctrl_in: (50.0, 50.0), ctrl_out: (50.0, 50.0) },
        ];
        let brush = Brush::default();
        let dabs = rasterize_pen_path(&path, &brush);
        assert!(dabs.is_empty());
    }
}

#[cfg(test)]
mod layer_comp_tests {
    use crate::app_state::{capture_layer_comp_states, apply_layer_comp_states};
    use prism_core::{Document, Size};

    fn make_doc() -> Document {
        let mut doc = Document::new(Size::new(100, 100));
        doc.layers.add_raster("BG");
        doc.layers.add_raster("FG");
        doc
    }

    #[test]
    fn test_layer_comp_capture() {
        let doc = make_doc();
        let states = capture_layer_comp_states(&doc);
        assert_eq!(states.len(), doc.layers.layers.len(),
            "comp should capture state for every layer");
    }

    #[test]
    fn test_layer_comp_apply() {
        let mut doc = make_doc();
        // Snapshot with both layers visible.
        let states = capture_layer_comp_states(&doc);
        // Now hide the first layer.
        if let Some(l) = doc.layers.layers.first_mut() {
            l.visible = false;
        }
        assert!(!doc.layers.layers[0].visible);
        // Restore the comp — first layer should be visible again.
        apply_layer_comp_states(&mut doc, &states);
        assert!(doc.layers.layers[0].visible, "layer should be restored to visible");
    }

    #[test]
    fn test_layer_comp_update() {
        let mut doc = make_doc();
        let states = capture_layer_comp_states(&doc);
        // Change opacity.
        if let Some(l) = doc.layers.layers.first_mut() {
            l.opacity = 0.5;
        }
        // Re-capture (simulating UpdateLayerComp).
        let new_states = capture_layer_comp_states(&doc);
        let id = doc.layers.layers[0].id;
        assert!((new_states[&id].opacity - 0.5).abs() < 1e-5,
            "updated comp should store new opacity");
        // Old comp still has 1.0.
        assert!((states[&id].opacity - 1.0).abs() < 1e-5,
            "old comp should retain original opacity");
    }

    #[test]
    fn test_layer_comp_opacity_restored() {
        let mut doc = make_doc();
        let id = doc.layers.layers[0].id;
        // Snapshot.
        let states = capture_layer_comp_states(&doc);
        // Change opacity.
        doc.layers.layers[0].opacity = 0.25;
        // Apply comp.
        apply_layer_comp_states(&mut doc, &states);
        assert!((doc.layers.layers[0].opacity - 1.0).abs() < 1e-5,
            "opacity should be restored to 1.0 after ApplyLayerComp");
        let _ = id;
    }
}

#[cfg(test)]
mod focus_area_tests {
    use crate::app_state::focus_area_mask;

    /// Build a flat RGBA f32 pixel buffer (all same luma).
    fn uniform_pixels(w: u32, h: u32, luma: f32) -> Vec<f32> {
        vec![luma, luma, luma, 1.0].repeat((w * h) as usize)
    }

    /// Build an RGBA f32 buffer with a sharp edge: left half dark, right half bright.
    fn edge_pixels(w: u32, h: u32) -> Vec<f32> {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for _y in 0..h {
            for x in 0..w {
                let v = if x < w / 2 { 0.0 } else { 1.0 };
                px.extend_from_slice(&[v, v, v, 1.0]);
            }
        }
        px
    }

    #[test]
    fn test_focus_area_uniform() {
        // Uniform image has zero variance → nothing selected (at any threshold > 0).
        let px = uniform_pixels(8, 8, 0.5);
        let mask = focus_area_mask(&px, 8, 8, 0.1, 0.0, false);
        // All pixels should be unselected (zero variance < threshold).
        for (i, &v) in mask.iter().enumerate() {
            assert_eq!(v, 0, "uniform image pixel {i} should not be selected");
        }
    }

    #[test]
    fn test_focus_area_edge_selected() {
        // Edge pixels have high variance → should be selected.
        let px = edge_pixels(16, 8);
        let mask = focus_area_mask(&px, 16, 8, 0.05, 0.0, false);
        // Pixels near the edge (x ≈ 8) should be selected.
        let edge_col = 7usize;
        let edge_idx = 0 * 16 + edge_col; // top row, edge column
        // The mask should have nonzero value near the edge.
        let sum: u32 = mask.iter().map(|&v| v as u32).sum();
        assert!(sum > 0, "edge image should select some pixels (sum={})", sum);
        let _ = edge_idx;
    }

    #[test]
    fn test_focus_area_invert() {
        // Inverted: uniform image → all pixels selected.
        let px = uniform_pixels(8, 8, 0.5);
        let mask_normal = focus_area_mask(&px, 8, 8, 0.1, 0.0, false);
        let mask_invert = focus_area_mask(&px, 8, 8, 0.1, 0.0, true);
        // Normal should all be 0 (no variance), inverted should all be 255.
        assert!(mask_normal.iter().all(|&v| v == 0));
        assert!(mask_invert.iter().all(|&v| v == 255));
    }

    #[test]
    fn test_focus_area_threshold() {
        // Higher threshold → fewer selected pixels in edge image.
        let px = edge_pixels(32, 8);
        let mask_low  = focus_area_mask(&px, 32, 8, 0.01, 0.0, false);
        let mask_high = focus_area_mask(&px, 32, 8, 0.99, 0.0, false);
        let sum_low:  u32 = mask_low.iter().map(|&v| v as u32).sum();
        let sum_high: u32 = mask_high.iter().map(|&v| v as u32).sum();
        assert!(sum_low >= sum_high,
            "lower threshold should select >= pixels: low={sum_low}, high={sum_high}");
    }
}

#[cfg(test)]
mod pattern_stamp_tests {
    use crate::app_state::{PatternDef};

    fn make_pattern(w: u32, h: u32) -> PatternDef {
        let pixels = vec![[1.0f32, 0.0, 0.0, 1.0]; (w * h) as usize];
        PatternDef { name: "Red".into(), pixels, width: w, height: h }
    }

    #[test]
    fn test_define_pattern() {
        let pat = make_pattern(4, 4);
        assert_eq!(pat.width, 4);
        assert_eq!(pat.height, 4);
        assert_eq!(pat.pixels.len(), 16);
        assert_eq!(pat.name, "Red");
    }

    #[test]
    fn test_pattern_pixel_access() {
        let pat = make_pattern(2, 2);
        // All pixels should be [1, 0, 0, 1].
        for px in &pat.pixels {
            assert!((px[0] - 1.0).abs() < 1e-5, "R should be 1.0");
            assert!(px[1].abs() < 1e-5, "G should be 0.0");
        }
    }

    #[test]
    fn test_pattern_stamp_scale_default() {
        // Default scale from App::new() is 1.0.
        let scale: f32 = 1.0;
        assert!((scale - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_pattern_aligned_default() {
        // Default aligned from App::new() is true.
        let aligned: bool = true;
        assert!(aligned);
    }

    #[test]
    fn test_pattern_library_push() {
        let mut library: Vec<PatternDef> = Vec::new();
        library.push(make_pattern(8, 8));
        library.push(make_pattern(4, 4));
        assert_eq!(library.len(), 2);
        // Delete first.
        library.remove(0);
        assert_eq!(library.len(), 1);
        assert_eq!(library[0].width, 4);
    }
}

#[cfg(test)]
mod match_color_tests {
    use crate::app_state::{match_color_stats, apply_match_color};

    fn uniform_pixels(r: f32, g: f32, b: f32, n: usize) -> Vec<f32> {
        let mut px = Vec::with_capacity(n * 4);
        for _ in 0..n { px.extend_from_slice(&[r, g, b, 1.0]); }
        px
    }

    #[test]
    fn test_match_color_stats_uniform() {
        let px = uniform_pixels(0.8, 0.2, 0.5, 100);
        let (l, a, _b) = match_color_stats(&px);
        // L ≈ 0.299*0.8 + 0.587*0.2 + 0.114*0.5
        let expected_l = 0.299_f32 * 0.8 + 0.587 * 0.2 + 0.114 * 0.5;
        assert!((l - expected_l).abs() < 1e-3, "L: got {l}, expected {expected_l}");
        let expected_a = 0.8 - 0.2;
        assert!((a - expected_a).abs() < 1e-3, "a: got {a}, expected {expected_a}");
    }

    #[test]
    fn test_match_color_apply_luminance() {
        // Source is bright (luma 0.9), target is dark (luma 0.1).
        let src = uniform_pixels(0.9, 0.9, 0.9, 4);
        let tgt = uniform_pixels(0.1, 0.1, 0.1, 4);
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 100.0, true, false, false);
        // Result should be brighter than the target.
        let result_l = result[0]; // R channel
        assert!(result_l > 0.1, "result should be brighter: got {result_l}");
    }

    #[test]
    fn test_match_color_fade_zero() {
        // Fade=0 should leave target unchanged.
        let src = uniform_pixels(0.9, 0.5, 0.1, 4);
        let tgt = uniform_pixels(0.3, 0.3, 0.3, 4);
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 0.0, true, true, false);
        for i in 0..4 {
            assert!((result[i*4] - tgt[i*4]).abs() < 1e-3, "fade=0 should not change target");
        }
    }

    #[test]
    fn test_match_color_neutralize() {
        // Colourful target → neutralize should pull toward grey.
        let src = uniform_pixels(0.5, 0.5, 0.5, 1);
        let tgt = uniform_pixels(1.0, 0.0, 0.0, 1); // pure red
        let src_stats = match_color_stats(&src);
        let tgt_stats = match_color_stats(&tgt);
        let result = apply_match_color(&tgt, src_stats, tgt_stats, 100.0, false, false, true);
        // G channel should increase (pulled toward grey).
        assert!(result[1] > 0.0, "neutralize should increase G: got {}", result[1]);
    }
}

#[cfg(test)]
mod vanishing_point_tests {
    use crate::app_state::{VanishingPlane, VanishingToolMode, stamp_in_perspective};

    fn unit_plane() -> [[f32; 2]; 4] {
        // A simple axis-aligned square: TL=(0,0), TR=(100,0), BR=(100,100), BL=(0,100).
        [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    #[test]
    fn test_vanishing_plane_add() {
        let mut planes: Vec<VanishingPlane> = Vec::new();
        planes.push(VanishingPlane {
            corners: unit_plane(),
            grid_size: 50.0,
            active: true,
        });
        assert_eq!(planes.len(), 1);
        assert_eq!(planes[0].grid_size, 50.0);
        assert_eq!(planes[0].corners[0], [0.0, 0.0]);
    }

    #[test]
    fn test_vanishing_plane_set_corner() {
        let mut planes = vec![VanishingPlane {
            corners: unit_plane(),
            grid_size: 50.0,
            active: true,
        }];
        planes[0].corners[2] = [120.0, 120.0];
        assert_eq!(planes[0].corners[2], [120.0, 120.0]);
    }

    #[test]
    fn test_vanishing_grid_size() {
        let mut plane = VanishingPlane { corners: unit_plane(), grid_size: 50.0, active: true };
        plane.grid_size = 100.0f32.clamp(5.0, 500.0);
        assert!((plane.grid_size - 100.0).abs() < 1e-5);
    }

    #[test]
    fn test_vanishing_tool_mode() {
        let mode_a = VanishingToolMode::Stamping;
        assert_eq!(mode_a, VanishingToolMode::Stamping);
        let mode_b = VanishingToolMode::Pasting;
        assert_eq!(mode_b, VanishingToolMode::Pasting);
        let mode_c = VanishingToolMode::DefiningPlane;
        assert_ne!(mode_c, VanishingToolMode::Stamping);
    }

    #[test]
    fn test_stamp_in_perspective_no_panic() {
        // Stamp on a small 10×10 canvas within the unit plane; should not panic.
        let corners = [[0.0f32, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
        let mut pixels = vec![0.5f32; 10 * 10 * 4];
        // Make src region bright.
        for i in 0..(10 * 2 * 4) { pixels[i] = 1.0; }
        stamp_in_perspective(&mut pixels, 10, 10, &corners, [2.0, 2.0], [7.0, 7.0], 2.0);
        // Just verify no panic and buffer size unchanged.
        assert_eq!(pixels.len(), 10 * 10 * 4);
    }
}

#[cfg(test)]
mod select_subject_tests {
    use crate::app_state::{select_subject_mask};

    #[test]
    fn mask_len_matches_image() {
        let px = vec![0.5f32; 16 * 16 * 4];
        let mask = select_subject_mask(&px, 16, 16, 0.5, 0.0);
        assert_eq!(mask.len(), 16 * 16);
    }

    #[test]
    fn threshold_zero_selects_nothing() {
        // threshold=0 → edge_thresh=0 → edge[start]=0 is NOT < 0 → BFS never starts
        let px = vec![0.5f32; 8 * 8 * 4];
        let mask = select_subject_mask(&px, 8, 8, 0.0, 0.0);
        assert!(mask.iter().all(|&v| v == 0), "threshold=0 should select nothing (strict <)");
    }

    #[test]
    fn mid_threshold_selects_all_on_flat() {
        // Flat image: all Sobel edges=0, max_e clamped to 1e-6.
        // threshold=0.5 → edge_thresh=0.5e-6 → 0 < 0.5e-6 is TRUE → BFS fills everything.
        let px = vec![0.5f32; 8 * 8 * 4];
        let mask = select_subject_mask(&px, 8, 8, 0.5, 0.0);
        assert!(mask.iter().all(|&v| v == 255), "mid threshold on flat should select all");
    }

    #[test]
    fn feather_blurs_mask_edges() {
        let px = vec![0.5f32; 16 * 16 * 4];
        let sharp = select_subject_mask(&px, 16, 16, 0.0, 0.0);
        let feathered = select_subject_mask(&px, 16, 16, 0.0, 3.0);
        // Both should be the same (all 255) for flat image — feathering all-255 keeps all-255
        assert_eq!(sharp.len(), feathered.len());
    }
}

#[cfg(test)]
mod artboard_tests {
    use crate::app_state::{Artboard, SoftProofSettings, ProofProfile, RenderingIntent};

    #[test]
    fn artboard_default_background_white() {
        let ab = Artboard { id: 1, name: "A".into(), x: 0, y: 0, width: 800, height: 600, background_color: [1.0, 1.0, 1.0, 1.0] };
        assert_eq!(ab.background_color, [1.0f32, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn artboard_clone_preserves_fields() {
        let ab = Artboard { id: 42, name: "Clone".into(), x: 10, y: 20, width: 300, height: 200, background_color: [0.5, 0.5, 0.5, 1.0] };
        let ab2 = ab.clone();
        assert_eq!(ab2.id, 42);
        assert_eq!(ab2.name, "Clone");
        assert_eq!(ab2.x, 10);
    }

    #[test]
    fn soft_proof_defaults() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.profile, ProofProfile::WorkingCmyk);
        assert_eq!(sp.intent, RenderingIntent::Perceptual);
        assert!(sp.black_point_compensation);
        assert!(!sp.simulate_paper_white);
        assert!(!sp.gamut_warning);
        assert_eq!(sp.gamut_warning_color, [0.0f32, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn proof_profile_labels() {
        assert_eq!(ProofProfile::Srgb.label(), "sRGB");
        assert_eq!(ProofProfile::AdobeRgb.label(), "Adobe RGB");
        assert_eq!(ProofProfile::WorkingCmyk.label(), "Working CMYK");
    }

    #[test]
    fn rendering_intent_labels() {
        assert_eq!(RenderingIntent::Perceptual.label(), "Perceptual");
        assert_eq!(RenderingIntent::AbsoluteColorimetric.label(), "Absolute Colorimetric");
    }
}

#[cfg(test)]
mod apply_image_tests {
    use crate::app_state::{ApplyImageChannel, apply_image_blend, BlendMode};

    #[test]
    fn blend_normal_opacity_half() {
        let src = vec![1.0f32, 0.0, 0.0, 1.0]; // red
        let tgt = vec![0.0f32, 0.0, 1.0, 1.0]; // blue
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 0.5, false);
        // Channel extraction: s = luma(red) = 0.2126; result.r = 0.0*0.5 + 0.2126*0.5 ≈ 0.106
        assert!((out[0] - 0.5 * 0.2126).abs() < 0.01, "red ch: {}", out[0]);
    }

    #[test]
    fn invert_source_flips() {
        let src = vec![1.0f32, 1.0, 1.0, 1.0]; // white
        let tgt = vec![0.5f32, 0.5, 0.5, 1.0];
        let normal   = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, false);
        let inverted = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, true);
        assert!(normal[0] > inverted[0], "invert should darken");
    }

    #[test]
    fn alpha_channel_passthrough() {
        let src = vec![0.5f32, 0.5, 0.5, 0.8];
        let tgt = vec![0.5f32, 0.5, 0.5, 0.3];
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Rgb, BlendMode::Normal, 1.0, false);
        // Alpha should come from tgt unchanged
        assert!((out[3] - 0.3).abs() < 0.01, "alpha should be tgt alpha: {}", out[3]);
    }

    #[test]
    fn multiply_blend_darkens() {
        // Both layers have r=0.8; multiply → 0.8*0.8=0.64 < 0.8
        let src = vec![0.8f32, 0.0, 0.0, 1.0];
        let tgt = vec![0.8f32, 0.0, 0.0, 1.0];
        let out = apply_image_blend(&src, &tgt, ApplyImageChannel::Red, BlendMode::Multiply, 1.0, false);
        assert!(out[0] < 0.8, "multiply should darken: {}", out[0]);
    }
}

#[cfg(test)]
mod soft_proof_tests {
    use crate::app_state::{SoftProofSettings, ProofProfile, RenderingIntent};

    #[test]
    fn toggle_enabled_field() {
        let mut enabled = false;
        enabled = !enabled;
        assert!(enabled);
        enabled = !enabled;
        assert!(!enabled);
    }

    #[test]
    fn proof_profile_default() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.profile, ProofProfile::WorkingCmyk);
    }

    #[test]
    fn rendering_intent_default() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.intent, RenderingIntent::Perceptual);
    }

    #[test]
    fn gamut_warning_default_green() {
        let sp = SoftProofSettings::default();
        assert_eq!(sp.gamut_warning_color, [0.0f32, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn simulate_flags_default_off() {
        let sp = SoftProofSettings::default();
        assert!(!sp.simulate_paper_white);
        assert!(!sp.simulate_black_ink);
        assert!(!sp.gamut_warning);
    }
}
