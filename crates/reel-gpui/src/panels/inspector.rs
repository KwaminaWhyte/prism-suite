//! Inspector — detail of the selected clip (or the sequence when no clip is
//! selected), with interactive controls.
//!
//! Read-only rows reflect the selected clip's name / source / track / placement
//! / opacity. Interactive **stepper** rows wire numeric edits per the panel
//! convention: audio clips get a **volume** (gain) stepper, and visual clips get
//! a basic **color grade** (exposure / contrast / saturation). Each stepper
//! emits an [`Action`] through `root.app.apply` (the single mutation choke
//! point), so preview + export pick the change up.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, badge, divider, section_header};

use crate::app_state::{Action, App, ClipSource, ColorGrade, HslSecondaryGrade, MAX_AUDIO_GAIN, SpeedCurve};
use crate::Reel;
use crate::panels::color_wheels;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let mut rows: Vec<gpui::AnyElement> = Vec::new();

    match app.selected.and_then(|i| app.project.clips.get(i)) {
        Some(clip) => {
            let index = app.selected.unwrap();
            let source = match &clip.source {
                ClipSource::Color(c) => format!(
                    "Color  ({:.0}, {:.0}, {:.0})",
                    c[0] * 255.0,
                    c[1] * 255.0,
                    c[2] * 255.0
                ),
                ClipSource::Image(p) => format!(
                    "Image  {}",
                    p.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ),
                ClipSource::Video(v) => format!(
                    "Video  {}",
                    v.path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ),
                ClipSource::Audio(a) => format!(
                    "Audio  {}",
                    a.path.file_name().and_then(|n| n.to_str()).unwrap_or("?")
                ),
                ClipSource::Title { text, .. } => format!("Title  \"{}\"", text),
                ClipSource::NestedClip { duration_secs, clips, .. } =>
                    format!("Nested  {:.2}s ({} clips)", duration_secs, clips.len()),
            };
            rows.push(field("Name", clip.name.clone()).into_any_element());
            rows.push(field("Source", source).into_any_element());
            rows.push(field("Track", format!("V{}", clip.track + 1)).into_any_element());
            rows.push(field("Start", format!("{:.2}s", clip.start)).into_any_element());
            rows.push(field("Duration", format!("{:.2}s", clip.duration)).into_any_element());
            rows.push(field("End", format!("{:.2}s", clip.end())).into_any_element());
            rows.push(field("Opacity", format!("{:.0}%", clip.opacity * 100.0)).into_any_element());

            if let ClipSource::Audio(audio) = &clip.source {
                // --- Per-clip audio volume (gain) ---
                let gain = audio.effective_gain();
                rows.push(
                    stepper(
                        "vol",
                        "Volume",
                        format!("{:.0}%", gain * 100.0),
                        cx,
                        move |g| {
                            let next = (g - 0.1).max(0.0);
                            Action::SetClipGain { index, gain: next }
                        },
                        move |g| {
                            let next = (g + 0.1).min(MAX_AUDIO_GAIN);
                            Action::SetClipGain { index, gain: next }
                        },
                        gain,
                    )
                    .into_any_element(),
                );
            } else if let ClipSource::Title { text, font_size, .. } = &clip.source {
                // --- Title clip fields ---
                rows.push(section_header("Title").into_any_element());
                rows.push(field("Text", text.clone()).into_any_element());
                let fs = *font_size;
                rows.push(stepper_row(
                    "font-size", "Font Size",
                    format!("{:.0}pt", fs),
                    Action::SetTitleFontSize { index, size: (fs - 4.0).max(4.0) },
                    Action::SetTitleFontSize { index, size: fs + 4.0 },
                    cx,
                ).into_any_element());
            } else {
                // --- Basic color grade (visual clips) ---
                let grade = clip.grade;
                rows.push(section_header("Color Grade").into_any_element());
                rows.push(divider().into_any_element());
                rows.push(grade_stepper(
                    "exposure", "Exposure", grade.exposure, 0.25, index, cx,
                    move |g, v| ColorGrade { exposure: (g.exposure + v).clamp(-4.0, 4.0), ..g },
                ));
                rows.push(grade_stepper(
                    "contrast", "Contrast", grade.contrast, 0.1, index, cx,
                    move |g, v| ColorGrade { contrast: (g.contrast + v).clamp(-1.0, 1.0), ..g },
                ));
                rows.push(grade_stepper(
                    "saturation", "Saturation", grade.saturation, 0.1, index, cx,
                    move |g, v| ColorGrade { saturation: (g.saturation + v).clamp(0.0, 2.0), ..g },
                ));
                // --- Color Wheels (global, shown per selected visual clip) ---
                rows.push(color_wheels::render_section(app, cx).into_any_element());
                // --- RGB Curves (global, shown per selected visual clip) ---
                rows.push(crate::panels::curves::render_section(app, cx).into_any_element());

                // --- HSL Secondary (per-clip, keys a hue range and grades it) ---
                let hsl = clip.hsl_secondary;
                rows.push(section_header("HSL Secondary").into_any_element());
                rows.push(divider().into_any_element());
                rows.push(stepper_row(
                    "hsl-hue", "Hue °",
                    format!("{:.0}°", hsl.hue_center * 360.0),
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        hue_center: (hsl.hue_center - 1.0 / 36.0).rem_euclid(1.0), ..hsl } },
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        hue_center: (hsl.hue_center + 1.0 / 36.0).rem_euclid(1.0), ..hsl } },
                    cx,
                ).into_any_element());
                rows.push(stepper_row(
                    "hsl-width", "Range",
                    format!("{:.0}%", hsl.hue_width * 100.0),
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        hue_width: (hsl.hue_width - 0.05).clamp(0.0, 0.5), ..hsl } },
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        hue_width: (hsl.hue_width + 0.05).clamp(0.0, 0.5), ..hsl } },
                    cx,
                ).into_any_element());
                rows.push(stepper_row(
                    "hsl-sat", "Sat",
                    format!("{:+.2}", hsl.sat_offset),
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        sat_offset: (hsl.sat_offset - 0.1).clamp(-1.0, 1.0), ..hsl } },
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        sat_offset: (hsl.sat_offset + 0.1).clamp(-1.0, 1.0), ..hsl } },
                    cx,
                ).into_any_element());
                rows.push(stepper_row(
                    "hsl-lum", "Lum",
                    format!("{:+.2}", hsl.lum_offset),
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        lum_offset: (hsl.lum_offset - 0.1).clamp(-1.0, 1.0), ..hsl } },
                    Action::SetHslSecondaryGrade { clip_id: index, grade: HslSecondaryGrade {
                        lum_offset: (hsl.lum_offset + 0.1).clamp(-1.0, 1.0), ..hsl } },
                    cx,
                ).into_any_element());
            }

            // --- Proxy / optimized media (video clips) ---
            if matches!(&clip.source, ClipSource::Video(_)) {
                let has_proxy = clip.proxy_path.is_some();
                rows.push(section_header("Proxy").into_any_element());
                rows.push(divider().into_any_element());
                rows.push(
                    div()
                        .id(("proxy-attach", index))
                        .flex().items_center().justify_between()
                        .px_3().py_1().gap_2().cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            if root.app.project.clips.get(index).and_then(|c| c.proxy_path.clone()).is_some() {
                                root.app.apply(Action::ClearProxy { index });
                            } else if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Video", crate::app_state::VIDEO_EXTENSIONS)
                                .pick_file()
                            {
                                root.app.apply(Action::SetProxyPath { index, path });
                            }
                            cx.notify();
                        }))
                        .child(div().text_color(colors::text_secondary()).text_size(px(11.0))
                            .child(if has_proxy { "Proxy" } else { "Attach proxy…" }))
                        .child(if has_proxy {
                            badge("ON — click to clear").into_any_element()
                        } else {
                            div().px_2().py(px(2.0)).rounded_sm().bg(colors::surface_overlay())
                                .text_color(colors::text_secondary()).text_size(px(11.0))
                                .child("off").into_any_element()
                        })
                        .into_any_element(),
                );
            }

            // --- Fade in/out (all clip types) ---
            let fi = clip.fade_in;
            let fo = clip.fade_out;
            rows.push(section_header("Fade").into_any_element());
            rows.push(divider().into_any_element());
            rows.push(stepper_row(
                "fade-in", "Fade In",
                format!("{:.1}s", fi),
                Action::SetClipFadeIn { index, secs: (fi - 0.1).max(0.0) },
                Action::SetClipFadeIn { index, secs: fi + 0.1 },
                cx,
            ).into_any_element());
            rows.push(stepper_row(
                "fade-out", "Fade Out",
                format!("{:.1}s", fo),
                Action::SetClipFadeOut { index, secs: (fo - 0.1).max(0.0) },
                Action::SetClipFadeOut { index, secs: fo + 0.1 },
                cx,
            ).into_any_element());

            // --- Speed ramp (non-audio clips only) ---
            if !matches!(&clip.source, ClipSource::Audio(_)) {
                let speed_pct = clip.speed * 100.0;
                let rev = clip.reversed;
                rows.push(section_header("Speed").into_any_element());
                rows.push(divider().into_any_element());
                // Speed badge when != 1×
                if (clip.speed - 1.0).abs() > 0.01 {
                    rows.push(
                        div()
                            .px_3()
                            .py_1()
                            .child(badge(format!("{:.1}×", clip.speed)))
                            .into_any_element(),
                    );
                }
                rows.push(stepper_row(
                    "speed", "Speed %",
                    format!("{:.0}%", speed_pct),
                    Action::SetClipSpeed(index, (speed_pct - 10.0).max(1.0)),
                    Action::SetClipSpeed(index, (speed_pct + 10.0).min(1000.0)),
                    cx,
                ).into_any_element());
                rows.push(
                    div()
                        .id(("reverse", index))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .gap_2()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _e, _w, cx| {
                            let cur = root.app.project.clips.get(index)
                                .map(|c| c.reversed).unwrap_or(false);
                            root.app.apply(Action::SetClipReverse(index, !cur));
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(11.0))
                                .child("Reverse"),
                        )
                        // Reverse badge: accent-colored when active
                        .child(if rev {
                            badge("⟵ ON").into_any_element()
                        } else {
                            div()
                                .px_2()
                                .py(px(2.0))
                                .rounded_sm()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_secondary())
                                .text_size(px(11.0))
                                .child("off")
                                .into_any_element()
                        })
                        .into_any_element(),
                );
                // Bezier speed curve toggle
                let clip_id = index;
                let is_bezier = matches!(&clip.speed_curve, SpeedCurve::Bezier { .. });
                rows.push(
                    div()
                        .id(("bezier-toggle", index))
                        .flex()
                        .items_center()
                        .justify_between()
                        .px_3()
                        .py_1()
                        .gap_2()
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let cur = &root.app.project.clips[clip_id].speed_curve;
                            let new_curve = if matches!(cur, SpeedCurve::Constant) {
                                SpeedCurve::Bezier { p0: 0.0, p1: 0.33, p2: 0.67, p3: 1.0 }
                            } else {
                                SpeedCurve::Constant
                            };
                            root.app.apply(Action::SetSpeedCurve { clip_id, curve: new_curve });
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(11.0))
                                .child("Bezier"),
                        )
                        .child(if is_bezier {
                            div()
                                .px_2()
                                .py(px(2.0))
                                .rounded_sm()
                                .bg(colors::accent())
                                .text_color(colors::text_primary())
                                .text_size(px(11.0))
                                .child("ON")
                                .into_any_element()
                        } else {
                            div()
                                .px_2()
                                .py(px(2.0))
                                .rounded_sm()
                                .bg(colors::surface_overlay())
                                .text_color(colors::text_secondary())
                                .text_size(px(11.0))
                                .child("off")
                                .into_any_element()
                        })
                        .into_any_element(),
                );
            }
        }
        None => {
            let p = &app.project;
            rows.push(field("Sequence", p.name.clone()).into_any_element());
            rows.push(field("Resolution", format!("{}×{}", p.width, p.height)).into_any_element());
            rows.push(field("Frame rate", format!("{:.2} fps", p.fps)).into_any_element());
            rows.push(field("Duration", format!("{:.1}s", p.duration)).into_any_element());
            rows.push(field("Tracks", format!("{}", p.tracks.len())).into_any_element());
            rows.push(field("Clips", format!("{}", p.clips.len())).into_any_element());

            // --- White balance ---
            let temp = app.white_balance_temp;
            let tint = app.white_balance_tint;
            rows.push(section_header("White Balance").into_any_element());
            rows.push(divider().into_any_element());
            rows.push(stepper_row(
                "wb-temp", "Temp (K)",
                format!("{:.0}K", temp),
                Action::SetWhiteBalance { temp: (temp - 250.0).max(2000.0), tint },
                Action::SetWhiteBalance { temp: (temp + 250.0).min(10000.0), tint },
                cx,
            ).into_any_element());
            rows.push(stepper_row(
                "wb-tint", "Tint",
                format!("{:+.0}", tint),
                Action::SetWhiteBalance { temp, tint: (tint - 10.0).max(-150.0) },
                Action::SetWhiteBalance { temp, tint: (tint + 10.0).min(150.0) },
                cx,
            ).into_any_element());

            // --- LUT preview bars (before / after) ---
            if let Some(lut) = &app.lut_table {
                rows.push(section_header("LUT Preview").into_any_element());
                rows.push(divider().into_any_element());
                // "Before" bar: linear grey ramp, 64 cells × 2px wide × 12px tall.
                let before_bar = div()
                    .flex()
                    .h(px(12.0))
                    .mx_3()
                    .children((0u32..64).map(|i| {
                        let v = (i * 255 / 63) as u32;
                        let grey = (v << 16) | (v << 8) | v;
                        div().w(px(2.0)).h_full().bg(rgb(grey))
                    }));
                // "After" bar: grey ramp mapped through the LUT diagonal.
                let lut_sampled: Vec<u32> = (0u32..64)
                    .map(|i| {
                        let grey = i as f32 / 63.0;
                        let rgb_out = lut_sample_grey(lut, grey);
                        ((rgb_out[0] as u32) << 16)
                            | ((rgb_out[1] as u32) << 8)
                            | (rgb_out[2] as u32)
                    })
                    .collect();
                let after_bar = div()
                    .flex()
                    .h(px(12.0))
                    .mx_3()
                    .children(lut_sampled.into_iter().map(|c| {
                        div().w(px(2.0)).h_full().bg(rgb(c))
                    }));
                rows.push(
                    div()
                        .px_3()
                        .py_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child("Before")
                        .into_any_element(),
                );
                rows.push(before_bar.into_any_element());
                rows.push(
                    div()
                        .px_3()
                        .py_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child("After")
                        .into_any_element(),
                );
                rows.push(after_bar.into_any_element());
            }
        }
    };

    div()
        .id("inspector-panel")
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Inspector"))
        .child(divider())
        .children(rows)
}

