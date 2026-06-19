//! Effects panel — lists the **selected** layer's effects, edits each effect's
//! scalar parameters as stepper rows, removes an effect, and adds one from an
//! Effects & Presets browser.
//!
//! Wave 8: each effect card now has an expand/collapse chevron (▶/▼) that
//! toggles the param rows inline. The expanded/collapsed state is stored in
//! `App::effects_expanded`. Cards start collapsed by default.

use gpui::{
    div, px, rgb, svg, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, Icon, section_header};

use crate::comp::filter_grouped;

use crate::app_state::{Action, App};
use crate::effect_params::{
    self, color_params, distort_params, generate_params, key_params, spatial_params,
    stylize_params, EffectStack, ScalarParam,
};
use crate::gpui_effects::{GpuiEffect, GpuiEffectKind};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// The six stacks in panel display order.
const STACKS: [(&str, EffectStack); 6] = [
    ("Color", EffectStack::Color),
    ("Spatial", EffectStack::Spatial),
    ("Distort", EffectStack::Distort),
    ("Stylize", EffectStack::Stylize),
    ("Keying", EffectStack::Keying),
    ("Generate", EffectStack::Generate),
];

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];

    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child(section_header("Effects")))
        .child(
            div()
                .id("fx-browser-toggle")
                .px_2()
                .py_1()
                .rounded_md()
                .bg(if app.effect_browser_open { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                .text_size(px(11.0))
                .cursor_pointer()
                .child(
                    svg()
                        .path(if app.effect_browser_open { Icon::Close.path() } else { Icon::Add.path() })
                        .w(px(12.0))
                        .h(px(12.0))
                        .text_color(colors::text_primary()),
                )
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ToggleEffectBrowser);
                    cx.notify();
                })),
        );

    // No selection → a hint.
    let Some(idx) = app.selected_layer.filter(|&i| comp.layers.get(i).is_some()) else {
        return div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(colors::surface_border())
            .child(header)
            .child(
                div()
                    .px_3()
                    .py_3()
                    .text_color(colors::text_secondary())
                    .text_size(px(11.0))
                    .child("Select a layer to edit its effects."),
            );
    };

    let layer = &comp.layers[idx];

    // One card per effect across every stack.
    let mut cards: Vec<gpui::AnyElement> = Vec::new();
    for (_stack_label, stack) in STACKS {
        match stack {
            EffectStack::Color => {
                for (ei, e) in layer.effects.iter().enumerate() {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), ei)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, ei, e.label(), color_params(e), expanded));
                }
            }
            EffectStack::Spatial => {
                for (ei, e) in layer.spatial_effects.iter().enumerate() {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), ei)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, ei, e.label(), spatial_params(e), expanded));
                }
            }
            EffectStack::Distort => {
                for (ei, e) in layer.distort_effects.iter().enumerate() {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), ei)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, ei, e.label(), distort_params(e), expanded));
                }
            }
            EffectStack::Stylize => {
                for (ei, e) in layer.stylize_effects.iter().enumerate() {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), ei)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, ei, e.label(), stylize_params(e), expanded));
                }
            }
            EffectStack::Keying => {
                for (ei, e) in layer.key_effects.iter().enumerate() {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), ei)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, ei, e.label(), key_params(e), expanded));
                }
            }
            EffectStack::Generate => {
                if let Some(e) = &layer.generate {
                    let expanded = *app.effects_expanded.get(&(stack_salt(stack), 0)).unwrap_or(&false);
                    cards.push(effect_card(cx, stack, 0, e.label(), generate_params(e), expanded));
                }
            }
        }
    }

    let body = if cards.is_empty() {
        div()
            .px_3()
            .py_3()
            .text_color(colors::text_secondary())
            .text_size(px(11.0))
            .child("No effects on this layer. Use + Add.")
            .into_any_element()
    } else {
        div().flex().flex_col().children(cards).into_any_element()
    };

    // GPUI-side post-process effects section
    let gpui_cards = gpui_effects_section(app, idx, cx);

    let mut panel = div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(layer.name.clone()),
        )
        .child(body)
        .child(gpui_cards);

    if app.effect_browser_open {
        panel = panel.child(browser(app, cx));
    }
    panel
}

