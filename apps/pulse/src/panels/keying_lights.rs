//! Keying + Lights inspector — a floating panel with two sections:
//!
//! 1. **Keying** (per selected layer): quick "+ Add" chips for each
//!    [`KeyEffect`](crate::comp::KeyEffect) (Color / Luma / Chroma Key, Spill
//!    Suppression, Matte Choke) emitting `AddEffect` with `EffectStack::Keying`,
//!    a list of the layer's current keyers with their scalar params edited via
//!    `SetEffectParam` steppers, and a remove (×) per keyer (`RemoveEffect`).
//!
//! 2. **Comp 3D Lights**: "+ Add" chips for each [`LightKind`](crate::comp::LightKind)
//!    (Ambient / Point / Spot / Parallel) emitting `AddLight`, a list of the
//!    comp's lights with intensity / position-Z steppers that re-emit
//!    `UpdateLight`, and a remove (×) per light (`RemoveLight`).
//!
//! Reads `&App`, emits [`Action`](crate::app_state::Action)s. Gated by
//! `app.keylight_open` (toggled from the toolbar).

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use prism_ui::{colors, section_header};

use crate::app_state::{Action, App};
use crate::comp::{Light, LightKind, EFFECT_REGISTRY, EffectCategory};
use crate::effect_params::{self, EffectStack};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// The light kinds offered in the "add light" chips.
const LIGHT_KINDS: [LightKind; 4] = [
    LightKind::Ambient,
    LightKind::Point,
    LightKind::Spot,
    LightKind::Parallel,
];

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let header = div()
        .flex()
        .items_center()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child(div().flex_1().child(section_header("Keying & Lights")))
        .child(
            div()
                .id("kl-close")
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
                    root.app.keylight_open = false;
                    cx.notify();
                })),
        );

    div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(header)
        .child(keying_section(app, cx))
        .child(lights_section(app, cx))
}

/// The Keying section — per selected layer.
fn keying_section(app: &App, cx: &mut Context<Pulse>) -> gpui::AnyElement {
    let title = div()
        .px_3()
        .py_1()
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .child("Keying");

    let Some(idx) = app.selected_layer.filter(|&i| {
        app.project.comps[app.active_comp_index()].layers.get(i).is_some()
    }) else {
        return div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(colors::surface_border())
            .child(title)
            .child(
                div()
                    .px_3()
                    .py_1()
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child("Select a layer to add keyers."),
            )
            .into_any_element();
    };

    // "+ Add keyer" chips — one per keying BrowserEntry.
    let add_chips: Vec<gpui::AnyElement> = EFFECT_REGISTRY
        .iter()
        .filter(|e| e.category == EffectCategory::Keying)
        .enumerate()
        .map(|(i, entry)| {
            let entry = *entry;
            let name = entry.name;
            div()
                .id(("kl-add-keyer", i))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_size(px(9.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child(name)
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::AddEffect(entry));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    // Existing keyers on the layer.
    let ci = app.active_comp_index();
    let layer = &app.project.comps[ci].layers[idx];
    let keyer_cards: Vec<gpui::AnyElement> = layer
        .key_effects
        .iter()
        .enumerate()
        .map(|(ei, e)| keyer_card(cx, ei, e.label(), effect_params::key_params(e)))
        .collect();

    let mut section = div()
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(colors::surface_border())
        .child(title)
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(add_chips));

    if keyer_cards.is_empty() {
        section = section.child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("No keyers on this layer."),
        );
    } else {
        section = section.children(keyer_cards);
    }
    section.into_any_element()
}

/// One keyer card: label, remove, and a stepper per scalar param.
fn keyer_card(
    cx: &mut Context<Pulse>,
    ei: usize,
    label: &str,
    params: Vec<effect_params::ScalarParam>,
) -> gpui::AnyElement {
    let rows: Vec<gpui::AnyElement> = params
        .into_iter()
        .enumerate()
        .map(|(pi, sp)| keyer_param_row(cx, ei, pi, sp))
        .collect();

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
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .id(("kl-keyer-remove", ei))
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
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveEffect {
                                stack: EffectStack::Keying,
                                index: ei,
                            });
                            cx.notify();
                        })),
                ),
        )
        .children(rows)
        .into_any_element()
}

/// One keyer scalar-param stepper row (emits `SetEffectParam` with the new
/// absolute value derived from the current value ± step).
fn keyer_param_row(
    cx: &mut Context<Pulse>,
    ei: usize,
    pi: usize,
    sp: effect_params::ScalarParam,
) -> gpui::AnyElement {
    let salt = ei * 64 + pi;
    let dec_value = sp.value - sp.step;
    let inc_value = sp.value + sp.step;
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(80.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(sp.label))
        .child(key_step_btn(cx, ("kl-kp-dec", salt), "−", ei, pi, dec_value))
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(10.0)).child(format!("{:.2}", sp.value)))
        .child(key_step_btn(cx, ("kl-kp-inc", salt), "+", ei, pi, inc_value))
        .into_any_element()
}

/// A keyer-param stepper button.
fn key_step_btn(
    cx: &mut Context<Pulse>,
    id: (&'static str, usize),
    glyph: &'static str,
    ei: usize,
    pi: usize,
    value: f32,
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
            root.app.apply(Action::SetEffectParam {
                stack: EffectStack::Keying,
                index: ei,
                param: pi,
                value,
            });
            cx.notify();
        }))
}