/// A read-only labeled field row: muted label left, value right.
fn field(label: &str, value: String) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_1()
        .gap_2()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .px_2()
                .py(px(2.0))
                .rounded_sm()
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child(value),
        )
}

/// A −/value/+ stepper row that emits an [`Action`] on each button. `dec` / `inc`
/// build the action from the current value `cur`. Used for the audio gain row.
#[allow(clippy::too_many_arguments)]
fn stepper(
    id: &'static str,
    label: &'static str,
    value: String,
    cx: &mut Context<Reel>,
    dec: impl Fn(f32) -> Action + 'static,
    inc: impl Fn(f32) -> Action + 'static,
    cur: f32,
) -> impl IntoElement {
    stepper_row(id, label, value, dec(cur), inc(cur), cx)
}

/// A grade stepper row: −/value/+ buttons stepping one [`ColorGrade`] control by
/// `step`. `update(grade, delta)` returns the new grade with the control nudged.
#[allow(clippy::too_many_arguments)]
fn grade_stepper(
    id: &'static str,
    label: &'static str,
    cur_val: f32,
    step: f32,
    index: usize,
    cx: &mut Context<Reel>,
    update: impl Fn(ColorGrade, f32) -> ColorGrade + Copy + 'static,
) -> gpui::AnyElement {
    let value = format!("{cur_val:+.2}");
    // The dec/inc actions re-read the clip's grade in the listener so each click
    // composes on the latest state (the closure captures `index` + `update`).
    let dec_action = move |root: &mut Reel| {
        if let Some(clip) = root.app.project.clips.get(index) {
            let grade = update(clip.grade, -step);
            root.app.apply(Action::SetClipGrade { index, grade });
        }
    };
    let inc_action = move |root: &mut Reel| {
        if let Some(clip) = root.app.project.clips.get(index) {
            let grade = update(clip.grade, step);
            root.app.apply(Action::SetClipGrade { index, grade });
        }
    };
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_1()
        .gap_2()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(step_btn(format!("{id}-dec").into(), "−", cx.listener(move |root, _e, _w, cx| {
                    dec_action(root);
                    cx.notify();
                })))
                .child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded_sm()
                        .bg(colors::surface_overlay())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .min_w(px(44.0))
                        .child(value),
                )
                .child(step_btn(format!("{id}-inc").into(), "+", cx.listener(move |root, _e, _w, cx| {
                    inc_action(root);
                    cx.notify();
                }))),
        )
        .into_any_element()
}

