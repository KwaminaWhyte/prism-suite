//! **Chart / Graph tool** geometry generation.
//!
//! Turns a [`ChartConfig`] (one or more named datasets + category labels) into
//! plain document `Shape`s laid out inside a target rectangle: grouped column /
//! bar columns, stacked columns, a line plot, or a pie. The output is the raw
//! vector primitives Illustrator's Graph tool would produce — rectangles for
//! columns, triangle/wedge fans for pie slices, an open polyline for line graphs
//! — plus a list of **label anchors** so the caller can place value / category
//! text wherever it wants.
//!
//! Pure: it takes data + a frame and returns geometry, with no `App` and no
//! document mutation, so the pie-sweep-sums-to-360° and column-count invariants
//! are unit-tested directly.

use crate::app_state::ChartConfig;
use crate::document::Shape;
use std::f32::consts::TAU;

/// The kind of chart to render. Mirrors Illustrator's nine graph types; we
/// implement the five with distinct, testable geometry. `Stacked` has no
/// `GraphType` counterpart in the current panel enum, so it is reachable only
/// via [`generate`] directly (kept for parity / future panel wiring).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ChartKind {
    Column,
    Bar,
    Stacked,
    Line,
    Pie,
}

/// A point at which the caller should place a text label, plus the string.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelAnchor {
    pub pos: (f32, f32),
    pub text: String,
}

/// The full result of generating a chart: the drawable shapes plus the label
/// anchors (value labels and category labels).
#[derive(Clone, Debug, Default)]
pub struct ChartGeometry {
    pub shapes: Vec<Shape>,
    pub labels: Vec<LabelAnchor>,
}

/// Distinct fill per dataset/series index — a small categorical palette that
/// cycles for >8 series.
fn series_color(i: usize) -> [f32; 4] {
    const PALETTE: [[f32; 4]; 8] = [
        [0.20, 0.55, 0.90, 1.0],
        [0.95, 0.45, 0.25, 1.0],
        [0.30, 0.75, 0.45, 1.0],
        [0.80, 0.35, 0.75, 1.0],
        [0.95, 0.78, 0.20, 1.0],
        [0.40, 0.45, 0.85, 1.0],
        [0.55, 0.75, 0.25, 1.0],
        [0.85, 0.30, 0.45, 1.0],
    ];
    PALETTE[i % PALETTE.len()]
}

/// The maximum *single* value across all datasets (for column / bar / line
/// scaling). Returns at least 1.0 so an all-zero chart still has a sane axis.
fn max_value(cfg: &ChartConfig) -> f32 {
    cfg.datasets
        .iter()
        .flat_map(|d| d.values.iter())
        .fold(0.0f64, |m, &v| m.max(v)) as f32
}

/// The maximum *stacked* total over all categories (for stacked scaling).
fn max_stacked(cfg: &ChartConfig, categories: usize) -> f32 {
    let mut max = 0.0f64;
    for ci in 0..categories {
        let total: f64 = cfg
            .datasets
            .iter()
            .map(|d| d.values.get(ci).copied().unwrap_or(0.0).max(0.0))
            .sum();
        max = max.max(total);
    }
    (max as f32).max(1.0)
}

/// Number of categories = the longest dataset (or the category-label count).
fn category_count(cfg: &ChartConfig) -> usize {
    cfg.datasets
        .iter()
        .map(|d| d.values.len())
        .max()
        .unwrap_or(0)
        .max(cfg.category_labels.len())
}

fn rect_shape(rect: [f32; 4], fill: [f32; 4]) -> Shape {
    Shape::rect(rect, fill, [0.0, 0.0, 0.0, 0.0], 0.0)
}