/// The "Post-process" section: GPUI-side effects (Mosaic, Chroma, Vignette, Noise).
fn gpui_effects_section(app: &App, layer_idx: usize, cx: &mut Context<Pulse>) -> impl IntoElement {
    let effects = app.gpui_effects.get(&layer_idx).cloned().unwrap_or_default();

    let add_buttons: Vec<gpui::AnyElement> = [
        ("Mosaic", GpuiEffectKind::Mosaic),
        ("Chroma Aberration", GpuiEffectKind::ChromaticAberration),
        ("Vignette", GpuiEffectKind::Vignette),
        ("Noise/Grain", GpuiEffectKind::Noise),
        ("Color Balance", GpuiEffectKind::ColorBalance),
        ("Levels", GpuiEffectKind::Levels),
        ("Hue/Sat", GpuiEffectKind::HueSaturation),
        ("Fractal Noise", GpuiEffectKind::FractalNoise),
    ]
    .into_iter()
    .map(|(label, kind)| {
        div()
            .id(("gpui-fx-add", kind as usize))
            .px_2()
            .py_1()
            .mr_1()
            .rounded_md()
            .bg(colors::surface_raised())
            .text_color(colors::text_primary())
            .text_size(px(10.0))
            .cursor_pointer()
            .child(label)
            .on_click(cx.listener(move |root, _ev, _win, cx| {
                root.app.apply(Action::AddGpuiEffect(kind));
                cx.notify();
            }))
            .into_any_element()
    })
    .collect();

    let cards: Vec<gpui::AnyElement> = effects
        .iter()
        .enumerate()
        .map(|(ei, effect)| gpui_effect_card(cx, layer_idx, ei, effect, app))
        .collect();

    div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(colors::surface_border())
        .px_2()
        .py_1()
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .mb_1()
                .child("Post-process (GPUI)"),
        )
        .child(div().flex().flex_row().flex_wrap().children(add_buttons))
        .children(cards)
}

fn gpui_effect_card(
    cx: &mut Context<Pulse>,
    layer_idx: usize,
    ei: usize,
    effect: &GpuiEffect,
    app: &App,
) -> gpui::AnyElement {
    let expanded = *app.gpui_effects_expanded.get(&(layer_idx, ei)).unwrap_or(&false);
    let label = effect.label().to_string();

    let params: Vec<gpui::AnyElement> = if expanded {
        gpui_effect_params(cx, ei, effect)
    } else {
        Vec::new()
    };

    div()
        .flex()
        .flex_col()
        .mx_2()
        .my_1()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                .child(
                    div()
                        .id(("gpui-chevron", ei))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(if expanded { Icon::ChevronDown.path() } else { Icon::ChevronRight.path() })
                                .w(px(10.0))
                                .h(px(10.0))
                                .text_color(colors::text_secondary()),
                        )
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleGpuiEffectExpand(ei));
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(label),
                )
                .child(
                    div()
                        .id(("gpui-fx-remove", ei))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(rgb(BG_ACTIVE))
                        .cursor_pointer()
                        .child(svg().path(Icon::Remove.path()).w(px(12.0)).h(px(12.0)).text_color(colors::text_primary()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveGpuiEffect(ei));
                            cx.notify();
                        })),
                ),
        )
        .children(params)
        .into_any_element()
}

