//! Top toolbar — full-width bar above the workspace.
//!
//! A dark Affinity/Photoshop-style top bar. Left→right:
//! - app-logo placeholder + persona/mode tabs (Photo / Develop / Export) as a
//!   visual pill toggle (one active, no Action),
//! - classic menu-bar labels (File … Help) as hover-highlight items whose
//!   dropdowns are visual stubs (no functional menus wired),
//! - document size + color-mode readout, then the live zoom controls:
//!   `−` → `ZoomBy(0.8)`, the current zoom %, `+` → `ZoomBy(1.25)`,
//!   `Fit` → `ResetView`, and a visual-only `Export PNG` button.
//!
//! Undo/Redo and Export buttons use `prism_ui::icon` for icon rendering.
//! The zoom/fit buttons are the only wired interactions that also use the panel
//! convention (`cx.listener` + `root.app.apply(...)`). Everything else is
//! presentation only this wave.

use gpui::prelude::FluentBuilder;
use gpui::{
    deferred, div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, font_size, icon_colored, icon, radius, Icon};

use crate::app_state::{
    Action, App, ColorProfile, Filter, RulerUnit, SoftProofMode, Tool, WorkingColorMode,
    WorkingSpace,
};
use crate::windows;
use crate::Pigment;

#[allow(unused_imports)]
use rfd;

const MENUS: [&str; 10] = [
    "File", "Edit", "Image", "Layer", "Select", "Filter", "View", "Color", "Window", "Help",
];

/// Persona/mode tabs — purely visual; `active` decides the accent pill.
const PERSONAS: [(&str, bool); 3] = [("Photo", true), ("Develop", false), ("Export", false)];

/// Compact icon+label button for toolbar actions.
#[allow(dead_code)]
fn labeled_icon_btn(
    cx: &mut Context<Pigment>,
    id: &'static str,
    ico: Icon,
    label: &'static str,
    on_click: impl Fn(&mut Pigment, &mut Context<Pigment>) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex().flex_row().items_center().gap_1()
        .h(px(26.0)).px(px(6.0))
        .rounded(px(radius::MD))
        .bg(colors::surface_overlay())
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| on_click(root, cx)))
        .child(icon(ico, 12.0))
        .child(
            div()
                .text_size(px(font_size::XS))
                .text_color(colors::text_secondary())
                .child(label),
        )
}