/// A −/value/+ row driven by two concrete actions (the audio gain path).
fn stepper_row(
    id: &'static str,
    label: &'static str,
    value: String,
    dec_action: Action,
    inc_action: Action,
    cx: &mut Context<Reel>,
) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .px_3()
        .py_1()
        .gap_2()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(label.to_string()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(step_btn(format!("{id}-dec").into(), "−", cx.listener(move |root, _e, _w, cx| {
                    root.app.apply(dec_action.clone());
                    cx.notify();
                })))
                .child(
                    div()
                        .px_2()
                        .py(px(2.0))
                        .rounded_sm()
                        .bg(colors::surface_overlay())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .min_w(px(44.0))
                        .child(value),
                )
                .child(step_btn(format!("{id}-inc").into(), "+", cx.listener(move |root, _e, _w, cx| {
                    root.app.apply(inc_action.clone());
                    cx.notify();
                }))),
        )
}

/// A small square +/− button. `on_click` is a pre-built `cx.listener` closure.
fn step_btn(
    id: gpui::SharedString,
    glyph: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(20.0))
        .h(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .bg(colors::surface_raised())
        .text_color(colors::text_primary())
        .text_size(px(12.0))
        .cursor_pointer()
        .on_click(on_click)
        .child(glyph)
}

/// Sample a loaded 3D LUT along the grey diagonal (equal R=G=B=grey) for a
/// preview ramp. Returns RGB u8 output.
fn lut_sample_grey(lut: &(u32, Vec<[f32; 3]>), grey: f32) -> [u8; 3] {
    let (size, table) = lut;
    let s = (*size as f32) - 1.0;
    let ri = (grey * s).clamp(0.0, s);
    let r0 = ri.floor() as usize;
    let r1 = (r0 + 1).min(*size as usize - 1);
    let rf = ri - ri.floor();
    let n = *size as usize;
    let c0 = table[r0 + r0 * n + r0 * n * n];
    let c1 = table[r1 + r1 * n + r1 * n * n];
    [
        ((c0[0] + rf * (c1[0] - c0[0])).clamp(0.0, 1.0) * 255.0) as u8,
        ((c0[1] + rf * (c1[1] - c0[1])).clamp(0.0, 1.0) * 255.0) as u8,
        ((c0[2] + rf * (c1[2] - c0[2])).clamp(0.0, 1.0) * 255.0) as u8,
    ]
}
