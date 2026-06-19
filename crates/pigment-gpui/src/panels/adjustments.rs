//! Adjustments / filters browser — a categorized, scrollable list of every
//! adjustment and filter the editor offers, mirroring the egui app's Filter
//! menu (see `pigment-app/src/app/view.rs`).
//!
//! The "Adjustments" section is WIRED: each row adds a non-destructive adjustment
//! LAYER (`Action::AddAdjustment(AdjKind)`) on top of the stack, affecting the
//! layers below — exactly the egui app's "Adj" menu. The remaining sections
//! (Blur / Sharpen / …) are destructive filters and stay visual-only this wave
//! (no adjustment/filter math lives here; the engine owns it).

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, icon_colored, section_header, Icon};

use crate::app_state::{Action, AdjKind, App};
use crate::Pigment;

/// Every section of the browser: a header label plus its rows. The data drives
/// the render so adding a filter later is a one-line edit.
const SECTIONS: &[(&str, &[&str])] = &[
    (
        "Adjustments",
        &[
            "Brightness/Contrast",
            "Levels",
            "Curves",
            "Hue/Saturation",
            "Color Balance",
            "Vibrance",
            "Black & White",
            "Photo Filter",
            "Channel Mixer",
            "Posterize",
            "Threshold",
            "Invert",
            "Gradient Map",
        ],
    ),
    (
        "Blur",
        &[
            "Gaussian",
            "Box",
            "Motion",
            "Radial",
            "Tilt-Shift",
            "Iris",
            "Spin",
            "Field",
        ],
    ),
    ("Sharpen", &["Unsharp Mask", "High Pass"]),
    ("Distort", &["Twirl", "Pinch", "Ripple", "Polar Coordinates"]),
    (
        "Stylize",
        &["Find Edges", "Emboss", "Glowing Edges", "Diffuse", "Oil Paint"],
    ),
    ("Noise", &["Add Noise", "Median", "Dust & Scratches"]),
    (
        "Pixelate",
        &["Mosaic", "Crystallize", "Color Halftone", "Mezzotint"],
    ),
    ("Render", &["Clouds", "Difference Clouds"]),
];

/// Map an Adjustments-section row label to the adjustment-layer kind it adds.
/// Returns `None` for non-adjustment sections (filters), which stay visual-only.
fn adj_kind(label: &str) -> Option<AdjKind> {
    Some(match label {
        "Brightness/Contrast" => AdjKind::BrightnessContrast,
        "Levels" => AdjKind::Levels,
        "Curves" => AdjKind::Curves,
        "Hue/Saturation" => AdjKind::HueSaturation,
        "Color Balance" => AdjKind::ColorBalance,
        "Vibrance" => AdjKind::Vibrance,
        "Black & White" => AdjKind::BlackWhite,
        "Photo Filter" => AdjKind::PhotoFilter,
        "Channel Mixer" => AdjKind::ChannelMixer,
        "Posterize" => AdjKind::Posterize,
        "Threshold" => AdjKind::Threshold,
        "Invert" => AdjKind::Invert,
        "Gradient Map" => AdjKind::GradientMap,
        _ => return None,
    })
}

/// One clickable browser row. Adjustment rows add an adjustment layer on click;
/// filter rows are visual-only (cursor + hover/press styling, no listener).
fn row(
    section_idx: usize,
    row_idx: usize,
    label: &str,
    cx: &mut Context<Pigment>,
) -> impl IntoElement {
    let kind = adj_kind(label);
    div()
        .id(("adj-row", (section_idx as u64) << 32 | row_idx as u64))
        .flex()
        .items_center()
        .w_full()
        .py_1()
        .px_2()
        .text_size(px(font_size::MD))
        .text_color(colors::text_primary())
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .active(|s| s.bg(colors::accent()).text_color(colors::text_primary()))
        .when_some(kind, |s, k| {
            s.on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::AddAdjustment(k));
                cx.notify();
            }))
        })
        .child(label.to_string())
}

/// A section header band with a static ▾ chevron icon (drawn open; expand/collapse
/// is a later wave's job).
fn adj_section_header(label: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .w_full()
        .py_1()
        .px_2()
        .bg(colors::surface_overlay())
        .border_b_1()
        .border_color(colors::surface_bg())
        .child(icon_colored(Icon::ChevronDown, 9.0, colors::text_disabled()))
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(label.to_uppercase()),
        )
}

pub fn render(_app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let sections = SECTIONS
        .iter()
        .enumerate()
        .map(|(si, (title, rows))| {
            div()
                .flex()
                .flex_col()
                .w_full()
                .child(adj_section_header(title))
                .children(
                    rows.iter()
                        .enumerate()
                        .map(|(ri, label)| row(si, ri, label, cx))
                        .collect::<Vec<_>>(),
                )
                // Hairline between this section's rows and the next header.
                .child(divider())
        })
        .collect::<Vec<_>>();

    div()
        .id("adjustments-browser")
        .flex()
        .flex_col()
        .w_full()
        // Bounded height so the body scrolls within the dock rather than
        // pushing the panels below it off-screen.
        .max_h(px(360.0))
        .bg(colors::surface_raised())
        .overflow_y_scroll()
        .child(section_header("Adjustments"))
        .child(divider())
        .children(sections)
}
