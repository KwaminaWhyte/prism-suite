//! Audio mixer panel — one column per track plus a master fader.
//!
//! Shows when `app.show_mixer` is true (toggled by the toolbar "Mixer" button).
//! Each column has a vertical volume fader (0–200%), mute toggle, solo toggle,
//! and a track label. The master fader is at the right.

use gpui::{
    div, px, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, Icon};
use prism_ui::components::section_header;

use crate::app_state::{Action, App, AudioEffect, AudioTrackType};
use crate::Reel;

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let any_solo = app.track_soloed.iter().any(|&s| s);

    let track_cols = app
        .project
        .tracks
        .iter()
        .enumerate()
        .map(|(ti, track)| {
            let vol = app.track_volumes.get(ti).copied().unwrap_or(1.0);
            let muted = app.track_muted.get(ti).copied().unwrap_or(false);
            let soloed = app.track_soloed.get(ti).copied().unwrap_or(false);
            let effective = !muted && (!any_solo || soloed);
            let label = track.name.clone();
            let vol_pct = format!("{:.0}%", vol * 100.0);
            let eq_on = app.track_eq.get(ti).map(|e| e.enabled).unwrap_or(false);
            let comp_on = app.track_comp.get(ti).map(|c| c.enabled).unwrap_or(false);
            let track_type = app.track_types.get(ti).copied().unwrap_or(AudioTrackType::Stereo);
            let log_on = app.track_log_transform.get(ti).copied().unwrap_or(false);
            let eq_bands: Vec<(usize, f32)> = app.track_eq.get(ti)
                .map(|e| e.bands.iter().enumerate().map(|(i, b)| (i, b.gain_db)).collect())
                .unwrap_or_default();
            let eq3 = app.track_eq3.get(ti).cloned().unwrap_or_default();
            let eq3_low = eq3.low_gain_db;
            let eq3_mid = eq3.mid_gain_db;
            let eq3_high = eq3.high_gain_db;
            let fx_open = app.track_fx_open.get(ti).copied().unwrap_or(false);
            let fx_labels: Vec<String> = app.audio_effects.get(ti)
                .map(|chain| chain.iter().map(|e| e.label().to_string()).collect())
                .unwrap_or_default();

            // Pre-build FX chip elements before entering the div chain.
            let mut fx_chips: Vec<gpui::AnyElement> = Vec::new();
            if fx_open {
                for (ei, label) in fx_labels.iter().enumerate() {
                    let rm_id = gpui::SharedString::from(format!("fx-rm-{}-{}", ti, ei));
                    fx_chips.push(
                        div()
                            .flex().flex_row().items_center().gap(px(1.0))
                            .child(div().text_color(colors::text_secondary()).text_size(px(7.0)).child(label.clone()))
                            .child(
                                div().id(rm_id).w(px(10.0)).h(px(10.0))
                                    .flex().items_center().justify_center()
                                    .text_color(colors::text_secondary()).text_size(px(8.0))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                                        root.app.apply(Action::RemoveAudioEffect { track_idx: ti, effect_idx: ei });
                                        cx.notify();
                                    }))
                                    .child("\u{d7}")
                            )
                            .into_any_element()
                    );
                }
                // Add EQ button
                let n_fx = fx_labels.len();
                let add_fx_id = gpui::SharedString::from(format!("fx-add-eq-{}-{}", ti, n_fx));
                fx_chips.push(
                    div().id(add_fx_id).px(px(2.0)).py(px(1.0))
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_secondary()).text_size(px(7.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::AddAudioEffect {
                                track_idx: ti,
                                effect: AudioEffect::Eq3(crate::app_state::TrackEq3::default()),
                            });
                            cx.notify();
                        }))
                        .child("+EQ")
                        .into_any_element()
                );
                // Add Reverb button.
                let add_rev_id = gpui::SharedString::from(format!("fx-add-rev-{}-{}", ti, n_fx));
                fx_chips.push(
                    div().id(add_rev_id).px(px(2.0)).py(px(1.0))
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_secondary()).text_size(px(7.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::AddAudioEffect {
                                track_idx: ti,
                                effect: AudioEffect::Reverb { room_size: 0.6, damping: 0.4, wet: 0.3 },
                            });
                            cx.notify();
                        }))
                        .child("+Rev")
                        .into_any_element()
                );
                // Add Delay button.
                let add_dly_id = gpui::SharedString::from(format!("fx-add-dly-{}-{}", ti, n_fx));
                fx_chips.push(
                    div().id(add_dly_id).px(px(2.0)).py(px(1.0))
                        .rounded_sm().bg(colors::surface_raised())
                        .text_color(colors::text_secondary()).text_size(px(7.0)).cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::AddAudioEffect {
                                track_idx: ti,
                                effect: AudioEffect::Delay { time_ms: 250.0, feedback: 0.4, wet: 0.3 },
                            });
                            cx.notify();
                        }))
                        .child("+Dly")
                        .into_any_element()
                );
            }

            let mute_bg  = if muted  { colors::danger()   } else { colors::surface_overlay() };
            let solo_bg  = if soloed { colors::warning()  } else { colors::surface_overlay() };
            let fader_fill = if effective { colors::accent() } else { colors::surface_border() };

            let fader_h = (vol * 40.0).clamp(0.0, 80.0);

            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .p_2()
                .w(px(56.0))
                .border_r_1()
                .border_color(colors::surface_border())
                // Volume icon header row
                .child(
                    svg()
                        .path(Icon::Volume.path())
                        .w(px(12.0))
                        .h(px(12.0))
                        .text_color(colors::text_secondary()),
                )
                .child(
                    div()
                        .relative()
                        .w(px(16.0))
                        .h(px(80.0))
                        .bg(colors::surface_overlay())
                        .rounded_sm()
                        .overflow_hidden()
                        .child(
                            div()
                                .absolute()
                                .bottom_0()
                                .w_full()
                                .h(px(fader_h))
                                .bg(fader_fill),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.0))
                        .child(
                            div()
                                .id(("mix-dec", ti))
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .bg(colors::surface_raised())
                                .text_color(colors::text_primary())
                                .text_size(px(10.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    let cur = root.app.track_volumes.get(ti).copied().unwrap_or(1.0);
                                    root.app.apply(Action::SetTrackVolume {
                                        track_idx: ti,
                                        v: (cur - 0.1).max(0.0),
                                    });
                                    cx.notify();
                                }))
                                .child("−"),
                        )
                        .child(
                            div()
                                .text_color(colors::text_primary())
                                .text_size(px(9.0))
                                .child(vol_pct),
                        )
                        .child(
                            div()
                                .id(("mix-inc", ti))
                                .w(px(14.0))
                                .h(px(14.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_sm()
                                .bg(colors::surface_raised())
                                .text_color(colors::text_primary())
                                .text_size(px(10.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    let cur = root.app.track_volumes.get(ti).copied().unwrap_or(1.0);
                                    root.app.apply(Action::SetTrackVolume {
                                        track_idx: ti,
                                        v: (cur + 0.1).min(2.0),
                                    });
                                    cx.notify();
                                }))
                                .child("+"),
                        ),
                )
                // Mute — icon button (Icon::Mute)
                .child(
                    div()
                        .id(("mix-mute", ti))
                        .w(px(24.0))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(mute_bg)
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleTrackMute { track_idx: ti });
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(Icon::Mute.path())
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(colors::text_primary()),
                        ),
                )
                // Solo — icon button (Icon::Star)
                .child(
                    div()
                        .id(("mix-solo", ti))
                        .w(px(24.0))
                        .h(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(solo_bg)
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleTrackSolo { track_idx: ti });
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .path(Icon::Star.path())
                                .w(px(12.0))
                                .h(px(12.0))
                                .text_color(colors::text_primary()),
                        ),
                )
                // Track type badge (M / ST / 5.1) — click to cycle.
                .child(
                    div()
                        .id(("track-type", ti))
                        .px(px(3.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_secondary())
                        .text_size(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::CycleAudioTrackType { track_idx: ti });
                            cx.notify();
                        }))
                        .child(track_type.label()),
                )
                // Log→Rec709 toggle per track.
                .child(
                    div()
                        .id(("log-btn", ti))
                        .px(px(3.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(if log_on { colors::accent() } else { colors::surface_raised() })
                        .text_color(if log_on { colors::text_primary() } else { colors::text_secondary() })
                        .text_size(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleLogTransform { track_idx: ti });
                            cx.notify();
                        }))
                        .child("Log"),
                )
                // EQ toggle + per-band gain indicators.
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(1.0))
                        .child(
                            div()
                                .id(("eq-btn", ti))
                                .px(px(3.0))
                                .py(px(1.0))
                                .rounded_sm()
                                .bg(if eq_on { colors::accent() } else { colors::surface_raised() })
                                .text_color(colors::text_primary())
                                .text_size(px(8.0))
                                .cursor_pointer()
                                .on_click(cx.listener(move |root, _ev, _win, cx| {
                                    root.app.apply(Action::ToggleTrackEq { track_idx: ti });
                                    cx.notify();
                                }))
                                .child("EQ"),
                        )
                        .children(eq_bands.into_iter().map(|(band, gain)| {
                            let band_labels = ["80", "250", "1k", "4k", "12k"];
                            let label = band_labels.get(band).copied().unwrap_or("?");
                            let fill_h = ((gain + 12.0) / 24.0 * 20.0).clamp(0.0, 20.0);
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .relative()
                                        .w(px(8.0))
                                        .h(px(20.0))
                                        .bg(colors::surface_overlay())
                                        .rounded_sm()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .absolute()
                                                .bottom_0()
                                                .w_full()
                                                .h(px(fill_h))
                                                .bg(if eq_on { colors::accent() } else { colors::surface_border() }),
                                        )
                                        .id(("eq-band-inc", ti * 10 + band))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let cur = root.app.track_eq.get(ti)
                                                .map(|e| e.bands[band].gain_db).unwrap_or(0.0);
                                            root.app.apply(Action::SetEqBand { track_idx: ti, band, gain_db: (cur + 1.0).min(12.0) });
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .text_color(colors::text_secondary())
                                        .text_size(px(7.0))
                                        .child(label),
                                )
                        })),
                )
                // Compressor toggle.
                .child(
                    div()
                        .id(("comp-btn", ti))
                        .px(px(3.0))
                        .py(px(1.0))
                        .rounded_sm()
                        .bg(if comp_on { colors::warning() } else { colors::surface_raised() })
                        .text_color(colors::text_primary())
                        .text_size(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleTrackCompressor { track_idx: ti });
                            cx.notify();
                        }))
                        .child("Cmp"),
                )
                // FX chain toggle + effect chips
                .child(
                    div()
                        .id(("fx-btn", ti))
                        .px(px(3.0)).py(px(1.0))
                        .rounded_sm()
                        .bg(if fx_open { colors::accent() } else { colors::surface_raised() })
                        .text_color(colors::text_primary())
                        .text_size(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleTrackFx { track_idx: ti });
                            cx.notify();
                        }))
                        .child("FX"),
                )
                .children(fx_chips)
                // 3-Band EQ section
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(1.0))
                        .child(
                            div()
                                .text_color(colors::text_secondary())
                                .text_size(px(7.0))
                                .child("3-Band EQ"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .id(("eq3-low-dec", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.low_gain_db = (eq.low_gain_db - 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("−"),
                                )
                                .child(
                                    div()
                                        .text_color(colors::text_secondary())
                                        .text_size(px(8.0))
                                        .child(format!("L{:+.0}", eq3_low)),
                                )
                                .child(
                                    div()
                                        .id(("eq3-low-inc", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.low_gain_db = (eq.low_gain_db + 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("+"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .id(("eq3-mid-dec", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.mid_gain_db = (eq.mid_gain_db - 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("−"),
                                )
                                .child(
                                    div()
                                        .text_color(colors::text_secondary())
                                        .text_size(px(8.0))
                                        .child(format!("M{:+.0}", eq3_mid)),
                                )
                                .child(
                                    div()
                                        .id(("eq3-mid-inc", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.mid_gain_db = (eq.mid_gain_db + 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("+"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .id(("eq3-high-dec", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.high_gain_db = (eq.high_gain_db - 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("−"),
                                )
                                .child(
                                    div()
                                        .text_color(colors::text_secondary())
                                        .text_size(px(8.0))
                                        .child(format!("H{:+.0}", eq3_high)),
                                )
                                .child(
                                    div()
                                        .id(("eq3-high-inc", ti))
                                        .w(px(12.0))
                                        .h(px(12.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded_sm()
                                        .bg(colors::surface_raised())
                                        .text_color(colors::text_primary())
                                        .text_size(px(9.0))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                                            let mut eq = root.app.track_eq3.get(ti).cloned().unwrap_or_default();
                                            eq.high_gain_db = (eq.high_gain_db + 1.0).clamp(-12.0, 12.0);
                                            root.app.apply(Action::SetTrackEq3 { track_idx: ti, eq });
                                            cx.notify();
                                        }))
                                        .child("+"),
                                ),
                        ),
                )
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(9.0))
                        .child(label),
                )
        })
        .collect::<Vec<_>>();

    let master = app.master_volume;
    let master_pct = format!("{:.0}%", master * 100.0);
    let master_h = (master * 40.0).clamp(0.0, 80.0);
    let master_col = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .p_2()
        .w(px(64.0))
        .child(
            svg()
                .path(Icon::Volume.path())
                .w(px(12.0))
                .h(px(12.0))
                .text_color(colors::text_secondary()),
        )
        .child(
            div()
                .relative()
                .w(px(16.0))
                .h(px(80.0))
                .bg(colors::surface_overlay())
                .rounded_sm()
                .overflow_hidden()
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .w_full()
                        .h(px(master_h))
                        .bg(colors::accent()),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(2.0))
                .child(
                    div()
                        .id("master-dec")
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let cur = root.app.master_volume;
                            root.app.apply(Action::SetMasterVolume { v: (cur - 0.1).max(0.0) });
                            cx.notify();
                        }))
                        .child("−"),
                )
                .child(
                    div()
                        .text_color(colors::text_primary())
                        .text_size(px(9.0))
                        .child(master_pct),
                )
                .child(
                    div()
                        .id("master-inc")
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(10.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let cur = root.app.master_volume;
                            root.app.apply(Action::SetMasterVolume { v: (cur + 0.1).min(2.0) });
                            cx.notify();
                        }))
                        .child("+"),
                ),
        )
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("Master"),
        );

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Mixer"))
        .child(
            div()
                .flex()
                .flex_row()
                .bg(colors::surface_raised())
                .children(track_cols)
                .child(master_col),
        )
}