/// Generate a clustered **Column** (vertical bars) chart inside `frame`
/// `[x, y, w, h]`. Categories share the x-axis; each dataset is a coloured bar
/// in the cluster. `column_width` (0..100, % of the category slot) and
/// `cluster_width` (0..100, % used by the cluster) come from the config.
fn gen_column(cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let [fx, fy, fw, fh] = frame;
    let categories = category_count(cfg);
    let series = cfg.datasets.len();
    let mut geo = ChartGeometry::default();
    if categories == 0 || series == 0 {
        return geo;
    }
    let max = max_value(cfg).max(1.0);
    let cat_w = fw / categories as f32;
    let cluster_w = cat_w * (cfg.cluster_width / 100.0).clamp(0.05, 1.0);
    let bar_w = (cluster_w / series as f32) * (cfg.column_width / 100.0).clamp(0.05, 1.0);
    let cluster_x0 = |ci: usize| fx + ci as f32 * cat_w + (cat_w - cluster_w) * 0.5;

    for ci in 0..categories {
        for (si, ds) in cfg.datasets.iter().enumerate() {
            let v = ds.values.get(ci).copied().unwrap_or(0.0).max(0.0) as f32;
            let bh = (v / max) * fh;
            let bx = cluster_x0(ci) + si as f32 * (cluster_w / series as f32);
            let by = fy + fh - bh;
            geo.shapes
                .push(rect_shape([bx, by, bar_w.max(0.5), bh], series_color(si)));
            geo.labels.push(LabelAnchor {
                pos: (bx + bar_w * 0.5, by - 4.0),
                text: format_value(ds.values.get(ci).copied().unwrap_or(0.0)),
            });
        }
        // Category label below the axis.
        if let Some(lbl) = cfg.category_labels.get(ci) {
            geo.labels.push(LabelAnchor {
                pos: (fx + ci as f32 * cat_w + cat_w * 0.5, fy + fh + 4.0),
                text: lbl.clone(),
            });
        }
    }
    geo
}

/// Generate a clustered **Bar** (horizontal bars) chart — Column transposed.
fn gen_bar(cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let [fx, fy, fw, fh] = frame;
    let categories = category_count(cfg);
    let series = cfg.datasets.len();
    let mut geo = ChartGeometry::default();
    if categories == 0 || series == 0 {
        return geo;
    }
    let max = max_value(cfg).max(1.0);
    let cat_h = fh / categories as f32;
    let cluster_h = cat_h * (cfg.cluster_width / 100.0).clamp(0.05, 1.0);
    let bar_h = (cluster_h / series as f32) * (cfg.column_width / 100.0).clamp(0.05, 1.0);
    let cluster_y0 = |ci: usize| fy + ci as f32 * cat_h + (cat_h - cluster_h) * 0.5;

    for ci in 0..categories {
        for (si, ds) in cfg.datasets.iter().enumerate() {
            let v = ds.values.get(ci).copied().unwrap_or(0.0).max(0.0) as f32;
            let bw = (v / max) * fw;
            let by = cluster_y0(ci) + si as f32 * (cluster_h / series as f32);
            geo.shapes
                .push(rect_shape([fx, by, bw, bar_h.max(0.5)], series_color(si)));
            geo.labels.push(LabelAnchor {
                pos: (fx + bw + 4.0, by + bar_h * 0.5),
                text: format_value(ds.values.get(ci).copied().unwrap_or(0.0)),
            });
        }
        if let Some(lbl) = cfg.category_labels.get(ci) {
            geo.labels.push(LabelAnchor {
                pos: (fx - 4.0, fy + ci as f32 * cat_h + cat_h * 0.5),
                text: lbl.clone(),
            });
        }
    }
    geo
}

/// Generate a **Stacked** column chart: each category is one column of stacked
/// segments, one per dataset.
fn gen_stacked(cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let [fx, fy, fw, fh] = frame;
    let categories = category_count(cfg);
    let series = cfg.datasets.len();
    let mut geo = ChartGeometry::default();
    if categories == 0 || series == 0 {
        return geo;
    }
    let max = max_stacked(cfg, categories);
    let cat_w = fw / categories as f32;
    let bar_w = cat_w * (cfg.column_width / 100.0).clamp(0.05, 0.95);

    for ci in 0..categories {
        let bx = fx + ci as f32 * cat_w + (cat_w - bar_w) * 0.5;
        let mut acc = 0.0f32; // running stacked height from the baseline
        for (si, ds) in cfg.datasets.iter().enumerate() {
            let v = ds.values.get(ci).copied().unwrap_or(0.0).max(0.0) as f32;
            let seg_h = (v / max) * fh;
            let by = fy + fh - acc - seg_h;
            if seg_h > 0.0 {
                geo.shapes
                    .push(rect_shape([bx, by, bar_w, seg_h], series_color(si)));
            }
            acc += seg_h;
        }
        if let Some(lbl) = cfg.category_labels.get(ci) {
            geo.labels.push(LabelAnchor {
                pos: (bx + bar_w * 0.5, fy + fh + 4.0),
                text: lbl.clone(),
            });
        }
    }
    geo
}

