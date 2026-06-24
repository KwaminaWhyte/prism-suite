//! Document Setup — a floating OS-level child window.
//!
//! Holds a [`WeakEntity<Contour>`] (the welcome / Reel preferences pattern) so
//! its controls dispatch [`Action`]s into the main view and read its state back.
//! It edits `app.doc_setup` via the Batch-11/12 actions:
//!
//! - width / height steppers  → [`Action::SetDocSetupSize`]
//! - unit chips               → [`Action::SetDocSetupUnit`]
//! - colour-mode chips        → [`Action::SetDocSetupColorMode`]
//! - per-side bleed steppers  → [`Action::SetDocSetupBleed`]
//! - "Create"                 → [`Action::NewDocumentFromSetup`] (then closes)
//!
//! Numeric fields use ± steppers rather than free text entry — the same input
//! idiom the rest of Contour's floating windows use (no text-input subsystem).
//! Dimensions / bleed are stored in document points; the panel formats them in
//! the selected display unit via [`DocumentSetup::from_points`] /
//! [`DocumentSetup::to_points`].

use gpui::{
    ClickEvent, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window, div, px,
};
use prism_ui::{colors, font_size};

use crate::app_state::{Action, DocColorMode, DocUnit, DocumentSetup};
use crate::Contour;

pub struct DocumentSetupView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
}

impl Focusable for DocumentSetupView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DocumentSetupView {
    pub fn new(focus: FocusHandle, app_entity: WeakEntity<Contour>) -> Self {
        Self { focus, app_entity }
    }

    /// Dispatch an action into the main Contour view and request a redraw there.
    fn dispatch(&self, cx: &mut Context<Self>, action: Action) {
        if let Some(entity) = self.app_entity.upgrade() {
            entity.update(cx, |c, cx| {
                c.app.apply(action);
                cx.notify();
            });
        }
    }

    /// Snapshot the document-setup model so the render borrow ends before the
    /// listeners re-borrow the entity.
    fn snapshot(&self, cx: &mut Context<Self>) -> DocumentSetup {
        self.app_entity
            .upgrade()
            .map(|e| e.read(cx).app.doc_setup.clone())
            .unwrap_or_default()
    }
}

/// Unit chips offered in the setup window.
const UNITS: [(DocUnit, &str); 6] = [
    (DocUnit::Points, "Points"),
    (DocUnit::Pixels, "Pixels"),
    (DocUnit::Picas, "Picas"),
    (DocUnit::Inches, "Inches"),
    (DocUnit::Millimeters, "mm"),
    (DocUnit::Centimeters, "cm"),
];

const COLOR_MODES: [(DocColorMode, &str); 2] =
    [(DocColorMode::Rgb, "RGB"), (DocColorMode::Cmyk, "CMYK")];

