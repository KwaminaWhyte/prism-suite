//! Layer Style panel — collapsible sections for Drop Shadow / Outer Glow /
//! Inner Glow / Bevel & Emboss. Shown when `App::style_panel_open` is true.
//!
//! Each section has a toggle and param steppers. All mutations go through
//! `Action::SetLayerStyle(id, LayerStyle)`.

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::{colors, divider, font_size, section_header, spacing};

use crate::app_state::{Action, App, Bevel, Glow, Shadow};
use crate::Pigment;

pub fn render(app: &App, cx: &mut Context<Pigment>) -> impl IntoElement {
    let Some(id) = app.style_panel_layer else {
        return div()
            .id("lsr-empty")
            .p_3()
            .text_color(colors::text_secondary())
            .text_size(px(font_size::SM))
            .child("No layer selected");
    };

    let style = app.layer_styles.get(&id).cloned().unwrap_or_default();

    // ---- Drop Shadow section ---------------------------------------------------
    let ds_enabled = style.drop_shadow.is_some();
    let ds = style.drop_shadow.clone().unwrap_or_default();

    let ds_dot_bg = if ds_enabled { colors::accent() } else { colors::surface_overlay() };
    let ds_toggle_row = div()
        .id("lsr-ds-tog")
        .flex().flex_row().items_center().gap_2()
        .px(px(spacing::MD)).py(px(spacing::XS))
        .cursor_pointer()
        .hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener({
            let s = style.clone();
            move |root, _ev, _win, cx| {
                let mut s2 = s.clone();
                if s2.drop_shadow.is_some() {
                    s2.drop_shadow = None;
                } else {
                    s2.drop_shadow = Some(Shadow { offset_x: 5.0, offset_y: 5.0,
                        blur: 10.0, spread: 0.0, color: [0.0,0.0,0.0,1.0], opacity: 0.75 });
                }
                root.app.apply(Action::SetLayerStyle(id, s2));
                cx.notify();
            }
        }))
        .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(ds_dot_bg).border_1().border_color(colors::surface_border()))
        .child(div().flex_1().text_color(if ds_enabled { colors::text_primary() } else { colors::text_secondary() })
            .text_size(px(font_size::SM)).child("Drop Shadow"));

    let ds_params = if ds_enabled {
        let (s1, s2, s3, s4) = (style.clone(), style.clone(), style.clone(), style.clone());
        Some(div().flex().flex_col().gap_1().px(px(spacing::MD)).pb(px(spacing::XS))
            .child(f32_stepper("lsr-ds-ox", "Offset X", ds.offset_x,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s1.clone();
                    if let Some(ref mut sh) = s.drop_shadow { sh.offset_x += 1.0; }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut sh) = s2.drop_shadow { sh.offset_x -= 1.0; }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-ds-oy", "Offset Y", ds.offset_y,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s2.clone();
                    if let Some(ref mut sh) = s.drop_shadow { sh.offset_y += 1.0; }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut sh) = s2.drop_shadow { sh.offset_y -= 1.0; }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-ds-blur", "Blur", ds.blur,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s3.clone();
                    if let Some(ref mut sh) = s.drop_shadow { sh.blur = (sh.blur + 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut sh) = s2.drop_shadow { sh.blur = (sh.blur - 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-ds-op", "Opacity", ds.opacity,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s4.clone();
                    if let Some(ref mut sh) = s.drop_shadow { sh.opacity = (sh.opacity + 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut sh) = s2.drop_shadow { sh.opacity = (sh.opacity - 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            )))
    } else { None };

    // ---- Outer Glow section ---------------------------------------------------
    let og_enabled = style.outer_glow.is_some();
    let og = style.outer_glow.clone().unwrap_or_default();

    let og_dot_bg = if og_enabled { colors::accent() } else { colors::surface_overlay() };
    let og_toggle_row = div()
        .id("lsr-og-tog")
        .flex().flex_row().items_center().gap_2()
        .px(px(spacing::MD)).py(px(spacing::XS))
        .cursor_pointer().hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
            let mut s2 = s.clone();
            if s2.outer_glow.is_some() { s2.outer_glow = None; }
            else { s2.outer_glow = Some(Glow { blur: 15.0, spread: 0.0, color: [1.0,0.85,0.0,1.0], opacity: 0.75 }); }
            root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
        }}))
        .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(og_dot_bg).border_1().border_color(colors::surface_border()))
        .child(div().flex_1().text_color(if og_enabled { colors::text_primary() } else { colors::text_secondary() }).text_size(px(font_size::SM)).child("Outer Glow"));

    let og_params = if og_enabled {
        let (s1, s2) = (style.clone(), style.clone());
        Some(div().flex().flex_col().gap_1().px(px(spacing::MD)).pb(px(spacing::XS))
            .child(f32_stepper("lsr-og-blur", "Blur", og.blur,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s1.clone();
                    if let Some(ref mut g) = s.outer_glow { g.blur = (g.blur + 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut g) = s2.outer_glow { g.blur = (g.blur - 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-og-op", "Opacity", og.opacity,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s2.clone();
                    if let Some(ref mut g) = s.outer_glow { g.opacity = (g.opacity + 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut g) = s2.outer_glow { g.opacity = (g.opacity - 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            )))
    } else { None };

    // ---- Inner Glow section ---------------------------------------------------
    let ig_enabled = style.inner_glow.is_some();
    let ig = style.inner_glow.clone().unwrap_or_default();

    let ig_dot_bg = if ig_enabled { colors::accent() } else { colors::surface_overlay() };
    let ig_toggle_row = div()
        .id("lsr-ig-tog")
        .flex().flex_row().items_center().gap_2()
        .px(px(spacing::MD)).py(px(spacing::XS))
        .cursor_pointer().hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
            let mut s2 = s.clone();
            if s2.inner_glow.is_some() { s2.inner_glow = None; }
            else { s2.inner_glow = Some(Glow { blur: 10.0, spread: 0.0, color: [1.0,1.0,1.0,1.0], opacity: 0.75 }); }
            root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
        }}))
        .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(ig_dot_bg).border_1().border_color(colors::surface_border()))
        .child(div().flex_1().text_color(if ig_enabled { colors::text_primary() } else { colors::text_secondary() }).text_size(px(font_size::SM)).child("Inner Glow"));

    let ig_params = if ig_enabled {
        let (s1, s2) = (style.clone(), style.clone());
        Some(div().flex().flex_col().gap_1().px(px(spacing::MD)).pb(px(spacing::XS))
            .child(f32_stepper("lsr-ig-blur", "Blur", ig.blur,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s1.clone();
                    if let Some(ref mut g) = s.inner_glow { g.blur = (g.blur + 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut g) = s2.inner_glow { g.blur = (g.blur - 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-ig-op", "Opacity", ig.opacity,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s2.clone();
                    if let Some(ref mut g) = s.inner_glow { g.opacity = (g.opacity + 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut g) = s2.inner_glow { g.opacity = (g.opacity - 0.05).clamp(0.0,1.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            )))
    } else { None };

    // ---- Bevel & Emboss section -----------------------------------------------
    let be_enabled = style.bevel_emboss.is_some();
    let be = style.bevel_emboss.clone().unwrap_or_default();

    let be_dot_bg = if be_enabled { colors::accent() } else { colors::surface_overlay() };
    let be_toggle_row = div()
        .id("lsr-be-tog")
        .flex().flex_row().items_center().gap_2()
        .px(px(spacing::MD)).py(px(spacing::XS))
        .cursor_pointer().hover(|s| s.bg(colors::surface_overlay()))
        .on_click(cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
            let mut s2 = s.clone();
            if s2.bevel_emboss.is_some() { s2.bevel_emboss = None; }
            else { s2.bevel_emboss = Some(Bevel { depth: 1.0, size: 5.0, angle: 120.0, highlight_opacity: 0.75, shadow_opacity: 0.75 }); }
            root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
        }}))
        .child(div().w(px(10.0)).h(px(10.0)).rounded_full().bg(be_dot_bg).border_1().border_color(colors::surface_border()))
        .child(div().flex_1().text_color(if be_enabled { colors::text_primary() } else { colors::text_secondary() }).text_size(px(font_size::SM)).child("Bevel & Emboss"));

    let be_params = if be_enabled {
        let (s1, s2, s3) = (style.clone(), style.clone(), style.clone());
        Some(div().flex().flex_col().gap_1().px(px(spacing::MD)).pb(px(spacing::XS))
            .child(f32_stepper("lsr-be-depth", "Depth", be.depth,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s1.clone();
                    if let Some(ref mut b) = s.bevel_emboss { b.depth = (b.depth + 0.1).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut b) = s2.bevel_emboss { b.depth = (b.depth - 0.1).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-be-size", "Size", be.size,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s2.clone();
                    if let Some(ref mut b) = s.bevel_emboss { b.size = (b.size + 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut b) = s2.bevel_emboss { b.size = (b.size - 1.0).max(0.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            ))
            .child(f32_stepper("lsr-be-angle", "Angle°", be.angle,
                cx.listener(move |root, _ev, _win, cx| {
                    let mut s = s3.clone();
                    if let Some(ref mut b) = s.bevel_emboss { b.angle = (b.angle + 5.0).rem_euclid(360.0); }
                    root.app.apply(Action::SetLayerStyle(id, s)); cx.notify();
                }),
                cx.listener({let s = style.clone(); move |root, _ev, _win, cx| {
                    let mut s2 = s.clone();
                    if let Some(ref mut b) = s2.bevel_emboss { b.angle = (b.angle - 5.0).rem_euclid(360.0); }
                    root.app.apply(Action::SetLayerStyle(id, s2)); cx.notify();
                }}),
            )))
    } else { None };

    // ---- Footer ---------------------------------------------------------------
    let close_btn = div()
        .id("lsr-close")
        .h(px(22.0)).px_2()
        .flex().items_center().justify_center()
        .rounded_md().bg(colors::surface_overlay())
        .text_color(colors::text_primary()).text_size(px(font_size::SM))
        .cursor_pointer().hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::CloseStylePanel);
            cx.notify();
        }))
        .child("Close");

    let clear_btn = div()
        .id("lsr-clear")
        .h(px(22.0)).px_2()
        .flex().items_center().justify_center()
        .rounded_md().bg(colors::surface_overlay())
        .text_color(colors::text_secondary()).text_size(px(font_size::SM))
        .cursor_pointer().hover(|s| s.bg(colors::tool_hover()))
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ClearLayerStyle(id));
            root.app.apply(Action::CloseStylePanel);
            cx.notify();
        }))
        .child("Clear All");

    div()
        .id("layer-style-panel")
        .w_full()
        .flex().flex_col()
        .bg(colors::surface_raised())
        .border_1().border_color(colors::surface_border())
        .rounded_md()
        .child(section_header("Layer Style"))
        .child(divider())
        .child(ds_toggle_row)
        .when_some(ds_params, |s, b| s.child(b))
        .child(divider())
        .child(og_toggle_row)
        .when_some(og_params, |s, b| s.child(b))
        .child(divider())
        .child(ig_toggle_row)
        .when_some(ig_params, |s, b| s.child(b))
        .child(divider())
        .child(be_toggle_row)
        .when_some(be_params, |s, b| s.child(b))
        .child(divider())
        .child(
            div().flex().flex_row().gap_2()
                .px(px(spacing::MD)).py(px(spacing::XS))
                .child(close_btn)
                .child(clear_btn),
        )
}