/// Generate a **Line** chart: one open polyline per dataset across the
/// categories, value-scaled to the frame.
fn gen_line(cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let [fx, fy, fw, fh] = frame;
    let categories = category_count(cfg);
    let mut geo = ChartGeometry::default();
    if categories < 2 {
        return geo;
    }
    let max = max_value(cfg).max(1.0);
    let dx = fw / (categories - 1) as f32;
    for (si, ds) in cfg.datasets.iter().enumerate() {
        let pts: Vec<(f32, f32)> = (0..categories)
            .map(|ci| {
                let v = ds.values.get(ci).copied().unwrap_or(0.0) as f32;
                (fx + ci as f32 * dx, fy + fh - (v / max) * fh)
            })
            .collect();
        let handles = vec![(0.0, 0.0); pts.len()];
        let color = series_color(si);
        geo.shapes.push(Shape::path(
            pts.clone(),
            handles,
            false,
            [0.0, 0.0, 0.0, 0.0],
            color,
            2.0,
        ));
        // A value label at each vertex.
        for (ci, &p) in pts.iter().enumerate() {
            geo.labels.push(LabelAnchor {
                pos: (p.0, p.1 - 4.0),
                text: format_value(ds.values.get(ci).copied().unwrap_or(0.0)),
            });
        }
    }
    for ci in 0..categories {
        if let Some(lbl) = cfg.category_labels.get(ci) {
            geo.labels.push(LabelAnchor {
                pos: (fx + ci as f32 * dx, fy + fh + 4.0),
                text: lbl.clone(),
            });
        }
    }
    geo
}

