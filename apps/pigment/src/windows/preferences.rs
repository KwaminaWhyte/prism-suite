//! Preferences window — floating OS-level window (Photoshop *Edit >
//! Preferences* parity). Reads `app.preferences` from the main `Pigment` entity
//! and emits `SetPref*` / `SavePreferences` / `LoadPreferences` /
//! `ResetPreferences` back to it.
//!
//! The view holds only a `WeakEntity<Pigment>`; it reads a *snapshot* clone of
//! the live `PigmentPreferences` at the top of `render` (via `entity.read(cx)`)
//! and never mutates `App` outside an `entity.update(..)` closure.

use gpui::{
    div, px, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement,
    Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, PigmentPreferences};
use crate::windows::{labeled_row, section_label, window_header};
use crate::Pigment;

pub struct PreferencesView {
    pub focus: FocusHandle,
    app_entity: WeakEntity<Pigment>,
}

impl PreferencesView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Pigment>) -> Self {
        Self { focus, app_entity }
    }
}

impl Focusable for PreferencesView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

/// Small pill button that dispatches an [`Action`] to the main entity.
fn action_btn(
    cx: &mut Context<PreferencesView>,
    entity: WeakEntity<Pigment>,
    id: &'static str,
    label: &'static str,
    accent: bool,
    make_action: impl Fn() -> Action + 'static,
) -> impl IntoElement {
    let (bg, hov) = if accent {
        (colors::accent(), colors::accent_hover())
    } else {
        (colors::surface_overlay(), colors::surface_border())
    };
    div()
        .id(id)
        .px_3()
        .py(px(5.0))
        .rounded_md()
        .bg(bg)
        .text_size(px(font_size::SM))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(move |s| s.bg(hov))
        .on_click(cx.listener(move |_this, _ev, _win, cx| {
            if let Some(e) = entity.upgrade() {
                let act = make_action();
                e.update(cx, |root, cx| {
                    root.app.apply(act);
                    cx.notify();
                });
            }
            cx.notify();
        }))
        .child(label)
}

/// A choice chip: highlighted when `selected`; click emits an action.
fn choice_chip(
    cx: &mut Context<PreferencesView>,
    entity: WeakEntity<Pigment>,
    id: (&'static str, usize),
    label: String,
    selected: bool,
    make_action: impl Fn() -> Action + 'static,
) -> impl IntoElement {
    let mut chip = div()
        .id(id)
        .px_2()
        .py(px(3.0))
        .rounded_md()
        .text_size(px(font_size::XS))
        .cursor_pointer()
        .child(label);
    if selected {
        chip = chip.bg(colors::accent()).text_color(colors::text_primary());
    } else {
        chip = chip
            .bg(colors::surface_overlay())
            .text_color(colors::text_secondary());
    }
    chip.on_click(cx.listener(move |_this, _ev, _win, cx| {
        if let Some(e) = entity.upgrade() {
            let act = make_action();
            e.update(cx, |root, cx| {
                root.app.apply(act);
                cx.notify();
            });
        }
        cx.notify();
    }))
}

/// A stepper row: − value + that emits actions built from the next/prev value.
fn stepper_row(
    cx: &mut Context<PreferencesView>,
    entity: WeakEntity<Pigment>,
    id: &'static str,
    caption: &'static str,
    value_text: String,
    dec: impl Fn() -> Action + 'static,
    inc: impl Fn() -> Action + 'static,
) -> impl IntoElement {
    let e_dec = entity.clone();
    let e_inc = entity;
    let dec_btn = div()
        .id((id, 0u64))
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |_this, _ev, _win, cx| {
            if let Some(en) = e_dec.upgrade() {
                let act = dec();
                en.update(cx, |root, cx| {
                    root.app.apply(act);
                    cx.notify();
                });
            }
            cx.notify();
        }))
        .child("−");
    let inc_btn = div()
        .id((id, 1u64))
        .w(px(22.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |_this, _ev, _win, cx| {
            if let Some(en) = e_inc.upgrade() {
                let act = inc();
                en.update(cx, |root, cx| {
                    root.app.apply(act);
                    cx.notify();
                });
            }
            cx.notify();
        }))
        .child("+");
    labeled_row(
        caption,
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(dec_btn)
            .child(
                div()
                    .min_w(px(70.0))
                    .flex()
                    .justify_center()
                    .text_size(px(font_size::SM))
                    .text_color(colors::text_primary())
                    .child(value_text),
            )
            .child(inc_btn),
    )
}