/// Build the contextual tool-options strip for the active tool.
fn tool_options(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let active = app.active;
    match active {
        Tool::Brush | Tool::Eraser | Tool::Dodge | Tool::Burn | Tool::Smudge | Tool::Clone | Tool::Heal => {
            let size = app.brush.size;
            let hard = app.brush.hardness;
            let opac = app.brush.opacity;
            let sizes = [1.0_f32, 5.0, 10.0, 20.0, 50.0, 100.0];
            // Next size in cycle.
            let next_size = sizes
                .iter()
                .find(|&&s| s > size)
                .copied()
                .unwrap_or(sizes[0]);
            div()
                .flex().flex_row().items_center().gap_2()
                .child(
                    div()
                        .id("tb-brush-size")
                        .px(px(6.0)).h(px(22.0))
                        .flex().items_center()
                        .rounded(px(radius::SM))
                        .bg(colors::surface_overlay())
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::SetBrushSize(next_size));
                            cx.notify();
                        }))
                        .child(format!("{size:.0}px")),
                )
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child(format!("{:.0}% hard", hard * 100.0)),
                )
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child(format!("{:.0}% opac", opac * 100.0)),
                )
        }
        Tool::Text => {
            let ts = app.text_size;
            div()
                .flex().flex_row().items_center().gap_1()
                .child(
                    div()
                        .id("tb-text-sz-dn")
                        .w(px(20.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded(px(radius::SM))
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let next = (root.app.text_size - 2.0).max(6.0);
                            root.app.apply(Action::SetTextSize(next));
                            cx.notify();
                        }))
                        .child(icon(Icon::ArrowDown, 10.0)),
                )
                .child(
                    div()
                        .px(px(6.0)).h(px(22.0))
                        .flex().items_center()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_secondary())
                        .child(format!("{ts:.0}pt")),
                )
                .child(
                    div()
                        .id("tb-text-sz-up")
                        .w(px(20.0)).h(px(22.0))
                        .flex().items_center().justify_center()
                        .rounded(px(radius::SM))
                        .bg(colors::surface_overlay())
                        .cursor_pointer()
                        .hover(|s| s.bg(colors::tool_hover()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let next = (root.app.text_size + 2.0).min(400.0);
                            root.app.apply(Action::SetTextSize(next));
                            cx.notify();
                        }))
                        .child(icon(Icon::ArrowUp, 10.0)),
                )
        }
        Tool::Fill => {
            let tol = app.fill_tolerance;
            div()
                .flex().flex_row().items_center()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child(format!("Tol {:.0}%", tol * 100.0)),
                )
        }
        Tool::Eyedropper => {
            div()
                .flex().flex_row().items_center()
                .child(
                    div()
                        .text_size(px(font_size::XS))
                        .text_color(colors::text_disabled())
                        .child("Point Sample"),
                )
        }
        _ => div().flex().flex_row().items_center(),
    }
}

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let zoom_pct = format!("{:.0}%", app.view.zoom * 100.0);
    // Status message (Wave 11).
    let status = app.status_message.clone();
    // Undo/redo availability + next-step labels for the Edit controls.
    let (undos, redos) = app.host.history_labels();
    let can_undo = !undos.is_empty();
    let can_redo = !redos.is_empty();
    let undo_title = undos
        .last()
        .map(|l| format!("Undo {l}"))
        .unwrap_or_else(|| "Undo".to_string());
    let redo_title = redos
        .first()
        .map(|l| format!("Redo {l}"))
        .unwrap_or_else(|| "Redo".to_string());
    let readout = format!(
        "{} × {} px  ·  RGBA/8",
        app.doc.size.width, app.doc.size.height
    );

    // --- Left: logo placeholder + persona pill toggle ---
    let logo = div()
        .w(px(22.0))
        .h(px(22.0))
        .rounded_md()
        .bg(colors::surface_overlay())
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::MD))
        .text_color(colors::text_primary())
        .child("P");

    let personas = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .p(px(2.0))
        .rounded_md()
        .bg(colors::surface_bg())
        .children(PERSONAS.into_iter().map(|(name, active)| {
            let mut tab = div()
                .px_2()
                .py(px(3.0))
                .rounded_md()
                .text_size(px(font_size::MD))
                .child(name);
            if active {
                tab = tab.bg(colors::accent()).text_color(colors::text_primary());
            } else {
                tab = tab.text_color(colors::text_secondary());
            }
            tab
        }));

    let left = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(logo)
        .child(personas);

    // --- Center-left: classic menu-bar labels (click to open dropdown) ---
    let active_menu = app.active_menu.clone();
    let menus = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .children(MENUS.into_iter().map(|label| {
            let is_open = active_menu.as_deref() == Some(label);
            let label_str = label.to_string();
            div()
                .id(("tb-menu", label.as_ptr() as u64))
                .relative()
                .child(
                    div()
                        .id(("tb-menu-btn", label.as_ptr() as u64))
                        .px_2()
                        .py(px(3.0))
                        .rounded_md()
                        .text_size(px(font_size::MD))
                        .text_color(colors::text_primary())
                        .cursor_pointer()
                        .when(is_open, |d| d.bg(colors::surface_overlay()))
                        .hover(|s| s.bg(colors::surface_overlay()))
                        .on_click({
                            let label_str = label_str.clone();
                            cx.listener(move |root, _ev, _win, cx| {
                                let next = if root.app.active_menu.as_deref() == Some(&label_str) {
                                    None
                                } else {
                                    Some(label_str.clone())
                                };
                                root.app.apply(Action::OpenMenu(next));
                                cx.notify();
                            })
                        })
                        .child(label),
                )
                .when(is_open, |d| {
                    d.child(deferred(menu_dropdown(cx, label)).with_priority(100))
                })
        }));

    // --- Right: readout + zoom controls + export ---
    let readout_el = div()
        .text_size(px(font_size::SM))
        .text_color(colors::text_secondary())
        .child(readout);

    // --- Edit: Undo / Redo icons (also bound to Cmd+Z / Cmd+Shift+Z) ---
    let undo_icon_color = if can_undo {
        colors::text_primary()
    } else {
        colors::text_disabled()
    };
    let redo_icon_color = if can_redo {
        colors::text_primary()
    } else {
        colors::text_disabled()
    };

    let undo_btn = div()
        .id("tb-undo")
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .child(icon_colored(Icon::Undo, 14.0, undo_icon_color))
        .when(can_undo, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::Undo);
                    cx.notify();
                }))
        });

    let redo_btn = div()
        .id("tb-redo")
        .w(px(28.0))
        .h(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .child(icon_colored(Icon::Redo, 14.0, redo_icon_color))
        .when(can_redo, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(colors::tool_hover()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::Redo);
                    cx.notify();
                }))
        });
    // Titles surfaced for context (next undo/redo step), kept lightweight.
    let _ = (&undo_title, &redo_title);

    // --- Contextual tool options ---
    let tool_opts = tool_options(app, cx);

    let zoom_minus = div()
        .id("tb-zoom-out")
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(14.0))
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ZoomBy(0.8));
            cx.notify();
        }))
        .child(icon(Icon::ZoomOut, 14.0));

    let zoom_label = div()
        .min_w(px(44.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(font_size::MD))
        .text_color(colors::text_primary())
        .child(zoom_pct);

    let zoom_plus = div()
        .id("tb-zoom-in")
        .w(px(24.0))
        .h(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .bg(colors::surface_overlay())
        .text_color(colors::text_primary())
        .text_size(px(14.0))
        .cursor_pointer()
        .hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ZoomBy(1.25));
            cx.notify();
        }))
        .child(icon(Icon::ZoomIn, 14.0));

    // Status message display.
    let status_el = status.map(|msg| {
        div()
            .flex()
            .items_center()
            .px_2()
            .h(px(22.0))
            .rounded_md()
            .bg(colors::surface_overlay())
            .text_color(colors::text_secondary())
            .text_size(px(font_size::XS))
            .child(msg)
    });

    // --- Crop confirm/cancel bar (shown only when crop_rect is Some) ---
    let crop_bar = app.crop_rect.map(|r| {
        let x = r[0].min(r[2]).round() as u32;
        let y = r[1].min(r[3]).round() as u32;
        let w = (r[0] - r[2]).abs().round() as u32;
        let h = (r[1] - r[3]).abs().round() as u32;
        let label = format!("Crop {w}×{h}");
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .id("tb-crop-label")
                    .px_2()
                    .py(px(3.0))
                    .text_size(px(font_size::XS))
                    .text_color(colors::text_secondary())
                    .child(label),
            )
            .child(
                div()
                    .id("tb-apply-crop")
                    .px_2()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(colors::accent())
                    .text_color(colors::text_primary())
                    .text_size(px(font_size::XS))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::ApplyCrop { x, y, w: w.max(1), h: h.max(1) });
                        cx.notify();
                    }))
                    .child("Apply"),
            )
            .child(
                div()
                    .id("tb-cancel-crop")
                    .px_2()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .rounded_md()
                    .bg(colors::surface_overlay())
                    .text_color(colors::text_secondary())
                    .text_size(px(font_size::XS))
                    .cursor_pointer()
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        root.app.apply(Action::CancelCrop);
                        cx.notify();
                    }))
                    .child("Cancel"),
            )
    });

    let right = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .when_some(crop_bar, |s, el| s.child(el))
        .when_some(status_el, |s, el| s.child(el))
        .child(tool_opts)
        .child(
            div()
                .flex().flex_row().items_center().gap_1()
                .child(undo_btn)
                .child(redo_btn),
        )
        .child(readout_el)
        .child(
            div()
                .flex().flex_row().items_center().gap_1()
                .child(zoom_minus)
                .child(zoom_label)
                .child(zoom_plus),
        );

    div()
        .size_full()
        .h(px(40.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_3()
        .gap_3()
        .bg(colors::surface_raised())
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        // Left cluster groups the logo/personas with the menu bar so they stay
        // packed at the start while the right cluster floats to the end.
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_4()
                .child(left)
                .child(menus),
        )
        .child(right)
}