/// Generate a **Pie** chart from the *first* dataset's values (Illustrator's pie
/// uses one row). Each slice is a wedge built from the centre + an arc-flattened
/// fan, filled by series colour. Slice sweeps sum to a full 360° (`TAU`).
fn gen_pie(cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let [fx, fy, fw, fh] = frame;
    let mut geo = ChartGeometry::default();
    let Some(ds) = cfg.datasets.first() else {
        return geo;
    };
    let values: Vec<f32> = ds.values.iter().map(|&v| (v as f32).max(0.0)).collect();
    let total: f32 = values.iter().sum();
    if total <= 0.0 {
        return geo;
    }
    let cx = fx + fw * 0.5;
    let cy = fy + fh * 0.5;
    let r = fw.min(fh) * 0.5;
    let mut a0 = -std::f32::consts::FRAC_PI_2; // start at 12 o'clock
    for (i, &v) in values.iter().enumerate() {
        let sweep = (v / total) * TAU;
        let a1 = a0 + sweep;
        // Flatten the arc into ≥2 segments so the wedge is a smooth fan.
        let steps = (sweep / 0.20).ceil().max(2.0) as usize;
        let mut pts = vec![(cx, cy)];
        for s in 0..=steps {
            let a = a0 + (a1 - a0) * (s as f32 / steps as f32);
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        let handles = vec![(0.0, 0.0); pts.len()];
        geo.shapes.push(Shape::path(
            pts,
            handles,
            true,
            series_color(i),
            [0.0, 0.0, 0.0, 0.0],
            0.0,
        ));
        // Label at the slice midpoint, slightly outside the radius.
        let mid = a0 + sweep * 0.5;
        geo.labels.push(LabelAnchor {
            pos: (cx + (r + 12.0) * mid.cos(), cy + (r + 12.0) * mid.sin()),
            text: cfg
                .category_labels
                .get(i)
                .cloned()
                .unwrap_or_else(|| format_value(v as f64)),
        });
        a0 = a1;
    }
    geo
}

/// Sweep angles of every pie slice, in radians — exposed for tests so the
/// 360°-sum invariant can be asserted without inspecting shape geometry.
#[allow(dead_code)]
pub fn pie_sweeps(cfg: &ChartConfig) -> Vec<f32> {
    let Some(ds) = cfg.datasets.first() else {
        return Vec::new();
    };
    let values: Vec<f32> = ds.values.iter().map(|&v| (v as f32).max(0.0)).collect();
    let total: f32 = values.iter().sum();
    if total <= 0.0 {
        return Vec::new();
    }
    values.iter().map(|&v| (v / total) * TAU).collect()
}

/// Format a value for a label: integers without a decimal, otherwise one place.
fn format_value(v: f64) -> String {
    if (v.fract()).abs() < 1e-6 {
        format!("{}", v as i64)
    } else {
        format!("{:.1}", v)
    }
}

/// Generate the chart geometry for `kind` from `cfg` inside `frame`
/// `[x, y, w, h]` (document space, +y down). The single public entry point used
/// by the apply layer.
pub fn generate(kind: ChartKind, cfg: &ChartConfig, frame: [f32; 4]) -> ChartGeometry {
    let mut geo = match kind {
        ChartKind::Column => gen_column(cfg, frame),
        ChartKind::Bar => gen_bar(cfg, frame),
        ChartKind::Stacked => gen_stacked(cfg, frame),
        ChartKind::Line => gen_line(cfg, frame),
        ChartKind::Pie => gen_pie(cfg, frame),
    };
    // An optional title label centred above the frame.
    if !cfg.title.is_empty() {
        geo.labels.push(LabelAnchor {
            pos: (frame[0] + frame[2] * 0.5, frame[1] - 16.0),
            text: cfg.title.clone(),
        });
    }
    geo
}

/// Bundle the generated shapes into a single grouped, even-odd `Compound` plus
/// the raw shapes — convenience for callers that just want everything tagged
/// into one group id. (Currently returns the shapes with `group` set; the apply
/// layer assigns the id.)
pub fn group_shapes(mut shapes: Vec<Shape>, group_id: u64) -> Vec<Shape> {
    for s in &mut shapes {
        s.set_group(Some(group_id));
    }
    shapes
}

/// Build a closed `Compound` baseline/axis frame around `frame` (a thin grid box)
/// — used when `show_grid` is on. Kept tiny and separate so callers can opt in.
#[allow(dead_code)]
pub fn axis_box(frame: [f32; 4]) -> Shape {
    let [x, y, w, h] = frame;
    // L-shaped axis: down the left edge then along the bottom edge.
    let pts = vec![(x, y), (x, y + h), (x + w, y + h)];
    let handles = vec![(0.0, 0.0); pts.len()];
    Shape::path(pts, handles, false, [0.0, 0.0, 0.0, 0.0], [0.4, 0.4, 0.4, 1.0], 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::ChartDataSet;

    fn cfg_two_series() -> ChartConfig {
        ChartConfig {
            datasets: vec![
                ChartDataSet {
                    label: "A".into(),
                    values: vec![10.0, 20.0, 30.0],
                    color: [0.0; 4],
                },
                ChartDataSet {
                    label: "B".into(),
                    values: vec![5.0, 25.0, 15.0],
                    color: [0.0; 4],
                },
            ],
            category_labels: vec!["Q1".into(), "Q2".into(), "Q3".into()],
            ..ChartConfig::default()
        }
    }

    #[test]
    fn column_makes_one_bar_per_value() {
        let cfg = cfg_two_series();
        let geo = generate(ChartKind::Column, &cfg, [0.0, 0.0, 300.0, 200.0]);
        // 2 series × 3 categories = 6 bars.
        assert_eq!(geo.shapes.len(), 6);
        // Tallest bar (value 30) is the full height; check a bar reaches near top.
        let tallest = geo
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect { rect, .. } => Some(rect[3]),
                _ => None,
            })
            .fold(0.0f32, f32::max);
        assert!((tallest - 200.0).abs() < 1.0, "max value spans full height");
    }

    #[test]
    fn bar_makes_one_bar_per_value() {
        let cfg = cfg_two_series();
        let geo = generate(ChartKind::Bar, &cfg, [0.0, 0.0, 300.0, 200.0]);
        assert_eq!(geo.shapes.len(), 6);
    }

    #[test]
    fn stacked_makes_one_segment_per_nonzero_value() {
        let cfg = cfg_two_series();
        let geo = generate(ChartKind::Stacked, &cfg, [0.0, 0.0, 300.0, 200.0]);
        // 3 categories × 2 non-zero segments = 6.
        assert_eq!(geo.shapes.len(), 6);
        // The tallest stack (Q2: 20+25=45) should span the full frame height.
        let max_top = geo
            .shapes
            .iter()
            .filter_map(|s| match s {
                Shape::Rect { rect, .. } => Some(rect[1]),
                _ => None,
            })
            .fold(f32::INFINITY, f32::min);
        assert!(max_top.abs() < 1.0, "tallest stack reaches the frame top");
    }

    #[test]
    fn line_makes_one_polyline_per_series() {
        let cfg = cfg_two_series();
        let geo = generate(ChartKind::Line, &cfg, [0.0, 0.0, 300.0, 200.0]);
        let polylines = geo
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Path { closed: false, .. }))
            .count();
        assert_eq!(polylines, 2, "one open polyline per dataset");
    }

    #[test]
    fn pie_sweeps_sum_to_full_circle() {
        let cfg = ChartConfig {
            datasets: vec![ChartDataSet {
                label: "P".into(),
                values: vec![1.0, 2.0, 3.0, 4.0],
                color: [0.0; 4],
            }],
            ..ChartConfig::default()
        };
        let sweeps = pie_sweeps(&cfg);
        assert_eq!(sweeps.len(), 4);
        let total: f32 = sweeps.iter().sum();
        assert!((total - TAU).abs() < 1e-4, "pie sweeps sum to 360°, got {total}");
    }

    #[test]
    fn pie_makes_one_wedge_per_slice() {
        let cfg = ChartConfig {
            datasets: vec![ChartDataSet {
                label: "P".into(),
                values: vec![1.0, 2.0, 3.0, 4.0],
                color: [0.0; 4],
            }],
            ..ChartConfig::default()
        };
        let geo = generate(ChartKind::Pie, &cfg, [0.0, 0.0, 200.0, 200.0]);
        // 4 wedges (each a closed path).
        let wedges = geo
            .shapes
            .iter()
            .filter(|s| matches!(s, Shape::Path { closed: true, .. }))
            .count();
        assert_eq!(wedges, 4);
    }

    #[test]
    fn empty_config_yields_nothing() {
        let cfg = ChartConfig::default();
        for kind in [
            ChartKind::Column,
            ChartKind::Bar,
            ChartKind::Stacked,
            ChartKind::Line,
            ChartKind::Pie,
        ] {
            let geo = generate(kind, &cfg, [0.0, 0.0, 100.0, 100.0]);
            assert!(geo.shapes.is_empty(), "{kind:?} with no data ⇒ no shapes");
        }
    }

    #[test]
    fn group_shapes_tags_all() {
        let cfg = cfg_two_series();
        let geo = generate(ChartKind::Column, &cfg, [0.0, 0.0, 300.0, 200.0]);
        let grouped = group_shapes(geo.shapes, 99);
        assert!(grouped.iter().all(|s| s.group() == Some(99)));
    }

    #[test]
    fn title_label_emitted() {
        let mut cfg = cfg_two_series();
        cfg.title = "Sales".into();
        let geo = generate(ChartKind::Column, &cfg, [0.0, 0.0, 300.0, 200.0]);
        assert!(geo.labels.iter().any(|l| l.text == "Sales"));
    }
}