impl Render for PreferencesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Snapshot the live preferences from the main entity (read-only).
        let prefs: PigmentPreferences = match self.app_entity.upgrade() {
            Some(e) => e.read(cx).app.preferences.clone(),
            None => PigmentPreferences::default(),
        };

        let entity = self.app_entity.clone();

        // ── Performance pane ─────────────────────────────────────────────
        let mem = prefs.performance.memory_usage_fraction;
        let hist = prefs.performance.history_states;
        let use_gpu = prefs.performance.use_gpu;
        let perf = div()
            .flex()
            .flex_col()
            .child(section_label("PERFORMANCE"))
            .child(stepper_row(
                cx,
                entity.clone(),
                "pref-mem",
                "Memory Usage",
                format!("{:.0}%", mem * 100.0),
                move || Action::SetPrefMemoryFraction(mem - 0.05),
                move || Action::SetPrefMemoryFraction(mem + 0.05),
            ))
            .child(stepper_row(
                cx,
                entity.clone(),
                "pref-hist",
                "History States",
                format!("{hist}"),
                move || Action::SetPrefHistoryStates(hist.saturating_sub(10)),
                move || Action::SetPrefHistoryStates(hist + 10),
            ))
            .child(labeled_row(
                "Use GPU Compositor",
                {
                    let e = entity.clone();
                    div()
                        .id("pref-gpu")
                        .px_2()
                        .py(px(3.0))
                        .rounded_md()
                        .bg(if use_gpu {
                            colors::accent()
                        } else {
                            colors::surface_overlay()
                        })
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .on_click(cx.listener(move |_this, _ev, _win, cx| {
                            if let Some(en) = e.upgrade() {
                                en.update(cx, |root, cx| {
                                    root.app.apply(Action::SetPrefUseGpu(!use_gpu));
                                    cx.notify();
                                });
                            }
                            cx.notify();
                        }))
                        .child(if use_gpu { "On" } else { "Off" })
                },
            ));

        // ── Color pane ───────────────────────────────────────────────────
        let working_rgb = prefs.color.working_rgb.clone();
        let intent = prefs.color.rendering_intent.clone();
        const RGB_SPACES: [&str; 4] = [
            "sRGB IEC61966-2.1",
            "Adobe RGB (1998)",
            "Display P3",
            "ProPhoto RGB",
        ];
        const INTENTS: [&str; 4] = [
            "Perceptual",
            "Relative Colorimetric",
            "Saturation",
            "Absolute Colorimetric",
        ];
        let color = div()
            .flex()
            .flex_col()
            .child(section_label("COLOR"))
            .child(labeled_row(
                "Working RGB",
                div().flex().flex_row().flex_wrap().gap_1().children(
                    RGB_SPACES.iter().enumerate().map(|(i, name)| {
                        let n = name.to_string();
                        let sel = working_rgb == *name;
                        choice_chip(
                            cx,
                            entity.clone(),
                            ("pref-rgb", i),
                            n.clone(),
                            sel,
                            move || Action::SetPrefWorkingRgb(n.clone()),
                        )
                    }),
                ),
            ))
            .child(labeled_row(
                "Rendering Intent",
                div().flex().flex_row().flex_wrap().gap_1().children(
                    INTENTS.iter().enumerate().map(|(i, name)| {
                        let n = name.to_string();
                        let sel = intent == *name;
                        choice_chip(
                            cx,
                            entity.clone(),
                            ("pref-intent", i),
                            n.clone(),
                            sel,
                            move || Action::SetPrefRenderingIntent(n.clone()),
                        )
                    }),
                ),
            ));

        // ── Interface pane ───────────────────────────────────────────────
        let theme = prefs.interface.theme.clone();
        let ui_scale = prefs.interface.ui_scale;
        const THEMES: [&str; 4] = ["Dark", "Medium Dark", "Medium Light", "Light"];
        let interface = div()
            .flex()
            .flex_col()
            .child(section_label("INTERFACE"))
            .child(labeled_row(
                "Theme",
                div().flex().flex_row().flex_wrap().gap_1().children(
                    THEMES.iter().enumerate().map(|(i, name)| {
                        let n = name.to_string();
                        let sel = theme == *name;
                        choice_chip(
                            cx,
                            entity.clone(),
                            ("pref-theme", i),
                            n.clone(),
                            sel,
                            move || Action::SetPrefTheme(n.clone()),
                        )
                    }),
                ),
            ))
            .child(stepper_row(
                cx,
                entity.clone(),
                "pref-scale",
                "UI Scale",
                format!("{ui_scale:.2}×"),
                move || Action::SetPrefUiScale(ui_scale - 0.25),
                move || Action::SetPrefUiScale(ui_scale + 0.25),
            ));

        // ── File Handling pane ───────────────────────────────────────────
        let autosave = prefs.file_handling.autosave_minutes;
        let recent = prefs.file_handling.recent_file_count;
        let files = div()
            .flex()
            .flex_col()
            .child(section_label("FILE HANDLING"))
            .child(stepper_row(
                cx,
                entity.clone(),
                "pref-autosave",
                "Autosave (min)",
                format!("{autosave}"),
                move || Action::SetPrefAutosaveMinutes(autosave.saturating_sub(1)),
                move || Action::SetPrefAutosaveMinutes(autosave + 1),
            ))
            .child(stepper_row(
                cx,
                entity.clone(),
                "pref-recent",
                "Recent Files",
                format!("{recent}"),
                move || Action::SetPrefRecentFileCount(recent.saturating_sub(5)),
                move || Action::SetPrefRecentFileCount(recent + 5),
            ));

        // ── Footer: Load / Save / Reset ──────────────────────────────────
        let footer = div()
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .justify_end()
            .gap_2()
            .pt_3()
            .mt_2()
            .border_t_1()
            .border_color(colors::surface_border())
            .child(action_btn(
                cx,
                entity.clone(),
                "pref-reset",
                "Reset",
                false,
                || Action::ResetPreferences,
            ))
            .child(action_btn(
                cx,
                entity.clone(),
                "pref-load",
                "Load",
                false,
                || Action::LoadPreferences,
            ))
            .child(action_btn(
                cx,
                entity.clone(),
                "pref-save",
                "Save",
                true,
                || Action::SavePreferences,
            ));

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(window_header("Preferences", |_ev, win, _cx| {
                win.remove_window();
            }))
            .child(
                div()
                    .id("prefs-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_4()
                    .py_3()
                    .flex()
                    .flex_col()
                    .child(perf)
                    .child(color)
                    .child(interface)
                    .child(files)
                    .child(footer),
            )
    }
}