/// A labeled f32 stepper row: `−  LABEL  val  +`.
fn f32_stepper(
    key: &'static str,
    label: &'static str,
    val: f32,
    on_inc: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    on_dec: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex().flex_row().items_center().gap_2()
        .child(div().w(px(64.0)).text_color(colors::text_secondary()).text_size(px(font_size::XS)).child(label))
        .child(
            div()
                .id((key, 0u64))
                .w(px(18.0)).h(px(18.0))
                .flex().items_center().justify_center()
                .rounded_sm().bg(colors::surface_overlay())
                .text_color(colors::text_primary()).text_size(px(font_size::MD))
                .cursor_pointer().hover(|s| s.bg(colors::accent()))
                .on_click(on_dec)
                .child("−"),
        )
        .child(div().w(px(44.0)).text_color(colors::text_primary()).text_size(px(font_size::XS)).flex().justify_center().child(format!("{val:.1}")))
        .child(
            div()
                .id((key, 1u64))
                .w(px(18.0)).h(px(18.0))
                .flex().items_center().justify_center()
                .rounded_sm().bg(colors::surface_overlay())
                .text_color(colors::text_primary()).text_size(px(font_size::MD))
                .cursor_pointer().hover(|s| s.bg(colors::accent()))
                .on_click(on_inc)
                .child("+"),
        )
}