fn gpui_effect_params(cx: &mut Context<Pulse>, ei: usize, effect: &GpuiEffect) -> Vec<gpui::AnyElement> {
    match effect {
        GpuiEffect::Mosaic { block } => {
            let b = *block;
            vec![gpui_stepper_row(cx, "Block", format!("{b}px"),
                Action::SetMosaicBlock { effect_idx: ei, block: b.saturating_sub(1).max(1) },
                Action::SetMosaicBlock { effect_idx: ei, block: (b + 1).min(128) },
                ("gpui-p-dec", ei * 4 + 0), ("gpui-p-inc", ei * 4 + 0),
            )]
        }
        GpuiEffect::ChromaticAberration { offset } => {
            let o = *offset;
            vec![gpui_stepper_row(cx, "Offset", format!("{o}px"),
                Action::SetChromaOffset { effect_idx: ei, offset: o - 1 },
                Action::SetChromaOffset { effect_idx: ei, offset: o + 1 },
                ("gpui-p-dec", ei * 4 + 1), ("gpui-p-inc", ei * 4 + 1),
            )]
        }
        GpuiEffect::Vignette { intensity, radius } => {
            let i = *intensity;
            let r = *radius;
            vec![
                gpui_stepper_row(cx, "Intensity", format!("{i:.2}"),
                    Action::SetEffectIntensity { effect_idx: ei, intensity: (i - 0.05).max(0.0) },
                    Action::SetEffectIntensity { effect_idx: ei, intensity: (i + 0.05).min(1.0) },
                    ("gpui-p-dec", ei * 4 + 2), ("gpui-p-inc", ei * 4 + 2),
                ),
                gpui_stepper_row(cx, "Radius", format!("{r:.2}"),
                    Action::SetVignetteRadius { effect_idx: ei, radius: (r - 0.05).max(0.5) },
                    Action::SetVignetteRadius { effect_idx: ei, radius: (r + 0.05).min(1.0) },
                    ("gpui-p-dec", ei * 4 + 3), ("gpui-p-inc", ei * 4 + 3),
                ),
            ]
        }
        GpuiEffect::Noise { intensity } => {
            let i = *intensity;
            vec![gpui_stepper_row(cx, "Intensity", format!("{i:.2}"),
                Action::SetEffectIntensity { effect_idx: ei, intensity: (i - 0.05).max(0.0) },
                Action::SetEffectIntensity { effect_idx: ei, intensity: (i + 0.05).min(1.0) },
                ("gpui-p-dec", ei * 4 + 0), ("gpui-p-inc", ei * 4 + 0),
            )]
        }
        GpuiEffect::ColorBalance { shadows_r, shadows_g, shadows_b, midtones_r, midtones_g, midtones_b, highlights_r, highlights_g, highlights_b } => {
            let sr = *shadows_r; let sg = *shadows_g; let sb = *shadows_b;
            let mr = *midtones_r; let mg = *midtones_g; let mb = *midtones_b;
            let hr = *highlights_r; let hg = *highlights_g; let hb = *highlights_b;
            vec![
                gpui_stepper_row(cx, "Shad R", format!("{sr:.2}"),
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 0, value: sr - 0.05 },
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 0, value: sr + 0.05 },
                    ("gpui-p-dec", ei * 64 + 0), ("gpui-p-inc", ei * 64 + 0),
                ),
                gpui_stepper_row(cx, "Shad G", format!("{sg:.2}"),
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 1, value: sg - 0.05 },
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 1, value: sg + 0.05 },
                    ("gpui-p-dec", ei * 64 + 1), ("gpui-p-inc", ei * 64 + 1),
                ),
                gpui_stepper_row(cx, "Shad B", format!("{sb:.2}"),
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 2, value: sb - 0.05 },
                    Action::SetColorBalanceShadows { effect_idx: ei, channel: 2, value: sb + 0.05 },
                    ("gpui-p-dec", ei * 64 + 2), ("gpui-p-inc", ei * 64 + 2),
                ),
                gpui_stepper_row(cx, "Mid R", format!("{mr:.2}"),
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 0, value: mr - 0.05 },
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 0, value: mr + 0.05 },
                    ("gpui-p-dec", ei * 64 + 3), ("gpui-p-inc", ei * 64 + 3),
                ),
                gpui_stepper_row(cx, "Mid G", format!("{mg:.2}"),
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 1, value: mg - 0.05 },
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 1, value: mg + 0.05 },
                    ("gpui-p-dec", ei * 64 + 4), ("gpui-p-inc", ei * 64 + 4),
                ),
                gpui_stepper_row(cx, "Mid B", format!("{mb:.2}"),
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 2, value: mb - 0.05 },
                    Action::SetColorBalanceMidtones { effect_idx: ei, channel: 2, value: mb + 0.05 },
                    ("gpui-p-dec", ei * 64 + 5), ("gpui-p-inc", ei * 64 + 5),
                ),
                gpui_stepper_row(cx, "Hi R", format!("{hr:.2}"),
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 0, value: hr - 0.05 },
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 0, value: hr + 0.05 },
                    ("gpui-p-dec", ei * 64 + 6), ("gpui-p-inc", ei * 64 + 6),
                ),
                gpui_stepper_row(cx, "Hi G", format!("{hg:.2}"),
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 1, value: hg - 0.05 },
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 1, value: hg + 0.05 },
                    ("gpui-p-dec", ei * 64 + 7), ("gpui-p-inc", ei * 64 + 7),
                ),
                gpui_stepper_row(cx, "Hi B", format!("{hb:.2}"),
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 2, value: hb - 0.05 },
                    Action::SetColorBalanceHighlights { effect_idx: ei, channel: 2, value: hb + 0.05 },
                    ("gpui-p-dec", ei * 64 + 8), ("gpui-p-inc", ei * 64 + 8),
                ),
            ]
        }
        GpuiEffect::Levels { in_black, in_white, gamma, out_black, out_white } => {
            let ib = *in_black; let iw = *in_white; let g = *gamma; let ob = *out_black; let ow = *out_white;
            vec![
                gpui_stepper_row(cx, "In Black", format!("{ib}"),
                    Action::SetLevelsInBlack { effect_idx: ei, value: ib.saturating_sub(5) },
                    Action::SetLevelsInBlack { effect_idx: ei, value: ib.saturating_add(5) },
                    ("gpui-p-dec", ei * 64 + 10), ("gpui-p-inc", ei * 64 + 10),
                ),
                gpui_stepper_row(cx, "In White", format!("{iw}"),
                    Action::SetLevelsInWhite { effect_idx: ei, value: iw.saturating_sub(5) },
                    Action::SetLevelsInWhite { effect_idx: ei, value: iw.saturating_add(5) },
                    ("gpui-p-dec", ei * 64 + 11), ("gpui-p-inc", ei * 64 + 11),
                ),
                gpui_stepper_row(cx, "Gamma", format!("{g:.2}"),
                    Action::SetLevelsGamma { effect_idx: ei, value: (g - 0.1).max(0.01) },
                    Action::SetLevelsGamma { effect_idx: ei, value: g + 0.1 },
                    ("gpui-p-dec", ei * 64 + 12), ("gpui-p-inc", ei * 64 + 12),
                ),
                gpui_stepper_row(cx, "Out Black", format!("{ob}"),
                    Action::SetLevelsOutBlack { effect_idx: ei, value: ob.saturating_sub(5) },
                    Action::SetLevelsOutBlack { effect_idx: ei, value: ob.saturating_add(5) },
                    ("gpui-p-dec", ei * 64 + 13), ("gpui-p-inc", ei * 64 + 13),
                ),
                gpui_stepper_row(cx, "Out White", format!("{ow}"),
                    Action::SetLevelsOutWhite { effect_idx: ei, value: ow.saturating_sub(5) },
                    Action::SetLevelsOutWhite { effect_idx: ei, value: ow.saturating_add(5) },
                    ("gpui-p-dec", ei * 64 + 14), ("gpui-p-inc", ei * 64 + 14),
                ),
            ]
        }
        GpuiEffect::HueSaturation { hue_shift, saturation, lightness } => {
            let hs = *hue_shift; let sat = *saturation; let lit = *lightness;
            vec![
                gpui_stepper_row(cx, "Hue Shift", format!("{hs:.1}"),
                    Action::SetHueShift { effect_idx: ei, value: hs - 5.0 },
                    Action::SetHueShift { effect_idx: ei, value: hs + 5.0 },
                    ("gpui-p-dec", ei * 64 + 20), ("gpui-p-inc", ei * 64 + 20),
                ),
                gpui_stepper_row(cx, "Saturation", format!("{sat:.2}"),
                    Action::SetSaturation { effect_idx: ei, value: (sat - 0.1).max(0.0) },
                    Action::SetSaturation { effect_idx: ei, value: (sat + 0.1).min(2.0) },
                    ("gpui-p-dec", ei * 64 + 21), ("gpui-p-inc", ei * 64 + 21),
                ),
                gpui_stepper_row(cx, "Lightness", format!("{lit:.2}"),
                    Action::SetLightness { effect_idx: ei, value: (lit - 0.05).max(-1.0) },
                    Action::SetLightness { effect_idx: ei, value: (lit + 0.05).min(1.0) },
                    ("gpui-p-dec", ei * 64 + 22), ("gpui-p-inc", ei * 64 + 22),
                ),
            ]
        }
        GpuiEffect::FractalNoise { frequency, octaves, evolution } => {
            let freq = *frequency; let oct = *octaves; let evo = *evolution;
            vec![
                gpui_stepper_row(cx, "Frequency", format!("{freq:.3}"),
                    Action::SetNoiseFrequency { effect_idx: ei, value: (freq - 0.01).max(0.001) },
                    Action::SetNoiseFrequency { effect_idx: ei, value: freq + 0.01 },
                    ("gpui-p-dec", ei * 64 + 30), ("gpui-p-inc", ei * 64 + 30),
                ),
                gpui_stepper_row(cx, "Octaves", format!("{oct}"),
                    Action::SetNoiseFrequency { effect_idx: ei, value: freq },
                    Action::SetNoiseFrequency { effect_idx: ei, value: freq },
                    ("gpui-p-dec", ei * 64 + 31), ("gpui-p-inc", ei * 64 + 31),
                ),
                gpui_stepper_row(cx, "Evolution", format!("{evo:.2}"),
                    Action::SetNoiseEvolution { effect_idx: ei, value: evo - 0.1 },
                    Action::SetNoiseEvolution { effect_idx: ei, value: evo + 0.1 },
                    ("gpui-p-dec", ei * 64 + 32), ("gpui-p-inc", ei * 64 + 32),
                ),
            ]
        }
    }
}

