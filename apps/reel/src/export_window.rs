//! Export dialog — a floating OS-level child window exposing the Batch-5 export
//! codec matrix.
//!
//! Holds a [`WeakEntity<Reel>`] (same pattern as the welcome / preferences
//! windows). The left column lists every preset in `app.export_presets_b5` and
//! selects one ([`Action::SelectExportPresetB5`]); the right column edits the
//! active preset's container / video codec / audio codec / bitrate / resolution
//! through the existing `SetExport*B5` actions, and can duplicate it
//! ([`Action::DuplicateExportPresetB5`]).

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::reel_project::{
    AudioCodecB5, ExportContainer, ExportPresetB5, VideoCodecB5,
};
use crate::app_state::Action;
use crate::Reel;

pub struct ExportView {
    focus: FocusHandle,
    app_entity: WeakEntity<Reel>,
}

impl Focusable for ExportView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ExportView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Reel>) -> Self {
        Self { focus, app_entity }
    }

    fn dispatch(&self, cx: &mut Context<Self>, action: Action) {
        if let Some(entity) = self.app_entity.upgrade() {
            entity.update(cx, |reel, cx| {
                reel.app.apply(action);
                cx.notify();
            });
        }
    }

    /// Clone the preset list + the active preset so the render borrow ends
    /// before we build listeners that re-borrow the entity.
    fn snapshot(&self, cx: &mut Context<Self>) -> (Vec<ExportPresetB5>, Option<usize>) {
        self.app_entity
            .upgrade()
            .map(|e| {
                let reel = e.read(cx);
                (reel.app.export_presets_b5.clone(), reel.app.active_export_preset_b5)
            })
            .unwrap_or((Vec::new(), None))
    }
}

const CONTAINERS: [(ExportContainer, &str); 8] = [
    (ExportContainer::Mp4, "MP4"),
    (ExportContainer::Mov, "MOV"),
    (ExportContainer::Mxf, "MXF"),
    (ExportContainer::Mkv, "MKV"),
    (ExportContainer::Avi, "AVI"),
    (ExportContainer::Gif, "GIF"),
    (ExportContainer::Webm, "WebM"),
    (ExportContainer::M4v, "M4V"),
];

const VIDEO_CODECS: [(VideoCodecB5, &str); 13] = [
    (VideoCodecB5::H264, "H.264"),
    (VideoCodecB5::H265, "H.265"),
    (VideoCodecB5::ProRes422, "ProRes 422"),
    (VideoCodecB5::ProRes422Hq, "ProRes 422 HQ"),
    (VideoCodecB5::ProRes422Lt, "ProRes 422 LT"),
    (VideoCodecB5::ProRes422Proxy, "ProRes Proxy"),
    (VideoCodecB5::ProRes4444, "ProRes 4444"),
    (VideoCodecB5::Dnxhd, "DNxHD"),
    (VideoCodecB5::Dnxhr, "DNxHR"),
    (VideoCodecB5::Av1, "AV1"),
    (VideoCodecB5::Vp9, "VP9"),
    (VideoCodecB5::Mpeg2, "MPEG-2"),
    (VideoCodecB5::None, "None"),
];

const AUDIO_CODECS: [(AudioCodecB5, &str); 6] = [
    (AudioCodecB5::Aac, "AAC"),
    (AudioCodecB5::Mp3, "MP3"),
    (AudioCodecB5::Pcm, "PCM"),
    (AudioCodecB5::Alac, "ALAC"),
    (AudioCodecB5::Ac3, "AC-3"),
    (AudioCodecB5::None, "None"),
];

/// A row of selectable chips for an enum field.
fn chip_row<T: Copy + PartialEq + 'static>(
    id_prefix: &'static str,
    options: &[(T, &'static str)],
    current: T,
    make_action: impl Fn(T) -> Action + Copy + 'static,
    cx: &mut Context<ExportView>,
) -> gpui::AnyElement {
    let chips: Vec<gpui::AnyElement> = options
        .iter()
        .enumerate()
        .map(|(i, (val, label))| {
            let val = *val;
            let active = val == current;
            div()
                .id(gpui::SharedString::from(format!("{id_prefix}-{i}")))
                .px(px(7.0)).py(px(2.0))
                .rounded(px(3.0))
                .bg(if active { colors::accent() } else { colors::surface_overlay() })
                .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
                .text_size(px(10.0))
                .cursor_pointer()
                .hover(|s| s.bg(if active { colors::accent_hover() } else { colors::tool_hover() }))
                .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                    this.dispatch(cx, make_action(val));
                    cx.notify();
                }))
                .child(*label)
                .into_any_element()
        })
        .collect();

    div().flex().flex_row().flex_wrap().gap(px(4.0)).children(chips).into_any_element()
}

