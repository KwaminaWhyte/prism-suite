//! Filter Gallery panel — a Photoshop-style categorized filter browser.
//!
//! Renders a scrollable list of filter categories, each of which expands to
//! show individual named effects. This is a UI scaffold only: clicking a named
//! effect closes the gallery (via `Action::CloseFilterGallery`) and applies the
//! corresponding `Action::ApplyFilter` when a concrete filter mapping exists.
//! Effects listed as "stub" are UI-only — they show the entry but post no
//! pixel operation until their backing pass is wired.
//!
//! The panel is surfaced as an overlay in `main.rs` when `app.filter_gallery_open`
//! is `true` (toggled by `Action::OpenFilterGallery` / `Action::CloseFilterGallery`).
//! `ToggleFilterGallery` flips the flag from either state.

use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, Context, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled,
};
use prism_ui::colors;

use crate::app_state::{Action, App, Filter};
use crate::Pigment;

/// A filter entry: display label + optional concrete action.
/// `None` = UI stub (shows the row, no pixel op yet).
#[derive(Clone)]
struct FilterEntry {
    label: &'static str,
    filter: Option<Filter>,
}

impl FilterEntry {
    fn live(label: &'static str, f: Filter) -> Self {
        Self { label, filter: Some(f) }
    }
    fn stub(label: &'static str) -> Self {
        Self { label, filter: None }
    }
}

/// Category definition: name + list of effects.
struct Category {
    name: &'static str,
    entries: Vec<FilterEntry>,
}

/// Build the static category list. Categories mirror Photoshop's Filter Gallery
/// sections. Effects with a live backing action are wired; the rest are stubs so
/// the panel is visually complete now and the backing passes slot in per wave.
fn categories() -> Vec<Category> {
    vec![
        Category {
            name: "Artistic",
            entries: vec![
                FilterEntry::stub("Colored Pencil"),
                FilterEntry::stub("Cutout"),
                FilterEntry::stub("Dry Brush"),
                FilterEntry::stub("Film Grain"),
                FilterEntry::stub("Fresco"),
                FilterEntry::stub("Neon Glow"),
                FilterEntry::stub("Paint Daubs"),
                FilterEntry::stub("Palette Knife"),
                FilterEntry::stub("Plastic Wrap"),
                FilterEntry::stub("Poster Edges"),
                FilterEntry::stub("Rough Pastels"),
                FilterEntry::stub("Smudge Stick"),
                FilterEntry::stub("Sponge"),
                FilterEntry::stub("Underpainting"),
                FilterEntry::stub("Watercolor"),
            ],
        },
        Category {
            name: "Blur",
            entries: vec![
                FilterEntry::live("Gaussian Blur", Filter::GaussianBlur { radius: 3.0 }),
                FilterEntry::live("Box Blur", Filter::BoxBlur { radius: 3.0 }),
                FilterEntry::live("Unsharp Mask", Filter::UnsharpMask { radius: 3.0, amount: 1.0, threshold: 0.05 }),
                FilterEntry::live("Radial Blur", Filter::RadialBlur { amount: 15.0 }),
                FilterEntry::live("Motion Blur", Filter::MotionBlur { angle: 0.0, distance: 20.0 }),
                FilterEntry::stub("Lens Blur"),
                FilterEntry::stub("Smart Blur"),
                FilterEntry::stub("Surface Blur"),
            ],
        },
        Category {
            name: "Distort",
            entries: vec![
                FilterEntry::stub("Diffuse Glow"),
                FilterEntry::stub("Glass"),
                FilterEntry::live("Lens Correction", Filter::LensCorrection { barrel: 0.0, pincushion: 0.0, vignette: 0.0 }),
                FilterEntry::stub("Ocean Ripple"),
                FilterEntry::live("Pinch", Filter::Pinch { amount: 0.5, radius: 1.0 }),
                FilterEntry::stub("Polar Coordinates"),
                FilterEntry::stub("Ripple"),
                FilterEntry::stub("Shear"),
                FilterEntry::live("Spherize", Filter::Pinch { amount: -0.5, radius: 1.0 }),
                FilterEntry::live("Twirl", Filter::Twirl { angle: 90.0, radius: 1.0 }),
                FilterEntry::stub("Wave"),
                FilterEntry::stub("ZigZag"),
            ],
        },
        Category {
            name: "Sketch",
            entries: vec![
                FilterEntry::stub("Bas Relief"),
                FilterEntry::stub("Chalk & Charcoal"),
                FilterEntry::stub("Charcoal"),
                FilterEntry::stub("Chrome"),
                FilterEntry::stub("Conte Crayon"),
                FilterEntry::stub("Graphic Pen"),
                FilterEntry::stub("Halftone Pattern"),
                FilterEntry::stub("Note Paper"),
                FilterEntry::stub("Photocopy"),
                FilterEntry::stub("Plaster"),
                FilterEntry::stub("Reticulation"),
                FilterEntry::stub("Stamp"),
                FilterEntry::stub("Torn Edges"),
                FilterEntry::stub("Water Paper"),
            ],
        },
        Category {
            name: "Stylize",
            entries: vec![
                FilterEntry::live("Find Edges", Filter::FindEdges { width: 1.0 }),
                FilterEntry::live("Emboss", Filter::Emboss { amount: 1.0, width: 1.0 }),
                FilterEntry::stub("Diffuse"),
                FilterEntry::stub("Extrude"),
                FilterEntry::live("Glowing Edges", Filter::GlowingEdges { width: 1.0, intensity: 2.0 }),
                FilterEntry::live("Solarize", Filter::Solarize { threshold: 0.5 }),
                FilterEntry::stub("Tiles"),
                FilterEntry::stub("Trace Contour"),
                FilterEntry::stub("Wind"),
            ],
        },
        Category {
            name: "Texture",
            entries: vec![
                FilterEntry::stub("Craquelure"),
                FilterEntry::stub("Grain"),
                FilterEntry::stub("Mosaic Tiles"),
                FilterEntry::stub("Patchwork"),
                FilterEntry::stub("Stained Glass"),
                FilterEntry::stub("Texturizer"),
            ],
        },
    ]
}