fn gpui_stepper_row(
    cx: &mut Context<Pulse>,
    label: &'static str,
    value: String,
    dec: Action,
    inc: Action,
    dec_id: (&'static str, usize),
    inc_id: (&'static str, usize),
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(70.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(label))
        .child(
            div()
                .id(dec_id)
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
                .child("−")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(dec.clone());
                    cx.notify();
                })),
        )
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(10.0)).child(value))
        .child(
            div()
                .id(inc_id)
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
                .child("+")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(inc.clone());
                    cx.notify();
                })),
        )
        .into_any_element()
}

/// One effect card: chevron + label + stack tag + remove button.
/// When `expanded`, the param rows are shown inline below the header.
fn effect_card(
    cx: &mut Context<Pulse>,
    stack: EffectStack,
    ei: usize,
    label: &str,
    params: Vec<ScalarParam>,
    expanded: bool,
) -> gpui::AnyElement {
    let rows = if expanded {
        params
            .into_iter()
            .enumerate()
            .map(|(pi, sp)| param_row(cx, stack, ei, pi, sp))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    div()
        .flex()
        .flex_col()
        .mx_2()
        .my_1()
        .rounded_md()
        .bg(colors::surface_raised())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1()
                // Chevron toggle — expand / collapse this card's params.
                .child(
                    div()
                        .id(("fx-chevron", stack_salt(stack) * 256 + ei))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(if expanded { Icon::ChevronDown.path() } else { Icon::ChevronRight.path() })
                                .w(px(10.0))
                                .h(px(10.0))
                                .text_color(colors::text_secondary()),
                        )
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let key = (stack_salt(stack), ei);
                            let cur = *root.app.effects_expanded.get(&key).unwrap_or(&false);
                            root.app.effects_expanded.insert(key, !cur);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(9.0))
                        .child(stack.tag()),
                )
                .child(
                    // Remove this effect.
                    div()
                        .id(("fx-remove", stack_salt(stack) * 256 + ei))
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(rgb(BG_ACTIVE))
                        .cursor_pointer()
                        .child(svg().path(Icon::Trash.path()).w(px(12.0)).h(px(12.0)).text_color(colors::text_primary()))
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app
                                .apply(Action::RemoveEffect { stack, index: ei });
                            // Also clean up the expanded state for removed effects.
                            root.app.effects_expanded.remove(&(stack_salt(stack), ei));
                            cx.notify();
                        })),
                ),
        )
        .children(rows)
        .into_any_element()
}