fn field_block(label: &'static str, control: gpui::AnyElement) -> impl IntoElement {
    div()
        .flex().flex_col().gap(px(4.0)).mb(px(10.0))
        .child(
            div().text_color(colors::text_secondary()).text_size(px(font_size::XS))
                .child(label),
        )
        .child(control)
}

/// A labelled +/- stepper for a numeric preset field.
fn stepper(
    id: &'static str,
    value: String,
    dec: Action,
    inc: Action,
    cx: &mut Context<ExportView>,
) -> gpui::AnyElement {
    div()
        .flex().flex_row().items_center().gap(px(4.0))
        .child(
            div()
                .id(gpui::SharedString::from(format!("{id}-dec")))
                .w(px(20.0)).h(px(18.0))
                .flex().items_center().justify_center()
                .rounded(px(3.0)).bg(colors::surface_raised())
                .text_color(colors::text_primary()).text_size(px(13.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                    this.dispatch(cx, dec.clone());
                    cx.notify();
                }))
                .child("\u{2212}"),
        )
        .child(
            div()
                .px(px(8.0)).py(px(2.0)).min_w(px(72.0))
                .rounded(px(3.0)).bg(colors::surface_overlay())
                .text_color(colors::text_primary()).text_size(px(11.0))
                .child(value),
        )
        .child(
            div()
                .id(gpui::SharedString::from(format!("{id}-inc")))
                .w(px(20.0)).h(px(18.0))
                .flex().items_center().justify_center()
                .rounded(px(3.0)).bg(colors::surface_raised())
                .text_color(colors::text_primary()).text_size(px(13.0))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                    this.dispatch(cx, inc.clone());
                    cx.notify();
                }))
                .child("+"),
        )
        .into_any_element()
}