/// Render a dropdown for the given menu label. Positioned absolutely below the button.
fn menu_dropdown(cx: &mut Context<Pigment>, label: &str) -> impl IntoElement {
    let items: &[(&str, fn(&mut Pigment, &mut Context<Pigment>))] = match label {
        "File" => &[
            ("Open…", |root, cx| { root.app.apply(Action::OpenImage); cx.notify(); }),
            ("Open EXR…", |root, cx| { root.app.apply(Action::OpenEXR); cx.notify(); }),
            ("Save As…", |root, cx| { root.app.apply(Action::OpenSaveAsDialog); cx.notify(); }),
            ("Export…", |root, cx| { root.app.apply(Action::ExportImage); cx.notify(); }),
            ("Export EXR…", |root, cx| { root.app.apply(Action::ExportEXR); cx.notify(); }),
            ("Export Slices…", |root, cx| {
                if let Some(dir) = rfd::FileDialog::new().set_title("Export Slices To Folder").pick_folder() {
                    root.app.apply(Action::ExportSlices(dir));
                }
                cx.notify();
            }),
            ("Export Preset: JPEG 90%", |root, cx| { root.app.apply(Action::ExportWithPreset(0)); cx.notify(); }),
            ("Export Preset: PNG Lossless", |root, cx| { root.app.apply(Action::ExportWithPreset(1)); cx.notify(); }),
            ("Import PSD…", |root, cx| { root.app.apply(Action::OpenImportPsdDialog); cx.notify(); }),
            ("Import RAW…", |root, cx| { root.app.apply(Action::OpenImportRawDialog); cx.notify(); }),
        ],
        "Edit" => &[
            ("Undo", |root, cx| { root.app.apply(Action::Undo); cx.notify(); }),
            ("Redo", |root, cx| { root.app.apply(Action::Redo); cx.notify(); }),
            ("Select All", |root, cx| { root.app.apply(Action::SelectAll); cx.notify(); }),
            ("Deselect", |root, cx| { root.app.apply(Action::ClearSelection); cx.notify(); }),
            ("Keyboard Shortcuts…", |_root, cx| {
                let weak = cx.entity().downgrade();
                windows::open_shortcuts(cx, weak);
                cx.notify();
            }),
            ("Preferences…", |_root, cx| {
                let weak = cx.entity().downgrade();
                windows::open_preferences(cx, weak);
                cx.notify();
            }),
        ],
        "Image" => &[
            ("Image Size…", |root, cx| { root.app.apply(Action::SetImageSize { width: root.app.host.doc_w, height: root.app.host.doc_h }); cx.notify(); }),
            ("Canvas Size…", |root, cx| { root.app.apply(Action::SetCanvasSize { width: root.app.host.doc_w, height: root.app.host.doc_h }); cx.notify(); }),
            ("Flatten Layers", |root, cx| { root.app.apply(Action::FlattenLayers); cx.notify(); }),
            ("Rotate Canvas 90°", |root, cx| { root.app.apply(Action::RotateCanvas(90.0)); cx.notify(); }),
            ("Reset Rotation", |root, cx| { root.app.apply(Action::ResetCanvasRotation); cx.notify(); }),
        ],
        "Layer" => &[
            ("New Layer", |root, cx| { root.app.apply(Action::NewLayer); cx.notify(); }),
            ("Duplicate Layer", |root, cx| { root.app.apply(Action::DuplicateLayer); cx.notify(); }),
            ("Delete Layer", |root, cx| {
                if let Some(id) = root.app.doc.active_layer {
                    root.app.apply(Action::DeleteLayer(id));
                    cx.notify();
                }
            }),
            ("Merge Down", |root, cx| { root.app.apply(Action::MergeDown); cx.notify(); }),
            ("Create Layer Group", |root, cx| { root.app.apply(Action::CreateLayerGroup("Group".to_string())); cx.notify(); }),
            ("Flatten Layers", |root, cx| { root.app.apply(Action::FlattenLayers); cx.notify(); }),
        ],
        "Select" => &[
            ("Select All", |root, cx| { root.app.apply(Action::SelectAll); cx.notify(); }),
            ("Deselect All", |root, cx| { root.app.apply(Action::ClearSelection); cx.notify(); }),
        ],
        "Filter" => &[
            ("Filter Gallery…", |root, cx| { root.app.apply(Action::OpenFilterGallery); cx.notify(); }),
            ("Camera Raw…", |root, cx| { root.app.apply(Action::OpenCameraRaw); cx.notify(); }),
            ("Gaussian Blur", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::GaussianBlur { radius: 3.0 })); cx.notify(); }),
            ("Sharpen", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::Sharpen { amount: 0.5 })); cx.notify(); }),
            ("Add Noise", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::AddNoise { amount: 0.05 })); cx.notify(); }),
            ("Motion Blur", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::MotionBlur { angle: 0.0, distance: 20.0 })); cx.notify(); }),
            ("Twirl", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::Twirl { angle: 90.0, radius: 1.0 })); cx.notify(); }),
            ("Pinch", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::Pinch { amount: 0.5, radius: 1.0 })); cx.notify(); }),
            ("Glowing Edges", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::GlowingEdges { width: 1.0, intensity: 2.0 })); cx.notify(); }),
            ("Solarize", |root, cx| { root.app.apply(Action::ApplyFilter(Filter::Solarize { threshold: 0.5 })); cx.notify(); }),
        ],
        "View" => &[
            ("Zoom In", |root, cx| { root.app.apply(Action::ZoomBy(1.25)); cx.notify(); }),
            ("Zoom Out", |root, cx| { root.app.apply(Action::ZoomBy(0.8)); cx.notify(); }),
            ("Fit Canvas", |root, cx| { root.app.apply(Action::ResetView); cx.notify(); }),
            ("New Guide: Horizontal", |root, cx| {
                let pos = root.app.host.doc_h as f32 * 0.5;
                root.app.apply(Action::AddCanvasGuide { horizontal: true, position: pos });
                cx.notify();
            }),
            ("New Guide: Vertical", |root, cx| {
                let pos = root.app.host.doc_w as f32 * 0.5;
                root.app.apply(Action::AddCanvasGuide { horizontal: false, position: pos });
                cx.notify();
            }),
            ("Clear Guides", |root, cx| { root.app.apply(Action::ClearCanvasGuides); cx.notify(); }),
            ("Toggle Guide Visibility", |root, cx| {
                let v = !root.app.guide_state.visible;
                root.app.apply(Action::SetGuidesVisible(v));
                cx.notify();
            }),
            ("Toggle Snap to Guides", |root, cx| {
                let v = !root.app.guide_state.snap_enabled;
                root.app.apply(Action::SetGuideSnapEnabled(v));
                cx.notify();
            }),
            ("Toggle Smart Guides", |root, cx| {
                let v = !root.app.guide_state.smart_guides;
                root.app.apply(Action::SetSmartGuidesEnabled(v));
                cx.notify();
            }),
            ("Ruler Unit: Pixels", |root, cx| { root.app.apply(Action::SetRulerUnit(RulerUnit::Pixels)); cx.notify(); }),
            ("Ruler Unit: Inches", |root, cx| { root.app.apply(Action::SetRulerUnit(RulerUnit::Inches)); cx.notify(); }),
            ("Ruler Unit: Centimeters", |root, cx| { root.app.apply(Action::SetRulerUnit(RulerUnit::Centimeters)); cx.notify(); }),
            ("Toggle Guides (legacy)", |root, cx| { root.app.apply(Action::ToggleGuides); cx.notify(); }),
            ("Soft Proof: Off", |root, cx| { root.app.apply(Action::SetSoftProof(SoftProofMode::Off)); cx.notify(); }),
            ("Soft Proof: CMYK", |root, cx| { root.app.apply(Action::SetSoftProof(SoftProofMode::Cmyk)); cx.notify(); }),
            ("Color Profile: sRGB", |root, cx| { root.app.apply(Action::SetColorProfile(ColorProfile::Srgb)); cx.notify(); }),
            ("Color Profile: Adobe RGB", |root, cx| { root.app.apply(Action::SetColorProfile(ColorProfile::AdobeRgb)); cx.notify(); }),
            ("Color Profile: Display P3", |root, cx| { root.app.apply(Action::SetColorProfile(ColorProfile::P3)); cx.notify(); }),
        ],
        "Color" => &[
            ("Mode: RGB", |root, cx| { root.app.apply(Action::SetWorkingColorMode(WorkingColorMode::Rgb)); cx.notify(); }),
            ("Mode: Grayscale", |root, cx| { root.app.apply(Action::SetWorkingColorMode(WorkingColorMode::Grayscale)); cx.notify(); }),
            ("Mode: CMYK", |root, cx| { root.app.apply(Action::SetWorkingColorMode(WorkingColorMode::Cmyk)); cx.notify(); }),
            ("Mode: Lab", |root, cx| { root.app.apply(Action::SetWorkingColorMode(WorkingColorMode::Lab)); cx.notify(); }),
            ("Assign: sRGB", |root, cx| { root.app.apply(Action::AssignWorkingSpace(WorkingSpace::SRgb)); cx.notify(); }),
            ("Assign: Adobe RGB", |root, cx| { root.app.apply(Action::AssignWorkingSpace(WorkingSpace::AdobeRgb)); cx.notify(); }),
            ("Assign: Display P3", |root, cx| { root.app.apply(Action::AssignWorkingSpace(WorkingSpace::DisplayP3)); cx.notify(); }),
            ("Assign: ProPhoto RGB", |root, cx| { root.app.apply(Action::AssignWorkingSpace(WorkingSpace::ProPhotoRgb)); cx.notify(); }),
            ("Convert to CMYK (SWOP)", |root, cx| {
                root.app.apply(Action::ConvertWorkingSpace { mode: WorkingColorMode::Cmyk, space: WorkingSpace::UsWebCoatedSwop });
                cx.notify();
            }),
            ("Convert to Grayscale", |root, cx| {
                root.app.apply(Action::ConvertWorkingSpace { mode: WorkingColorMode::Grayscale, space: WorkingSpace::GrayGamma22 });
                cx.notify();
            }),
            ("Toggle Embed Profile", |root, cx| {
                let v = !root.app.color_management.embed_profile;
                root.app.apply(Action::SetEmbedColorProfile(v));
                cx.notify();
            }),
        ],
        "Window" => &[
            ("Toggle Layers", |root, cx| { root.app.apply(Action::TogglePanel("Layers".to_string())); cx.notify(); }),
            ("Toggle Color", |root, cx| { root.app.apply(Action::TogglePanel("Color".to_string())); cx.notify(); }),
            ("Toggle History", |root, cx| { root.app.apply(Action::TogglePanel("History".to_string())); cx.notify(); }),
            ("Script Editor…", |_root, cx| {
                let weak = cx.entity().downgrade();
                windows::open_script_editor(cx, weak);
                cx.notify();
            }),
            ("New / Image Size…", |_root, cx| {
                let weak = cx.entity().downgrade();
                windows::open_new_document(cx, weak);
                cx.notify();
            }),
        ],
        _ => &[],
    };

    div()
        .absolute()
        .top(px(28.0))
        .left_0()
        .w(px(180.0))
        .bg(colors::surface_raised())
        .border_1()
        .border_color(colors::surface_border())
        .rounded_md()
        .shadow_md()
        .py_1()
        .flex()
        .flex_col()
        .children(items.iter().enumerate().map(|(i, (item_label, handler))| {
            let item_label = *item_label;
            div()
                .id(("menu-item", (label.as_ptr() as u64).wrapping_add(i as u64)))
                .px_3()
                .py(px(4.0))
                .text_size(px(font_size::SM))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .hover(|s| s.bg(colors::surface_overlay()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    handler(root, cx);
                    root.app.apply(Action::OpenMenu(None));
                    cx.notify();
                }))
                .child(item_label)
        }))
}