/// Render the Filter Gallery as a full-screen deferred overlay. The panel shows
/// all six categories with their effects; wired effects apply on click, stubs
/// show a "(stub)" badge. A "Close" button / clicking the backdrop closes the
/// gallery.
///
/// Called from `main.rs` when `app.filter_gallery_open` is `true`.
pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    // Build category rows before the closure capture.
    let cats = categories();

    // Build the expanded category list (each category as a collapsible-looking
    // header + entry rows). Since we have no per-row expand state in the simple
    // App model, all categories start expanded. A future wave can add a
    // HashMap<String, bool> to App for individual collapse state.
    let mut cat_elements: Vec<gpui::AnyElement> = Vec::new();
    for (ci, cat) in cats.iter().enumerate() {
        // Category header row.
        cat_elements.push(
            div()
                .px_3()
                .py(px(3.0))
                .bg(colors::surface_overlay())
                .text_color(colors::text_primary())
                .text_size(px(11.5))
                .font_weight(gpui::FontWeight::BOLD)
                .child(cat.name)
                .into_any_element(),
        );
        // Effect rows.
        for (ei, entry) in cat.entries.iter().enumerate() {
            let label = entry.label;
            let filter_opt = entry.filter.clone();
            let row_id = SharedString::from(format!("fg-{ci}-{ei}"));
            let is_live = filter_opt.is_some();
            let row = div()
                .id(row_id)
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_4()
                .py(px(2.5))
                .text_size(px(11.0))
                .text_color(if is_live {
                    colors::text_primary()
                } else {
                    colors::text_secondary()
                })
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    if let Some(f) = filter_opt.clone() {
                        root.app.apply(Action::ApplyFilter(f));
                    }
                    root.app.apply(Action::CloseFilterGallery);
                    cx.notify();
                }))
                .child(label)
                .when(!is_live, |d| {
                    d.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(colors::text_secondary())
                            .child("stub"),
                    )
                });
            cat_elements.push(row.into_any_element());
        }
    }

    let app_filter_gallery_open = app.filter_gallery_open;
    let _ = app_filter_gallery_open; // read to suppress dead_code; used in main.rs guard

    deferred(
        div()
            .absolute()
            .top(px(50.0))
            .left(px(50.0))
            .w(px(360.0))
            .h(px(520.0))
            .bg(colors::surface_raised())
            .border_1()
            .border_color(colors::surface_border())
            .rounded_md()
            .shadow_md()
            .flex()
            .flex_col()
            // Title bar.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .text_color(colors::text_primary())
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Filter Gallery"),
                    )
                    .child(
                        div()
                            .id("fg-close-btn")
                            .px_2()
                            .py(px(2.0))
                            .rounded_sm()
                            .bg(colors::surface_overlay())
                            .text_color(colors::text_secondary())
                            .text_size(px(10.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::CloseFilterGallery);
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
            // Scrollable category/effect list.
            .child(
                div()
                    .id("fg-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap_0()
                    .children(cat_elements),
            )
            // Footer: close button.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .child(
                        div()
                            .id("fg-close-footer")
                            .px_3()
                            .py(px(4.0))
                            .rounded_md()
                            .bg(colors::surface_overlay())
                            .text_color(colors::text_secondary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .on_click(cx.listener(|root, _ev, _win, cx| {
                                root.app.apply(Action::CloseFilterGallery);
                                cx.notify();
                            }))
                            .child("Close"),
                    ),
            ),
    )
    .with_priority(300)
}