impl Render for ExportView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (presets, active_id) = self.snapshot(cx);

        // ---- Left: preset list ----
        let preset_rows: Vec<gpui::AnyElement> = presets
            .iter()
            .map(|p| {
                let pid = p.id;
                let active = active_id == Some(pid);
                let name = p.name.clone();
                let sub = format!("{:?} · {:?}", p.container, p.video_codec);
                div()
                    .id(gpui::SharedString::from(format!("preset-{pid}")))
                    .px(px(10.0)).py(px(6.0))
                    .border_b_1().border_color(colors::surface_border())
                    .bg(if active { colors::surface_overlay() } else { colors::surface_raised() })
                    .cursor_pointer()
                    .hover(|s| s.bg(colors::tool_hover()))
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, Action::SelectExportPresetB5 { preset_id: pid });
                        cx.notify();
                    }))
                    .child(div().text_color(colors::text_primary()).text_size(px(11.0)).child(name))
                    .child(div().text_color(colors::text_secondary()).text_size(px(9.0)).child(sub))
                    .into_any_element()
            })
            .collect();

        let left = div()
            .w(px(220.0)).h_full()
            .flex().flex_col()
            .bg(colors::surface_raised())
            .border_r_1().border_color(colors::surface_border())
            .child(
                div().px(px(10.0)).py(px(8.0))
                    .text_color(colors::text_secondary()).text_size(px(font_size::SM))
                    .child("PRESETS"),
            )
            .child(div().id("preset-scroll").flex_1().overflow_y_scroll().children(preset_rows));

        // ---- Right: codec matrix editor for the active preset ----
        let active = active_id.and_then(|id| presets.iter().find(|p| p.id == id).cloned());

        let right_body: gpui::AnyElement = if let Some(p) = active {
            let w = p.width;
            let h = p.height;
            let vbr = p.video_bitrate_kbps;
            let abr = p.audio_bitrate_kbps;

            let container_row = chip_row(
                "container", &CONTAINERS, p.container,
                Action::SetExportContainerB5, cx,
            );
            let vcodec_row = chip_row(
                "vcodec", &VIDEO_CODECS, p.video_codec,
                Action::SetExportVideoCodecB5, cx,
            );
            let acodec_row = chip_row(
                "acodec", &AUDIO_CODECS, p.audio_codec,
                Action::SetExportAudioCodecB5, cx,
            );

            let width_step = stepper(
                "exp-w", format!("{w} px"),
                Action::SetExportWidthB5(w.saturating_sub(160).max(160)),
                Action::SetExportWidthB5(w + 160),
                cx,
            );
            let height_step = stepper(
                "exp-h", format!("{h} px"),
                Action::SetExportHeightB5(h.saturating_sub(90).max(90)),
                Action::SetExportHeightB5(h + 90),
                cx,
            );
            let vbr_step = stepper(
                "exp-vbr", format!("{vbr} kbps"),
                Action::SetExportVideoBitrateB5(vbr.saturating_sub(1000)),
                Action::SetExportVideoBitrateB5(vbr + 1000),
                cx,
            );
            let abr_step = stepper(
                "exp-abr", format!("{abr} kbps"),
                Action::SetExportAudioBitrateB5(abr.saturating_sub(32)),
                Action::SetExportAudioBitrateB5(abr + 32),
                cx,
            );

            // Two-pass + hardware-encode toggles.
            let two_pass = p.two_pass;
            let hw = p.hardware_encode;
            let toggle = |id: &'static str, on: bool, on_label: &'static str,
                          off_label: &'static str, act: Action, cx: &mut Context<ExportView>| {
                div()
                    .id(gpui::SharedString::from(id))
                    .px(px(8.0)).py(px(2.0))
                    .rounded(px(3.0))
                    .bg(if on { colors::accent() } else { colors::surface_overlay() })
                    .text_color(colors::text_primary()).text_size(px(10.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                        this.dispatch(cx, act.clone());
                        cx.notify();
                    }))
                    .child(if on { on_label } else { off_label })
                    .into_any_element()
            };
            let two_pass_btn = toggle("exp-2pass", two_pass, "Two-pass: On", "Two-pass: Off",
                Action::SetExportTwoPassB5(!two_pass), cx);
            let hw_btn = toggle("exp-hw", hw, "HW Encode: On", "HW Encode: Off",
                Action::SetExportHardwareEncodeB5(!hw), cx);

            div()
                .id("export-editor-scroll")
                .flex().flex_col().flex_1()
                .p(px(16.0))
                .overflow_y_scroll()
                .child(
                    div().text_size(px(font_size::MD)).text_color(colors::text_primary())
                        .mb(px(12.0)).child(p.name.clone()),
                )
                .child(field_block("Container", container_row))
                .child(field_block("Video codec", vcodec_row))
                .child(field_block("Audio codec", acodec_row))
                .child(field_block("Width", width_step))
                .child(field_block("Height", height_step))
                .child(field_block("Video bitrate", vbr_step))
                .child(field_block("Audio bitrate", abr_step))
                .child(
                    div().flex().flex_row().gap(px(6.0)).mt(px(4.0))
                        .child(two_pass_btn).child(hw_btn),
                )
                .into_any_element()
        } else {
            div()
                .flex_1().flex().items_center().justify_center()
                .text_color(colors::text_disabled()).text_size(px(font_size::SM))
                .child("Select a preset to edit its codec settings.")
                .into_any_element()
        };

        // ---- Footer: duplicate + close ----
        let dup_btn = active_id.map(|id| {
            div()
                .id("exp-dup")
                .px(px(10.0)).py(px(5.0))
                .rounded(px(4.0)).bg(colors::surface_raised())
                .text_color(colors::text_secondary()).text_size(px(font_size::SM))
                .cursor_pointer()
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                    this.dispatch(cx, Action::DuplicateExportPresetB5 { preset_id: id });
                    cx.notify();
                }))
                .child("Duplicate Preset")
        });

        let close_btn = div()
            .id("exp-close")
            .px(px(12.0)).py(px(5.0))
            .rounded(px(4.0)).bg(colors::accent())
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |_this, _e: &ClickEvent, win, _cx| {
                win.remove_window();
            }))
            .child("Done");

        div()
            .size_full()
            .flex().flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            // Header
            .child(
                div()
                    .px(px(16.0)).py(px(10.0))
                    .border_b_1().border_color(colors::surface_border())
                    .text_size(px(18.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("Export Settings"),
            )
            // Body: presets | editor
            .child(
                div()
                    .flex_1().flex().flex_row().min_h(px(0.0))
                    .child(left)
                    .child(right_body),
            )
            // Footer
            .child(
                div()
                    .flex().flex_row().items_center().justify_between()
                    .px(px(16.0)).py(px(10.0))
                    .border_t_1().border_color(colors::surface_border())
                    .children(dup_btn)
                    .child(close_btn),
            )
    }
}
