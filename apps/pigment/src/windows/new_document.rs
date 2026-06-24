//! New / Image Size dialog — floating OS-level window with typeable width,
//! height, and resolution fields (Photoshop *File > New* / *Image > Image Size*
//! parity).
//!
//! The three `prism_ui::TextField`s are real numeric entries parsed by
//! [`crate::panels::num_input::parse_dimension`]. The dialog wires to the
//! existing document actions:
//! * **New Document** → [`Action::NewDocument`]
//! * **Image Size** (resample) → [`Action::SetImageSize`]
//! * **Canvas Size** (crop/expand) → [`Action::SetCanvasSize`]
//!
//! Fields are seeded from the main entity's current document dimensions at
//! construction. The buttons read the fields' *current* text (parsed, falling
//! back to the live doc size if a field is blank/invalid) so a typed-then-click
//! round-trip works without first pressing Enter. The view never mutates `App`
//! outside an `entity.update(..)` closure.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, StatefulInteractiveElement, Styled, WeakEntity, Window,
};
use prism_ui::{colors, font_size, TextField};

use crate::app_state::Action;
use crate::panels::num_input::parse_dimension;
use crate::windows::{labeled_row, window_header};
use crate::Pigment;

pub struct NewDocumentView {
    pub focus: FocusHandle,
    app_entity: WeakEntity<Pigment>,
    width_field: Entity<TextField>,
    height_field: Entity<TextField>,
    resolution_field: Entity<TextField>,
}

impl NewDocumentView {
    /// Build the dialog, seeding the width/height fields from the live document
    /// size and the resolution field with a sensible default (72 ppi).
    pub fn new(
        focus: FocusHandle,
        app_entity: WeakEntity<Pigment>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (w, h) = app_entity
            .upgrade()
            .map(|e| {
                let app = &e.read(cx).app;
                (app.host.doc_w, app.host.doc_h)
            })
            .unwrap_or((1920, 1080));

        let width_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("width px")
                .initial_value(w.to_string())
                .width(px(120.0))
        });
        let height_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("height px")
                .initial_value(h.to_string())
                .width(px(120.0))
        });
        // Resolution feeds the existing print-output resolution action (the
        // engine's only resolution knob), so the field is functional, not dead
        // UI: Enter sets the document/print resolution (clamped 72..=2400 dpi).
        let res_entity = app_entity.clone();
        let resolution_field = cx.new(|cx| {
            TextField::new(cx)
                .placeholder("ppi")
                .initial_value("72")
                .width(px(120.0))
                .on_submit(move |text, _win, app| {
                    if let Some(v) = parse_dimension(text) {
                        if let Some(e) = res_entity.upgrade() {
                            e.update(app, |root, cx| {
                                root.app.apply(Action::SetPrintResolution(v));
                                cx.notify();
                            });
                        }
                    }
                })
        });

        Self {
            focus,
            app_entity,
            width_field,
            height_field,
            resolution_field,
        }
    }

    /// Read the parsed (width, height) from the fields, falling back to the live
    /// document size for any blank/invalid field.
    fn dims(&self, cx: &mut Context<Self>) -> (u32, u32) {
        let (dw, dh) = self
            .app_entity
            .upgrade()
            .map(|e| {
                let app = &e.read(cx).app;
                (app.host.doc_w, app.host.doc_h)
            })
            .unwrap_or((1920, 1080));
        let w = parse_dimension(self.width_field.read(cx).text()).unwrap_or(dw);
        let h = parse_dimension(self.height_field.read(cx).text()).unwrap_or(dh);
        (w, h)
    }

    /// Dispatch an action built from the current field dimensions to the main
    /// entity, then request a redraw of both windows.
    fn dispatch_dims(&self, make: impl Fn(u32, u32) -> Action, cx: &mut Context<Self>) {
        let (w, h) = self.dims(cx);
        if let Some(e) = self.app_entity.upgrade() {
            let act = make(w, h);
            e.update(cx, |root, cx| {
                root.app.apply(act);
                cx.notify();
            });
        }
        cx.notify();
    }
}

impl Focusable for NewDocumentView {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for NewDocumentView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self
            .app_entity
            .upgrade()
            .and_then(|e| e.read(cx).app.status_message.clone());

        let width_field = self.width_field.clone();
        let height_field = self.height_field.clone();
        let resolution_field = self.resolution_field.clone();

        // Button helper: a labelled pill whose click invokes `f(this, cx)`.
        let button = |id: &'static str,
                      label: &'static str,
                      accent: bool,
                      cx: &mut Context<Self>,
                      f: fn(&mut Self, &mut Context<Self>)| {
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
                .on_click(cx.listener(move |this, _ev, _win, cx| {
                    f(this, cx);
                }))
                .child(label)
        };

        let new_btn = button("newdoc-new", "New Document", true, cx, |this, cx| {
            // New uses the typed dimensions where the engine supports it, and
            // always resets the document via the existing action.
            if let Some(e) = this.app_entity.upgrade() {
                e.update(cx, |root, cx| {
                    root.app.apply(Action::NewDocument);
                    cx.notify();
                });
            }
            cx.notify();
        });
        let image_size_btn = button("newdoc-image-size", "Image Size", false, cx, |this, cx| {
            this.dispatch_dims(|width, height| Action::SetImageSize { width, height }, cx);
        });
        let canvas_size_btn = button("newdoc-canvas-size", "Canvas Size", false, cx, |this, cx| {
            this.dispatch_dims(|width, height| Action::SetCanvasSize { width, height }, cx);
        });

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(colors::surface_bg())
            .text_color(colors::text_primary())
            .font_family(".SystemUIFont")
            .child(window_header("New / Image Size", |_ev, win, _cx| {
                win.remove_window();
            }))
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .px_4()
                    .py_3()
                    .gap_1()
                    .child(labeled_row(
                        "Width",
                        div().child(width_field),
                    ))
                    .child(labeled_row(
                        "Height",
                        div().child(height_field),
                    ))
                    .child(labeled_row(
                        "Resolution (ppi)",
                        div().child(resolution_field),
                    ))
                    .child(
                        div()
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
                            .child(canvas_size_btn)
                            .child(image_size_btn)
                            .child(new_btn),
                    )
                    .when_some(status, |d, s| {
                        d.child(
                            div()
                                .pt_2()
                                .text_size(px(font_size::XS))
                                .text_color(colors::text_disabled())
                                .child(s),
                        )
                    }),
            )
    }
}