/// One scalar-param stepper row.
fn param_row(
    cx: &mut Context<Pulse>,
    stack: EffectStack,
    ei: usize,
    pi: usize,
    sp: ScalarParam,
) -> gpui::AnyElement {
    let salt = (stack_salt(stack) * 256 + ei) * 64 + pi;
    let stepper = move |cx: &mut Context<Pulse>,
                        id: (&'static str, usize),
                        glyph: &'static str,
                        delta: f32| {
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
                if let Some(cur) = current_param(&root.app, stack, ei, pi) {
                    root.app.apply(Action::SetEffectParam {
                        stack,
                        index: ei,
                        param: pi,
                        value: cur + delta,
                    });
                    cx.notify();
                }
            }))
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(
            div()
                .w(px(70.0))
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child(sp.label),
        )
        .child(stepper(cx, ("fx-dec", salt), "−", -sp.step))
        .child(
            div()
                .flex_1()
                .text_color(colors::text_primary())
                .text_size(px(10.0))
                .child(format!("{:.2}", sp.value)),
        )
        .child(stepper(cx, ("fx-inc", salt), "+", sp.step))
        .into_any_element()
}

/// The Effects & Presets browser.
fn browser(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let groups = filter_grouped(&app.effect_query);

    let group_els = groups
        .into_iter()
        .map(|(cat, hits)| {
            let entries = hits
                .into_iter()
                .map(|hit| {
                    let entry = *hit.entry;
                    let name = entry.name;
                    div()
                        .id(("fx-add", name.as_ptr() as usize))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child(name)
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::AddEffect(entry));
                            cx.notify();
                        }))
                })
                .collect::<Vec<_>>();

            div()
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .child(
                    div()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child(cat.label()),
                )
                .children(entries)
        })
        .collect::<Vec<_>>();

    div()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(colors::surface_border())
        .bg(rgb(0x161617))
        .child(
            div()
                .px_3()
                .py_2()
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child("Effects & Presets"),
        )
        .children(group_els)
}

