//! Document Setup — a floating OS-level child window.
//!
//! Holds a [`WeakEntity<Contour>`] (the welcome / Reel preferences pattern) so
//! its controls dispatch [`Action`]s into the main view and read its state back.
//! It edits `app.doc_setup` via the Batch-11/12 actions:
//!
//! - width / height: typed via [`prism_ui::TextField`] **or** ± steppers
//!   → [`Action::SetDocSetupSize`]
//! - unit chips               → [`Action::SetDocSetupUnit`]
//! - colour-mode chips        → [`Action::SetDocSetupColorMode`]
//! - per-side bleed: typed via [`prism_ui::TextField`] **or** ± steppers
//!   → [`Action::SetDocSetupBleed`]
//! - "Create"                 → [`Action::NewDocumentFromSetup`] (then closes)
//!
//! The six numeric fields are real [`TextField`] entities held on the view; a
//! typed value is parsed in the current display unit and converted to document
//! points via [`DocumentSetup::to_points`] before dispatch. The ± steppers
//! remain for fine nudging.

use gpui::{
    AppContext, ClickEvent, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
    div, px,
};
use prism_ui::{colors, font_size, TextField};

use crate::app_state::{Action, DocColorMode, DocUnit, DocumentSetup};
use crate::Contour;

pub struct DocumentSetupView {
    focus: FocusHandle,
    app_entity: WeakEntity<Contour>,
    /// Real typing fields, in display units: width / height / bleed T/R/B/L.
    width_field: Entity<TextField>,
    height_field: Entity<TextField>,
    bleed_fields: [Entity<TextField>; 4],
}

impl Focusable for DocumentSetupView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DocumentSetupView {
    pub fn new(
        focus: FocusHandle,
        app_entity: WeakEntity<Contour>,
        cx: &mut Context<Self>,
    ) -> Self {
        let setup = app_entity
            .upgrade()
            .map(|e| e.read(cx).app.doc_setup.clone())
            .unwrap_or_default();

        let mk = |value: f32, weak: WeakEntity<Contour>, build: BuildSize, cx: &mut Context<Self>| {
            cx.new(|cx| {
                TextField::new(cx)
                    .width(px(96.0))
                    .initial_value(format!("{value:.2}"))
                    .on_submit(move |text, _win, cx| {
                        if let Ok(v) = text.trim().parse::<f32>() {
                            // Re-read the *current* unit so conversion is correct
                            // even after the unit chip changed.
                            let _ = weak.update(cx, |c, cx| {
                                let pts = c.app.doc_setup.to_points(v);
                                c.app.apply(build(&c.app.doc_setup, pts));
                                cx.notify();
                            });
                        }
                    })
            })
        };

        let bw: BuildSize =
            |s, pts| Action::SetDocSetupSize { width: pts.max(1.0), height: s.height };
        let bh: BuildSize =
            |s, pts| Action::SetDocSetupSize { width: s.width, height: pts.max(1.0) };
        let bt: BuildSize = |s, pts| Action::SetDocSetupBleed {
            top: pts.max(0.0), right: s.bleed[1], bottom: s.bleed[2], left: s.bleed[3],
        };
        let br: BuildSize = |s, pts| Action::SetDocSetupBleed {
            top: s.bleed[0], right: pts.max(0.0), bottom: s.bleed[2], left: s.bleed[3],
        };
        let bb: BuildSize = |s, pts| Action::SetDocSetupBleed {
            top: s.bleed[0], right: s.bleed[1], bottom: pts.max(0.0), left: s.bleed[3],
        };
        let bl: BuildSize = |s, pts| Action::SetDocSetupBleed {
            top: s.bleed[0], right: s.bleed[1], bottom: s.bleed[2], left: pts.max(0.0),
        };

        let width_field = mk(setup.from_points(setup.width), app_entity.clone(), bw, cx);
        let height_field = mk(setup.from_points(setup.height), app_entity.clone(), bh, cx);
        let bleed_fields = [
            mk(setup.from_points(setup.bleed[0]), app_entity.clone(), bt, cx),
            mk(setup.from_points(setup.bleed[1]), app_entity.clone(), br, cx),
            mk(setup.from_points(setup.bleed[2]), app_entity.clone(), bb, cx),
            mk(setup.from_points(setup.bleed[3]), app_entity.clone(), bl, cx),
        ];

        Self { focus, app_entity, width_field, height_field, bleed_fields }
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

/// `(setup, value_in_points) -> Action` builder for a typed size/bleed field.
type BuildSize = fn(&DocumentSetup, f32) -> Action;

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

        let cur_w = setup.width;
        let cur_h = setup.height;

        // --- Size rows (TextField + ± stepper) ---
        let width_row = self.dim_row(
            "ds-w", "Width", self.width_field.clone(), setup.unit, cx,
            move |delta_pts| Action::SetDocSetupSize {
                width: (cur_w + delta_pts).max(1.0),
                height: cur_h,
            },
        );
        let height_row = self.dim_row(
            "ds-h", "Height", self.height_field.clone(), setup.unit, cx,
            move |delta_pts| Action::SetDocSetupSize {
                width: cur_w,
                height: (cur_h + delta_pts).max(1.0),
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

        // --- Bleed rows (top / right / bottom / left), each typed + stepped ---
        let [bt, br, bb, bl] = setup.bleed;
        let bleed_t = self.dim_row(
            "ds-bt", "Top", self.bleed_fields[0].clone(), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: (bt + d).max(0.0), right: br, bottom: bb, left: bl },
        );
        let bleed_r = self.dim_row(
            "ds-br", "Right", self.bleed_fields[1].clone(), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: bt, right: (br + d).max(0.0), bottom: bb, left: bl },
        );
        let bleed_b = self.dim_row(
            "ds-bb", "Bottom", self.bleed_fields[2].clone(), setup.unit, cx,
            move |d| Action::SetDocSetupBleed { top: bt, right: br, bottom: (bb + d).max(0.0), left: bl },
        );
        let bleed_l = self.dim_row(
            "ds-bl", "Left", self.bleed_fields[3].clone(), setup.unit, cx,
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
    /// A labelled row: `Label  [typed field] unit  −  +`. The field carries the
    /// typed value (parsed in `new`); the steppers nudge by ±1 unit via
    /// `make_action(delta_pts)`.
    #[allow(clippy::too_many_arguments)]
    fn dim_row(
        &self,
        id: &'static str,
        label: &'static str,
        field: Entity<TextField>,
        unit: DocUnit,
        cx: &mut Context<Self>,
        make_action: impl Fn(f32) -> Action + Clone + 'static,
    ) -> impl IntoElement {
        let step = unit.points_per_unit();
        let dec = make_action.clone();
        let inc = make_action;
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
                    .child(field)
                    .child(
                        div().text_color(colors::text_secondary()).text_size(px(font_size::XS))
                            .child(unit.label()),
                    )
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
                                this.dispatch(cx, dec(-step));
                                cx.notify();
                            }))
                            .child("\u{2212}"),
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
                                this.dispatch(cx, inc(step));
                                cx.notify();
                            }))
                            .child("+"),
                    ),
            )
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
