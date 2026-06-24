//! Inspector — fill/stroke colour section. Split out of `inspector.rs`.

use super::*;

/// A labelled colour block: swatch + hex readout, R/G/B steppers, and a row of
/// preset swatches. Edits emit `SetFillColor` / `SetStrokeColor` per `target`.
pub(super) fn color_section(
    title: &'static str,
    target: Target,
    c: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let (r8, g8, b8) = (to_u8(c[0]), to_u8(c[1]), to_u8(c[2]));

    let channels = [("R", 0usize), ("G", 1usize), ("B", 2usize)]
        .into_iter()
        .map(|(label, idx)| channel_row(title, target, label, idx, c, cx))
        .collect::<Vec<_>>();

    let presets: [[f32; 4]; 8] = [
        [0.0, 0.0, 0.0, c[3]],   // black
        [1.0, 1.0, 1.0, c[3]],   // white
        [0.5, 0.5, 0.5, c[3]],   // gray
        [1.0, 0.0, 0.0, c[3]],   // red
        [0.0, 0.8, 0.0, c[3]],   // green
        [0.0, 0.4, 1.0, c[3]],   // blue
        [1.0, 0.85, 0.0, c[3]],  // yellow
        [1.0, 0.0, 0.85, c[3]],  // magenta
    ];
    let preset_cells = presets
        .into_iter()
        .enumerate()
        .map(|(i, p)| swatch_cell((preset_id(title), i as u64), target, p, cx))
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(ui_section_header(title))
        .child(ui_divider())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .size(px(40.0))
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(SWATCH_BORDER))
                        .bg(rgba(pack(c))),
                )
                .child(
                    div()
                        .text_color(colors::text_primary())
                        .child(format!("#{r8:02X}{g8:02X}{b8:02X}")),
                ),
        )
        .child(div().flex().flex_col().gap_1().children(channels))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Presets"),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .children(preset_cells),
        )
}

/// One R/G/B stepper row: `−  LABEL  value  ＋`, clamped to [0,1]. The new colour
/// is built from the current channels (alpha preserved) and emitted per `target`.
fn channel_row(
    section: &'static str,
    target: Target,
    label: &'static str,
    idx: usize,
    c: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    let val = to_u8(c[idx]);

    let mk = |suffix: &'static str, glyph: &'static str, delta: f32| {
        // Distinct element id per (section, channel, button) so gpui's stateful
        // interactivity never collides between the Fill and Stroke sections.
        let id = ((section.as_bytes()[0] as u64) << 8) | (idx as u64) << 1;
        div()
            .id((suffix, id))
            .size(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .text_color(colors::text_primary())
            .cursor_pointer()
            .hover(|s| s.bg(colors::surface_overlay()))
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                let cur = match target {
                    Target::Fill => root.app.selected_fill(),
                    Target::Stroke => root.app.selected_stroke(),
                };
                if let Some(mut next) = cur {
                    next[idx] = (next[idx] + delta).clamp(0.0, 1.0);
                    root.app.apply(match target {
                        Target::Fill => Action::SetFillColor(next),
                        Target::Stroke => Action::SetStrokeColor(next),
                    });
                    cx.notify();
                }
            }))
            .child(glyph)
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(div().w(px(14.0)).text_color(colors::text_secondary()).child(label))
        .child(mk("ch-dec", "−", -COLOR_STEP))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_center()
                .text_color(colors::text_primary())
                .child(format!("{val}")),
        )
        .child(mk("ch-inc", "＋", COLOR_STEP))
}

/// A clickable preset swatch that sets the target colour (alpha preserved by the
/// preset itself). Distinct id-prefix per section keeps Fill / Stroke swatches
/// from colliding.
fn swatch_cell(
    id: (&'static str, u64),
    target: Target,
    color: [f32; 4],
    cx: &mut Context<Contour>,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(20.0))
        .rounded_md()
        .border_1()
        .border_color(rgb(SWATCH_BORDER))
        .bg(rgba(pack(color)))
        .cursor_pointer()
        .hover(|s| s.border_color(colors::text_primary()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            // Keep the shape's existing alpha so a preset only changes hue.
            let cur = match target {
                Target::Fill => root.app.selected_fill(),
                Target::Stroke => root.app.selected_stroke(),
            };
            let mut next = color;
            if let Some(prev) = cur {
                next[3] = prev[3];
            }
            root.app.apply(match target {
                Target::Fill => Action::SetFillColor(next),
                Target::Stroke => Action::SetStrokeColor(next),
            });
            cx.notify();
        }))
}
