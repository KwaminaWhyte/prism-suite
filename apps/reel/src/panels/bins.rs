//! Media Bins panel — organizes imported clips into named bins.
//!
//! Shown when `app.bins_open` is true. Displays bins as tabs/rows; clicking a
//! bin selects it, showing its clips below. Clips can be inserted to the
//! timeline via `InsertClipFromBin`. The Import button on each bin calls
//! `ImportToBin` for the selected bin.

use gpui::{div, px, svg, AnyElement, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled};
use prism_ui::{colors, section_header, Icon};

use crate::app_state::{Action, App};
use crate::panels::TextFields;
use crate::Reel;

pub fn render(app: &App, fields: &TextFields, cx: &mut Context<Reel>) -> impl IntoElement {
    let selected_bin = app.selected_bin;
    let selected_clip = app.selected_bin_clip;
    // Case-insensitive name filter from the search box.
    let query = app.bin_query.trim().to_lowercase();

    // Bin tab row.
    let bin_tabs = app
        .bins
        .iter()
        .enumerate()
        .map(|(bi, bin)| {
            let is_selected = bi == selected_bin;
            let name = bin.name.clone();
            div()
                .id(("bin-tab", bi))
                .cursor_pointer()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(if is_selected {
                    colors::tool_active()
                } else {
                    colors::surface_overlay()
                })
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SelectBin(bi));
                    cx.notify();
                }))
                .child(name)
        })
        .collect::<Vec<_>>();

    // Clip list for the active bin.
    let clip_rows = if let Some(bin) = app.bins.get(selected_bin) {
        bin.clips
            .iter()
            .enumerate()
            // Keep the true clip index `ci` so actions stay correct after filtering.
            .filter(|(_, clip)| query.is_empty() || clip.name.to_lowercase().contains(&query))
            .map(|(ci, clip)| {
                let is_sel = selected_clip == Some(ci);
                let name = clip.name.clone();
                let dur = clip.duration;
                div()
                    .id(("bin-clip", ci))
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .bg(if is_sel {
                        colors::surface_border()
                    } else {
                        colors::surface_overlay()
                    })
                    .rounded_sm()
                    .text_color(colors::text_primary())
                    .text_size(px(10.0))
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::SelectBinClip {
                            bin_idx: selected_bin,
                            clip_idx: ci,
                        });
                        cx.notify();
                    }))
                    .child(div().flex_1().child(name))
                    .child(
                        div()
                            .text_color(colors::text_secondary())
                            .child(format!("{:.1}s", dur)),
                    )
                    // Insert button.
                    .child(
                        div()
                            .id(("bin-insert", ci))
                            .w(px(20.0))
                            .h(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .rounded_sm()
                            .bg(colors::accent())
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::InsertClipFromBin {
                                    bin_clip_idx: ci,
                                    track_idx: 0,
                                    at_t: root.app.time,
                                });
                                cx.notify();
                            }))
                            .child(
                                svg()
                                    .path(Icon::Add.path())
                                    .w(px(10.0))
                                    .h(px(10.0))
                                    .text_color(colors::text_primary()),
                            ),
                    )
            })
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    // Import button for selected bin.
    let import_btn = div()
        .id("bin-import")
        .w(px(20.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded_sm()
        .bg(colors::accent())
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            let image_exts = prism_io::SUPPORTED_EXTENSIONS;
            use crate::app_state::{AUDIO_EXTENSIONS, VIDEO_EXTENSIONS};
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Video", VIDEO_EXTENSIONS)
                .add_filter("Audio", AUDIO_EXTENSIONS)
                .add_filter("Images", image_exts)
                .set_title("Import to Bin")
                .pick_file()
            {
                root.app.apply(Action::ImportToBin {
                    bin_idx: selected_bin,
                    path,
                });
                cx.notify();
            }
        }))
        .child(
            svg()
                .path(Icon::Import.path())
                .w(px(10.0))
                .h(px(10.0))
                .text_color(colors::text_primary()),
        );

    // Add bin button.
    let add_bin_btn = div()
        .id("bin-add")
        .w(px(20.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .rounded_sm()
        .bg(colors::surface_overlay())
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            let n = root.app.bins.len() + 1;
            root.app.apply(Action::AddBin(format!("Bin {}", n)));
            cx.notify();
        }))
        .child(
            svg()
                .path(Icon::Add.path())
                .w(px(10.0))
                .h(px(10.0))
                .text_color(colors::text_primary()),
        );

    // Search box (filters the active bin's clips by name) — SetBinQuery.
    let search_box: AnyElement = match fields.get("bin-search").cloned() {
        Some(f) => f.into_any_element(),
        None => div()
            .px_2().py(px(2.0)).rounded_sm()
            .bg(colors::surface_overlay())
            .text_color(colors::text_disabled()).text_size(px(10.0))
            .child("Search clips…")
            .into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(section_header("Media Bins"))
        .child(
            div()
                .flex()
                .flex_col()
                .p_2()
                .gap_1()
                // Bin tabs row.
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .children(bin_tabs)
                        .child(add_bin_btn)
                        .child(import_btn),
                )
                // Search box row.
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .child(search_box),
                )
                // Clip list.
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .children(if clip_rows.is_empty() {
                            vec![div()
                                .text_color(colors::text_disabled())
                                .text_size(px(10.0))
                                .child("Empty bin — import media to add clips.")
                                .into_any_element()]
                        } else {
                            clip_rows.into_iter().map(|r: gpui::Stateful<gpui::Div>| r.into_any_element()).collect::<Vec<AnyElement>>()
                        }),
                ),
        )
}