/// The Comp 3D Lights section.
fn lights_section(app: &App, cx: &mut Context<Pulse>) -> gpui::AnyElement {
    let title = div()
        .px_3()
        .py_1()
        .text_color(colors::text_primary())
        .text_size(px(10.0))
        .child("Comp 3D Lights");

    // "+ Add light" chips.
    let add_chips: Vec<gpui::AnyElement> = LIGHT_KINDS
        .iter()
        .copied()
        .map(|kind| {
            let light = new_light(kind);
            div()
                .id(("kl-add-light", kind as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_overlay())
                .text_size(px(9.0))
                .text_color(colors::text_primary())
                .cursor_pointer()
                .child(kind.label())
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::AddLight(light));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

    let ci = app.active_comp_index();
    let lights = &app.project.comps[ci].lights;
    let light_cards: Vec<gpui::AnyElement> = lights
        .iter()
        .enumerate()
        .map(|(i, l)| light_card(cx, i, l))
        .collect();

    let mut section = div()
        .flex()
        .flex_col()
        .child(title)
        .child(div().flex().flex_wrap().gap_1().px_2().pb_1().children(add_chips));

    if light_cards.is_empty() {
        section = section.child(
            div()
                .px_3()
                .py_1()
                .text_color(colors::text_secondary())
                .text_size(px(9.0))
                .child("No lights in this comp."),
        );
    } else {
        section = section.children(light_cards);
    }
    section.into_any_element()
}

/// One light card: kind label, intensity & Z-position steppers, remove.
fn light_card(cx: &mut Context<Pulse>, i: usize, l: &Light) -> gpui::AnyElement {
    let intensity = l.intensity;
    let pos_z = l.position[2];
    let light_dec_i = with_intensity(l, (intensity - 0.1).max(0.0));
    let light_inc_i = with_intensity(l, intensity + 0.1);
    let light_dec_z = with_pos_z(l, pos_z - 100.0);
    let light_inc_z = with_pos_z(l, pos_z + 100.0);

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
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(format!("{} Light", l.kind.label())),
                )
                .child(
                    div()
                        .id(("kl-light-remove", i))
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
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::RemoveLight(i));
                            cx.notify();
                        })),
                ),
        )
        .child(light_stepper_row(
            cx,
            "Intensity",
            format!("{intensity:.2}"),
            i * 4,
            i,
            light_dec_i,
            light_inc_i,
        ))
        .child(light_stepper_row(
            cx,
            "Position Z",
            format!("{pos_z:.0}"),
            i * 4 + 1,
            i,
            light_dec_z,
            light_inc_z,
        ))
        .into_any_element()
}

/// A light-edit stepper row that re-emits `UpdateLight` with a mutated copy.
fn light_stepper_row(
    cx: &mut Context<Pulse>,
    label: &'static str,
    value: String,
    salt: usize,
    index: usize,
    dec_light: Light,
    inc_light: Light,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px_2()
        .py_1()
        .child(div().w(px(80.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(label))
        .child(light_step_btn(cx, ("kl-l-dec", salt), "−", index, dec_light))
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(10.0)).child(value))
        .child(light_step_btn(cx, ("kl-l-inc", salt), "+", index, inc_light))
        .into_any_element()
}

/// A light stepper button emitting `UpdateLight { index, light }`.
fn light_step_btn(
    cx: &mut Context<Pulse>,
    id: (&'static str, usize),
    glyph: &'static str,
    index: usize,
    light: Light,
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
            root.app.apply(Action::UpdateLight { index, light });
            cx.notify();
        }))
}

/// A freshly-defaulted light of the given kind for the "add light" chips.
fn new_light(kind: LightKind) -> Light {
    match kind {
        LightKind::Ambient => Light::ambient([1.0, 1.0, 1.0], 0.3),
        LightKind::Point => Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0),
        LightKind::Spot => Light::spot(
            [0.0, 0.0, -500.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            1.0,
            30.0,
            10.0,
        ),
        LightKind::Parallel => Light::parallel([0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0),
    }
}

/// A copy of `l` with a new intensity.
fn with_intensity(l: &Light, intensity: f32) -> Light {
    let mut c = *l;
    c.intensity = intensity;
    c
}

/// A copy of `l` with a new Z position.
fn with_pos_z(l: &Light, z: f32) -> Light {
    let mut c = *l;
    c.position[2] = z;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_light_seeds_each_kind() {
        assert_eq!(new_light(LightKind::Ambient).kind, LightKind::Ambient);
        assert_eq!(new_light(LightKind::Point).kind, LightKind::Point);
        assert_eq!(new_light(LightKind::Spot).kind, LightKind::Spot);
        assert_eq!(new_light(LightKind::Parallel).kind, LightKind::Parallel);
        // Ambient is a dim floor; the others default to full intensity.
        assert!((new_light(LightKind::Ambient).intensity - 0.3).abs() < 1e-6);
        assert!((new_light(LightKind::Point).intensity - 1.0).abs() < 1e-6);
    }

    #[test]
    fn light_mutation_helpers_only_touch_one_field() {
        let base = Light::point([10.0, 20.0, -500.0], [1.0, 1.0, 1.0], 1.0);
        let i = with_intensity(&base, 2.5);
        assert!((i.intensity - 2.5).abs() < 1e-6);
        assert_eq!(i.position, base.position, "intensity edit must not move the light");
        let z = with_pos_z(&base, -800.0);
        assert!((z.position[2] - (-800.0)).abs() < 1e-6);
        assert!((z.intensity - base.intensity).abs() < 1e-6, "z edit must not change intensity");
    }

    #[test]
    fn registry_exposes_five_keyers() {
        let count = EFFECT_REGISTRY
            .iter()
            .filter(|e| e.category == EffectCategory::Keying)
            .count();
        assert_eq!(count, 5, "Color/Luma/Chroma Key + Spill + Matte Choke");
    }
}