/// A stable per-stack salt for element ids.
fn stack_salt(stack: EffectStack) -> usize {
    match stack {
        EffectStack::Color => 0,
        EffectStack::Spatial => 1,
        EffectStack::Distort => 2,
        EffectStack::Stylize => 3,
        EffectStack::Keying => 4,
        EffectStack::Generate => 5,
    }
}

/// The current value of the selected layer's effect param `(stack, ei, pi)`.
fn current_param(app: &App, stack: EffectStack, ei: usize, pi: usize) -> Option<f32> {
    let ci = app.active_comp_index();
    let layer = app.project.comps[ci].layers.get(app.selected_layer?)?;
    let params = match stack {
        EffectStack::Color => effect_params::color_params(layer.effects.get(ei)?),
        EffectStack::Spatial => effect_params::spatial_params(layer.spatial_effects.get(ei)?),
        EffectStack::Distort => effect_params::distort_params(layer.distort_effects.get(ei)?),
        EffectStack::Stylize => effect_params::stylize_params(layer.stylize_effects.get(ei)?),
        EffectStack::Keying => effect_params::key_params(layer.key_effects.get(ei)?),
        EffectStack::Generate => effect_params::generate_params(layer.generate.as_ref()?),
    };
    params.get(pi).map(|sp| sp.value)
}
