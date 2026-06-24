//! Preferences panel — a floating panel binding `app.preferences` (After
//! Effects' Preferences dialog) across four panes:
//!
//! - **General**: undo levels, autosave minutes, tooltips (`SetPrefUndoLevels` /
//!   `SetPrefAutosaveMinutes` / `SetPrefShowTooltips`).
//! - **Display**: UI scale, dark theme, motion-path keyframes (`SetPrefUiScale`
//!   / `SetPrefDarkTheme` / `SetPrefMotionPathKeyframes`).
//! - **Media & Disk Cache**: cache dir + size + RAM reserve + conform fps
//!   (`SetPrefDiskCacheMaxGb` / `SetPrefRamReserve` / `SetPrefConformFps`), plus
//!   the live disk-cache via `SetCacheDir` / `SetCacheMaxGb` / `PurgeDiskCache`.
//! - **Previews**: preview quality + fast-draft (`SetPrefPreviewQuality` /
//!   `SetPrefFastDraft`).
//!
//! Reads `&App`, emits [`Action`](crate::app_state::Action)s. Gated by
//! `app.preferences_open` (toggled via the existing `TogglePreferences` action).

use std::path::PathBuf;

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, section_header};

use crate::app_state::{Action, App, PreviewQuality};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let p = &app.preferences;

    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child(section_header("Preferences")))
        .child(
            div()
                .id("pref-reset")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_size(px(10.0))
                .text_color(colors::text_secondary())
                .cursor_pointer()
                .child("Reset")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ResetPreferences);
                    cx.notify();
                })),
        )
        .child(
            div()
                .id("pref-close")
                .ml_2()
                .w(px(18.0))
                .h(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(rgb(BG_ACTIVE))
                .text_color(colors::text_primary())
                .text_size(px(12.0))
                .cursor_pointer()
                .child("×")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::TogglePreferences);
                    cx.notify();
                })),
        );

    // --- General pane ---
    let undo = p.general.undo_levels;
    let autosave = p.general.autosave_minutes;
    let tooltips = p.general.show_tooltips;
    let general = pane(
        "General",
        vec![
            stepper_row(cx, "Undo levels", undo.to_string(), 1,
                Action::SetPrefUndoLevels(undo.saturating_sub(1)),
                Action::SetPrefUndoLevels(undo + 1)),
            stepper_row(cx, "Autosave (min)", autosave.to_string(), 2,
                Action::SetPrefAutosaveMinutes(autosave.saturating_sub(1)),
                Action::SetPrefAutosaveMinutes(autosave + 1)),
            toggle_row(cx, "Tooltips", tooltips, 3,
                Action::SetPrefShowTooltips(!tooltips)),
        ],
    );

    // --- Display pane ---
    let ui_scale = p.display.ui_scale;
    let dark = p.display.dark_theme;
    let mp_keys = p.display.motion_path_keyframes;
    let display = pane(
        "Display",
        vec![
            stepper_row(cx, "UI scale", format!("{:.0}%", ui_scale * 100.0), 10,
                Action::SetPrefUiScale((ui_scale - 0.1).max(0.5)),
                Action::SetPrefUiScale((ui_scale + 0.1).min(3.0))),
            toggle_row(cx, "Dark theme", dark, 11,
                Action::SetPrefDarkTheme(!dark)),
            stepper_row(cx, "Path keyframes", mp_keys.to_string(), 12,
                Action::SetPrefMotionPathKeyframes(mp_keys.saturating_sub(1)),
                Action::SetPrefMotionPathKeyframes(mp_keys + 1)),
        ],
    );

    // --- Media & Disk Cache pane ---
    let max_gb = p.media.disk_cache_max_gb;
    let ram = p.media.ram_reserve_fraction;
    let conform = p.media.conform_fps;
    let cache_dir = p.media.disk_cache_dir.clone();
    let usage_frac = app.disk_cache.usage_fraction();
    let media = pane(
        "Media & Disk Cache",
        vec![
            cache_dir_row(cx, cache_dir),
            stepper_row(cx, "Cache max (GB)", format!("{max_gb:.0}"), 20,
                Action::SetPrefDiskCacheMaxGb((max_gb - 5.0).max(1.0)),
                Action::SetPrefDiskCacheMaxGb(max_gb + 5.0)),
            stepper_row(cx, "RAM reserve", format!("{:.0}%", ram * 100.0), 21,
                Action::SetPrefRamReserve((ram - 0.05).max(0.0)),
                Action::SetPrefRamReserve((ram + 0.05).min(0.9))),
            stepper_row(cx, "Conform FPS", format!("{conform:.0}"), 22,
                Action::SetPrefConformFps((conform - 1.0).max(1.0)),
                Action::SetPrefConformFps(conform + 1.0)),
            cache_usage_row(usage_frac),
            purge_cache_row(cx),
        ],
    );

    // --- Previews pane ---
    let quality = p.previews.quality;
    let fast = p.previews.fast_draft;
    let quality_chips: Vec<gpui::AnyElement> = PreviewQuality::ALL
        .iter()
        .copied()
        .map(|q| {
            let active = quality == q;
            div()
                .id(("pref-quality", q as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if active { rgb(BG_ACTIVE) } else { colors::surface_overlay() })
                .text_size(px(9.0))
                .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
                .cursor_pointer()
                .child(q.label())
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetPrefPreviewQuality(q));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();
    let previews = pane(
        "Previews",
        vec![
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Quality"))
                .child(div().flex().flex_wrap().gap_1().children(quality_chips))
                .into_any_element(),
            toggle_row(cx, "Fast draft", fast, 30, Action::SetPrefFastDraft(!fast)),
        ],
    );

    // Save status readout.
    let status: Vec<gpui::AnyElement> = app
        .last_prefs_save_result
        .as_ref()
        .map(|s| {
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child(s.clone())
                .into_any_element()
        })
        .into_iter()
        .collect();

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(general)
        .child(display)
        .child(media)
        .child(previews)
        .children(status)
}

/// A titled pane wrapping a set of rows.
fn pane(title: &str, rows: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .child(title.to_string()),
        )
        .children(rows)
        .into_any_element()
}