impl Render for DocumentSetupView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let setup = self.snapshot(cx);

        // Dimensions, displayed in the chosen unit. A stepper step is one unit
        // (so 1 pt / 1 px / 1 in / etc.), kept in points internally; the per-row
        // step in points is derived inside `dim_row` from the unit.
        let w_disp = setup.from_points(setup.width);
        let h_disp = setup.from_points(setup.height);

        let cur_w = setup.width;
        let cur_h = setup.height;

        // --- Size steppers ---
        let width_row = self.dim_row(
            "ds-w",
            "Width",
            w_disp,
            setup.unit,
            cx,
            {
                let cur_h = cur_h;
                move |delta_pts| Action::SetDocSetupSize {
                    width: (cur_w + delta_pts).max(1.0),
                    height: cur_h,
                }
            },
        );
        let height_row = self.dim_row(
            "ds-h",
            "Height",
            h_disp,
            setup.unit,
            cx,
            {
                let cur_w = cur_w;
                move |delta_pts| Action::SetDocSetupSize {
                    width: cur_w,
                    height: (cur_h + delta_pts).max(1.0),
                }
            },
        );

        // --- Unit chips ---
        let unit_chips: Vec<gpui::AnyElement> = UNITS
            .iter()
            .enumerate()
            .map(|(i, (unit, label))| {
                let unit = *unit;
                let active = setup.unit == unit;
                chip(("ds-unit", i), label, active, cx, move |this, cx| {
                    this.dispatch(cx, Action::SetDocSetupUnit(unit));
                })
                .into_any_element()
            })
            .collect();

        // --- Colour-mode chips ---
        let mode_chips: Vec<gpui::AnyElement> = COLOR_MODES
            .iter()
            .enumerate()
            .map(|(i, (mode, label))| {
                let mode = *mode;
                let active = setup.color_mode == mode;
                chip(("ds-mode", i), label, active, cx, move |this, cx| {
                    this.dispatch(cx, Action::SetDocSetupColorMode(mode));
                })
                .into_any_element()
            })
            .collect();

        // --- Bleed steppers (top / right / bottom / left), each in display unit ---
        let [bt, br, bb, bl] = setup.bleed;
        let bleed_t = self.bleed_row(
            "ds-bt", "Top", setup.from_points(bt), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: (bt + d).max(0.0), right: br, bottom: bb, left: bl },
        );
        let bleed_r = self.bleed_row(
            "ds-br", "Right", setup.from_points(br), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: bt, right: (br + d).max(0.0), bottom: bb, left: bl },
        );
        let bleed_b = self.bleed_row(
            "ds-bb", "Bottom", setup.from_points(bb), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: bt, right: br, bottom: (bb + d).max(0.0), left: bl },
        );
        let bleed_l = self.bleed_row(
            "ds-bl", "Left", setup.from_points(bl), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: bt, right: br, bottom: bb, left: (bl + d).max(0.0) },
        );

        // --- Create / Cancel buttons ---
        let create_btn = div()
            .id("ds-create")
            .px(px(16.0)).py(px(6.0))
            .rounded(px(4.0)).bg(colors::accent())
            .text_color(colors::text_primary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::accent_hover()))
            .on_click(cx.listener(move |this, _e: &ClickEvent, win, cx| {
                this.dispatch(cx, Action::NewDocumentFromSetup);
                win.remove_window();
            }))
            .child("Create");

        let cancel_btn = div()
            .id("ds-cancel")
            .px(px(14.0)).py(px(6.0))
            .rounded(px(4.0)).bg(colors::surface_raised())
            .border_1().border_color(colors::surface_border())
            .text_color(colors::text_secondary()).text_size(px(font_size::SM))
            .cursor_pointer()
            .hover(|s| s.bg(colors::tool_hover()))
            .on_click(cx.listener(move |_this, _e: &ClickEvent, win, _cx| {
                win.remove_window();
            }))
            .child("Cancel");

        div()
            .size_full()
            .flex().flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .p(px(20.0))
            .child(
                div()
                    .text_size(px(22.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .mb(px(12.0))
                    .child("Document Setup"),
            )
            // Size
            .child(section_label("SIZE"))
            .child(width_row)
            .child(height_row)
            // Unit
            .child(section_label("UNITS"))
            .child(div().flex().flex_row().flex_wrap().gap(px(6.0)).children(unit_chips))
            // Colour mode
            .child(section_label("COLOR MODE"))
            .child(div().flex().flex_row().gap(px(6.0)).children(mode_chips))
            // Bleed
            .child(section_label("BLEED"))
            .child(bleed_t)
            .child(bleed_r)
            .child(bleed_b)
            .child(bleed_l)
            .child(div().flex_1())
            .child(
                div()
                    .flex().flex_row().justify_end().gap(px(8.0))
                    .mt(px(12.0))
                    .child(cancel_btn)
                    .child(create_btn),
            )
    }
}

impl DocumentSetupView {
    /// A labelled dimension stepper row. `make_action(delta_pts)` builds the
    /// action for a ±1-unit change (delta already converted to points).
    #[allow(clippy::too_many_arguments)]
    fn dim_row(
        &self,
        id: &'static str,
        label: &'static str,
        value_disp: f32,
        unit: DocUnit,
        cx: &mut Context<Self>,
        make_action: impl Fn(f32) -> Action + Clone + 'static,
    ) -> impl IntoElement {
        let step = unit.points_per_unit();
        let dec = make_action.clone();
        let inc = make_action;
        stepper_row(id, label, value_disp, unit.label(), cx, step, dec, inc)
    }

