//! Lumetri Scopes panel — waveform, vectorscope, and RGB histogram.
//!
//! Three tabs selectable via `app.scope_tab` (0=Waveform, 1=Vectorscope, 2=Histogram):
//!   - **Waveform**: per-column luma bars from scope_data.
//!   - **Vectorscope**: Cb/Cr dots plotted in a 64×64 virtual circle.
//!   - **Histogram**: overlapping R/G/B bar charts (64 buckets).
//!
//! Shown when `app.scopes_open` is true (toolbar Scopes button).

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, section_header};

use crate::app_state::{Action, App};
use crate::Reel;

/// Bar chart height in pixels.
const BAR_H: f32 = 56.0;
/// Number of histogram buckets (downsample 256 → 64).
const BUCKETS: usize = 64;
/// Vectorscope canvas size in pixels.
const VS_SIZE: f32 = 80.0;

/// Downsample a 256-bin histogram to BUCKETS by summing each group.
fn downsample(bins: &[f32]) -> Vec<f32> {
    let group = bins.len() / BUCKETS;
    if group == 0 {
        return bins.to_vec();
    }
    (0..BUCKETS)
        .map(|i| {
            let s: f32 = bins[i * group..(i * group + group).min(bins.len())].iter().sum();
            s / group as f32
        })
        .collect()
}

/// Render a histogram bar chart with overlapping R (red), G (green), B (blue) bars.
fn histogram_chart(
    hist_r: &[f32],
    hist_g: &[f32],
    hist_b: &[f32],
) -> impl IntoElement {
    let r = downsample(hist_r);
    let g = downsample(hist_g);
    let b = downsample(hist_b);
    let peak = r.iter().chain(g.iter()).chain(b.iter())
        .copied()
        .fold(0.0_f32, f32::max)
        .max(1e-5);

    let bars: Vec<gpui::AnyElement> = (0..BUCKETS)
        .map(|i| {
            let hr = (r.get(i).copied().unwrap_or(0.0) / peak * BAR_H).max(1.0);
            let hg = (g.get(i).copied().unwrap_or(0.0) / peak * BAR_H).max(1.0);
            let hb = (b.get(i).copied().unwrap_or(0.0) / peak * BAR_H).max(1.0);
            div()
                .relative()
                .w(px(3.0))
                .h(px(BAR_H))
                .flex()
                .items_end()
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(px(hb))
                        .bg(rgb(0x3b82f6)),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(px(hg))
                        .bg(rgb(0x22c55e)),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(px(hr))
                        .bg(rgb(0xef4444)),
                )
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .pb_1()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Histogram (R/G/B)"),
        )
        .child(
            div()
                .relative()
                .h(px(BAR_H))
                .bg(colors::surface_overlay())
                .rounded_sm()
                .overflow_hidden()
                .flex()
                .flex_row()
                .items_end()
                .gap_px()
                .children(bars),
        )
}

/// Render a luma waveform as individual 2×2 dots (one dot per pixel sample per column).
fn waveform_chart(cols: &[Vec<f32>]) -> impl IntoElement {
    let n_cols = cols.len().max(1);
    let col_width_px = 256.0 / n_cols as f32;

    // Pre-build dots Vec before any div chain (GPUI 0.2.2 rule).
    let dots: Vec<gpui::AnyElement> = cols
        .iter()
        .enumerate()
        .flat_map(|(ci, col)| {
            let x = ci as f32 * col_width_px;
            col.iter().map(move |&v| {
                let y = (1.0 - v) * BAR_H;
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(2.0))
                    .h(px(2.0))
                    .bg(rgb(0x94a3b8))
                    .into_any_element()
            })
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .pb_1()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Waveform (luma)"),
        )
        .child(
            div()
                .relative()
                .w(px(256.0))
                .h(px(BAR_H))
                .bg(colors::surface_overlay())
                .rounded_sm()
                .overflow_hidden()
                .children(dots),
        )
}

/// Render a vectorscope: Cb/Cr dots in a VS_SIZE×VS_SIZE canvas.
/// Each dot is a 2×2 px absolute div. Cb maps to x, Cr to y.
fn vectorscope_chart(dots: &[(f32, f32)]) -> impl IntoElement {
    // Limit rendered dots to avoid overwhelming the layout engine.
    let max_dots = 512usize;
    let step = if dots.len() > max_dots { dots.len() / max_dots } else { 1 };

    let dot_els: Vec<gpui::AnyElement> = dots
        .iter()
        .step_by(step.max(1))
        .take(max_dots)
        .map(|(u, v)| {
            // u,v are in roughly -0.5..0.5; map to 0..VS_SIZE
            let x = ((u + 0.5) * VS_SIZE).clamp(0.0, VS_SIZE - 2.0);
            let y = ((0.5 - v) * VS_SIZE).clamp(0.0, VS_SIZE - 2.0); // flip y
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(2.0))
                .h(px(2.0))
                .bg(rgb(0x7ee8a2)) // light green dots
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_col()
        .gap_1()
        .px_2()
        .pb_1()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child(format!("Vectorscope ({} dots)", dots.len())),
        )
        .child(
            div()
                .relative()
                .w(px(VS_SIZE))
                .h(px(VS_SIZE))
                .bg(colors::surface_overlay())
                .rounded_sm()
                .overflow_hidden()
                // Center crosshair lines
                .child(
                    div()
                        .absolute()
                        .left(px(VS_SIZE / 2.0 - 0.5))
                        .top_0()
                        .bottom_0()
                        .w(px(1.0))
                        .bg(rgb(0x334155))
                )
                .child(
                    div()
                        .absolute()
                        .top(px(VS_SIZE / 2.0 - 0.5))
                        .left_0()
                        .right_0()
                        .h(px(1.0))
                        .bg(rgb(0x334155))
                )
                .children(dot_els),
        )
}

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let scope_tab = app.scope_tab;

    // Pre-build tab buttons before the div chain (GPUI 0.2.2: closures using cx
    // must not be inside inline map chains on the div).
    let tabs: Vec<gpui::AnyElement> = [("Wave", 0u8), ("Vector", 1u8), ("Hist", 2u8)]
        .into_iter()
        .map(|(label, tab_id)| {
            let is_active = scope_tab == tab_id;
            div()
                .id(("scope-tab", tab_id as u64))
                .px(px(6.0))
                .py(px(2.0))
                .text_size(px(9.0))
                .cursor_pointer()
                .rounded_sm()
                .text_color(if is_active { colors::text_primary() } else { colors::text_secondary() })
                .bg(if is_active { colors::surface_raised() } else { colors::surface_bg() })
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetScopeTab(tab_id));
                    cx.notify();
                }))
                .child(label)
                .into_any_element()
        })
        .collect();

    let content = if let Some(scope) = &app.scope_data {
        match scope_tab {
            1 => vectorscope_chart(&scope.vectorscope_dots).into_any_element(),
            2 => histogram_chart(&scope.hist_r, &scope.hist_g, &scope.hist_b).into_any_element(),
            _ => waveform_chart(&scope.waveform_cols).into_any_element(),
        }
    } else {
        div()
            .p_2()
            .text_color(colors::text_disabled())
            .text_size(px(10.0))
            .child("No frame data — seek or play to populate scopes.")
            .into_any_element()
    };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Lumetri Scopes"))
        // Tab row
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1()
                .px_2()
                .py_1()
                .bg(colors::surface_bg())
                .border_b_1()
                .border_color(colors::surface_border())
                .children(tabs),
        )
        .child(content)
}