/// A labeled value with −/+ steppers.
fn stepper_row(
    cx: &mut Context<Pulse>,
    label: &'static str,
    value: String,
    salt: usize,
    dec: Action,
    inc: Action,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(label))
        .child(step_btn(cx, ("pref-dec", salt), "−", dec))
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(10.0)).child(value))
        .child(step_btn(cx, ("pref-inc", salt), "+", inc))
        .into_any_element()
}

/// An on/off toggle row.
fn toggle_row(
    cx: &mut Context<Pulse>,
    label: &'static str,
    on: bool,
    salt: usize,
    action: Action,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(label))
        .child(
            div()
                .id(("pref-toggle", salt))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if on { rgb(BG_ACTIVE) } else { colors::surface_overlay() })
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child(if on { "On" } else { "Off" })
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(action.clone());
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// Disk-cache directory row: shows the path, a "Browse…" button that pops a
/// native folder picker and emits both `SetCacheDir` (live cache) and
/// `SetPrefDiskCacheDir` (persisted preference).
fn cache_dir_row(cx: &mut Context<Pulse>, dir: String) -> gpui::AnyElement {
    let shown = if dir.len() > 28 {
        format!("…{}", &dir[dir.len() - 27..])
    } else {
        dir.clone()
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Cache dir"))
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(9.0)).child(shown))
        .child(
            div()
                .id("pref-cache-browse")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child("Browse…")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title("Select disk-cache folder…")
                        .pick_folder()
                    {
                        root.app.apply(Action::SetCacheDir(PathBuf::from(&path)));
                        root.app.apply(Action::SetPrefDiskCacheDir(
                            path.to_string_lossy().to_string(),
                        ));
                        cx.notify();
                    }
                })),
        )
        .into_any_element()
}

/// A read-only disk-cache usage bar.
fn cache_usage_row(frac: f32) -> gpui::AnyElement {
    let pct = (frac * 100.0).clamp(0.0, 100.0);
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Cache usage"))
        .child(
            div()
                .flex_1()
                .h(px(6.0))
                .bg(colors::surface_overlay())
                .rounded_md()
                .child(
                    div()
                        .h(px(6.0))
                        .w(px(120.0 * (frac.clamp(0.0, 1.0))))
                        .bg(rgb(0x37c8c0))
                        .rounded_md(),
                ),
        )
        .child(div().text_color(colors::text_secondary()).text_size(px(9.0)).child(format!("{pct:.0}%")))
        .into_any_element()
}

/// Purge-cache button row (`PurgeDiskCache`).
fn purge_cache_row(cx: &mut Context<Pulse>) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(110.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Disk cache"))
        .child(
            div()
                .id("pref-cache-purge")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(rgb(0xe06c75u32))
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child("Empty Disk Cache")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::PurgeDiskCache);
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// A single stepper button emitting `action`.
fn step_btn(
    cx: &mut Context<Pulse>,
    id: (&'static str, usize),
    glyph: &'static str,
    action: Action,
) -> impl IntoElement {
    div()
        .id(id)
        .w(px(20.0))
        .h(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(rgb(BG_ACTIVE))
        .text_color(colors::text_primary())
        .text_size(px(12.0))
        .cursor_pointer()
        .child(glyph)
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(action.clone());
            cx.notify();
        }))
}