    /// A bleed stepper row (same widget as `dim_row`, semantically distinct).
    #[allow(clippy::too_many_arguments)]
    fn bleed_row(
        &self,
        id: &'static str,
        label: &'static str,
        value_disp: f32,
        unit: DocUnit,
        cx: &mut Context<Self>,
        make_action: impl Fn(f32) -> Action + Clone + 'static,
    ) -> impl IntoElement {
        let step = unit.points_per_unit();
        let dec = make_action.clone();
        let inc = make_action;
        stepper_row(id, label, value_disp, unit.label(), cx, step, dec, inc)
    }
}

// --- shared widgets ------------------------------------------------------------

fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .mt(px(12.0))
        .mb(px(4.0))
        .text_size(px(font_size::SM))
        .text_color(colors::text_secondary())
        .child(text)
}

/// A labelled `−  NNN unit  +` stepper. The − button dispatches
/// `make_dec(-step)`, the + button `make_inc(+step)`.
#[allow(clippy::too_many_arguments)]
fn stepper_row(
    id: &'static str,
    label: &'static str,
    value_disp: f32,
    unit_label: &'static str,
    cx: &mut Context<DocumentSetupView>,
    step: f32,
    make_dec: impl Fn(f32) -> Action + 'static,
    make_inc: impl Fn(f32) -> Action + 'static,
) -> impl IntoElement {
    let dec_id = (id, 0usize);
    let inc_id = (id, 1usize);
    div()
        .flex().flex_row().items_center().justify_between()
        .py(px(4.0))
        .child(
            div().text_color(colors::text_secondary()).text_size(px(font_size::SM))
                .child(label),
        )
        .child(
            div()
                .flex().flex_row().items_center().gap(px(4.0))
                .child(
                    div()
                        .id(dec_id)
                        .w(px(22.0)).h(px(20.0))
                        .flex().items_center().justify_center()
                        .rounded(px(3.0)).bg(colors::surface_raised())
                        .text_color(colors::text_primary()).text_size(px(14.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                            this.dispatch(cx, make_dec(-step));
                            cx.notify();
                        }))
                        .child("\u{2212}"),
                )
                .child(
                    div()
                        .px(px(8.0)).py(px(2.0))
                        .min_w(px(96.0))
                        .flex().justify_center()
                        .rounded(px(3.0)).bg(colors::surface_overlay())
                        .text_color(colors::text_primary()).text_size(px(font_size::SM))
                        .child(format!("{value_disp:.2} {unit_label}")),
                )
                .child(
                    div()
                        .id(inc_id)
                        .w(px(22.0)).h(px(20.0))
                        .flex().items_center().justify_center()
                        .rounded(px(3.0)).bg(colors::surface_raised())
                        .text_color(colors::text_primary()).text_size(px(14.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
                            this.dispatch(cx, make_inc(step));
                            cx.notify();
                        }))
                        .child("+"),
                ),
        )
}

/// A selectable chip (unit / colour-mode), accent-filled when active.
fn chip(
    id: (&'static str, usize),
    label: &'static str,
    active: bool,
    cx: &mut Context<DocumentSetupView>,
    on_click: impl Fn(&mut DocumentSetupView, &mut Context<DocumentSetupView>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px(px(12.0)).py(px(5.0))
        .rounded(px(4.0))
        .bg(if active { colors::accent() } else { colors::surface_raised() })
        .border_1()
        .border_color(if active { colors::accent() } else { colors::surface_border() })
        .text_color(colors::text_primary()).text_size(px(font_size::SM))
        .cursor_pointer()
        .hover(|s| if active { s } else { s.bg(colors::tool_hover()) })
        .on_click(cx.listener(move |this, _e: &ClickEvent, _w, cx| {
            on_click(this, cx);
            cx.notify();
        }))
        .child(label)
}
