//! Properties panel — edits the **selected** layer's transform at the playhead.
//!
//! Wave 8 additions:
//! - "Enable 3D" toggle per layer → `Action::SetLayer3D`.
//! - When 3D is on: Position Z stepper, Orientation X/Y/Z steppers.
//! - Expression icon ("=") badge on a property row when an expression is active.
//!
//! The standard 2D steppers (AnchorX/Y, X, Y, Scale, Rotation, Opacity) remain
//! exactly as before.

use gpui::{
    div, px, rgb, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use gpui::prelude::FluentBuilder;

use crate::comp::Prop;

use prism_ui::colors;

use crate::comp::{Light, LightKind};

use crate::app_state::{Action, App};
use crate::panels::BG_ACTIVE;
use crate::Pulse;

/// Accent for an animated property's stopwatch (matches the timeline diamonds).
const KEYFRAME: u32 = 0x37c8c0;
/// Accent for a property that has an expression active.
const EXPR_COLOR: u32 = 0xe5c07b;

/// The transform properties this panel edits (2-D), in After-Effects order.
const ROWS: [Prop; 7] = [
    Prop::AnchorX,
    Prop::AnchorY,
    Prop::X,
    Prop::Y,
    Prop::Scale,
    Prop::Rotation,
    Prop::Opacity,
];

fn step_for(prop: Prop) -> f32 {
    match prop {
        Prop::AnchorX | Prop::AnchorY | Prop::X | Prop::Y => 1.0,
        Prop::Rotation => 1.0,
        Prop::Scale => 0.05,
        Prop::Opacity => 0.05,
    }
}

fn fmt_value(prop: Prop, v: f32) -> String {
    let (_, suffix) = prop.range();
    match prop {
        Prop::AnchorX | Prop::AnchorY | Prop::X | Prop::Y | Prop::Rotation => {
            format!("{v:.1}{suffix}")
        }
        Prop::Scale | Prop::Opacity => format!("{v:.2}{suffix}"),
    }
}

/// A labelled `−` / value / `+` stepper row that dispatches a fixed [`Action`]
/// on each button (used by the comp Motion Blur section). The decrement /
/// increment actions are captured ready-made by the caller.
fn mb_stepper(
    cx: &mut Context<Pulse>,
    id: &'static str,
    label: &'static str,
    value: String,
    dec: Action,
    inc: Action,
) -> impl IntoElement {
    div()
        .flex().items_center().gap_2().px_3().py_1()
        .child(div().w(px(78.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child(label))
        .child(
            div()
                .id((id, 0usize))
                .w(px(22.0)).h(px(20.0)).flex().items_center().justify_center()
                .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary())
                .text_size(px(13.0)).cursor_pointer().child("−")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(dec.clone());
                    cx.notify();
                })),
        )
        .child(div().flex_1().text_color(colors::text_primary()).text_size(px(11.0)).child(value))
        .child(
            div()
                .id((id, 1usize))
                .w(px(22.0)).h(px(20.0)).flex().items_center().justify_center()
                .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary())
                .text_size(px(13.0)).cursor_pointer().child("+")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(inc.clone());
                    cx.notify();
                })),
        )
}

pub fn render(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    let time = app.time;

    let header = div()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(colors::surface_border())
        .text_color(colors::text_primary())
        .child("Transform");

    // No selection → show Camera + Lights sections instead.
    let Some(idx) = app.selected_layer.filter(|&i| comp.layers.get(i).is_some()) else {
        let cam = &comp.camera;
        let cam_pos = cam.position;
        let cam_fov = cam.fov_deg;
        // Comp motion-blur (shutter) settings for the Motion Blur section below.
        let mb = comp.motion_blur;

        // Pre-build light rows before the div chain.
        let lights = comp.lights.clone();
        let light_rows: Vec<gpui::AnyElement> = lights
            .iter()
            .enumerate()
            .map(|(i, light)| {
                let kind_label = light.kind.label();
                let intensity = light.intensity;
                let light_clone = *light;
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1()
                    .child(
                        div()
                            .w(px(12.0))
                            .h(px(12.0))
                            .rounded_full()
                            .bg(rgb(EXPR_COLOR)),
                    )
                    .child(
                        div()
                            .w(px(52.0))
                            .text_color(colors::text_primary())
                            .text_size(px(10.0))
                            .child(kind_label),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_color(colors::text_secondary())
                            .text_size(px(10.0))
                            .child(format!("{:.2}", intensity)),
                    )
                    // Intensity −
                    .child(
                        div()
                            .id(("light-int-dec", i))
                            .w(px(18.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_md()
                            .bg(colors::surface_raised())
                            .text_color(colors::text_primary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child("−")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut l = light_clone;
                                l.intensity = (l.intensity - 0.1).max(0.0);
                                root.app.apply(Action::UpdateLight { index: i, light: l });
                                cx.notify();
                            })),
                    )
                    // Intensity +
                    .child(
                        div()
                            .id(("light-int-inc", i))
                            .w(px(18.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_md()
                            .bg(colors::surface_raised())
                            .text_color(colors::text_primary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child("+")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                let mut l = light_clone;
                                l.intensity = l.intensity + 0.1;
                                root.app.apply(Action::UpdateLight { index: i, light: l });
                                cx.notify();
                            })),
                    )
                    // Remove
                    .child(
                        div()
                            .id(("light-remove", i))
                            .w(px(16.0)).h(px(16.0))
                            .flex().items_center().justify_center()
                            .rounded_md()
                            .bg(rgb(BG_ACTIVE))
                            .text_color(colors::text_primary())
                            .text_size(px(11.0))
                            .cursor_pointer()
                            .child("×")
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::RemoveLight(i));
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        // "Add Light" buttons for each kind.
        let add_light_btns: Vec<gpui::AnyElement> = [
            LightKind::Ambient,
            LightKind::Point,
            LightKind::Parallel,
            LightKind::Spot,
        ]
        .iter()
        .copied()
        .map(|kind| {
            div()
                .id(("light-add", kind as usize))
                .px_2()
                .py_1()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .cursor_pointer()
                .child(format!("+ {}", kind.label()))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let light = match kind {
                        LightKind::Ambient => Light::ambient([1.0, 1.0, 1.0], 0.5),
                        LightKind::Point => Light::point([0.0, 0.0, -500.0], [1.0, 1.0, 1.0], 1.0),
                        LightKind::Parallel => Light::parallel([0.0, 0.0, 1.0], [1.0, 1.0, 1.0], 1.0),
                        LightKind::Spot => Light::spot(
                            [0.0, 0.0, -500.0],
                            [0.0, 0.0, 1.0],
                            [1.0, 1.0, 1.0],
                            1.0,
                            30.0,
                            10.0,
                        ),
                    };
                    root.app.apply(Action::AddLight(light));
                    cx.notify();
                }))
                .into_any_element()
        })
        .collect();

        let has_lights = !light_rows.is_empty();

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
                    .child("Select a layer to edit its transform."),
            )
            // Camera section header
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .child("Camera"),
            )
            // Camera position X
            .child(
                div()
                    .flex().items_center().gap_2().px_3().py_1()
                    .child(div().w(px(60.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("X"))
                    .child(div().id("cam-x-dec").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("−")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[0] -= 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        })))
                    .child(div().flex_1().text_color(colors::text_primary()).text_size(px(11.0)).child(format!("{:.0}", cam_pos[0])))
                    .child(div().id("cam-x-inc").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("+")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[0] += 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        }))),
            )
            // Camera position Y
            .child(
                div()
                    .flex().items_center().gap_2().px_3().py_1()
                    .child(div().w(px(60.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Y"))
                    .child(div().id("cam-y-dec").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("−")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[1] -= 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        })))
                    .child(div().flex_1().text_color(colors::text_primary()).text_size(px(11.0)).child(format!("{:.0}", cam_pos[1])))
                    .child(div().id("cam-y-inc").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("+")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[1] += 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        }))),
            )
            // Camera position Z
            .child(
                div()
                    .flex().items_center().gap_2().px_3().py_1()
                    .child(div().w(px(60.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Z"))
                    .child(div().id("cam-z-dec").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("−")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[2] -= 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        })))
                    .child(div().flex_1().text_color(colors::text_primary()).text_size(px(11.0)).child(format!("{:.0}", cam_pos[2])))
                    .child(div().id("cam-z-inc").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("+")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let mut p = root.app.project.comps[ci].camera.position;
                            p[2] += 50.0;
                            root.app.apply(Action::SetCameraPosition(p)); cx.notify();
                        }))),
            )
            // Camera FOV
            .child(
                div()
                    .flex().items_center().gap_2().px_3().py_1()
                    .child(div().w(px(60.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("FOV°"))
                    .child(div().id("cam-fov-dec").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("−")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let fov = root.app.project.comps[ci].camera.fov_deg;
                            root.app.apply(Action::SetCameraFov(fov - 5.0)); cx.notify();
                        })))
                    .child(div().flex_1().text_color(colors::text_primary()).text_size(px(11.0)).child(format!("{:.0}°", cam_fov)))
                    .child(div().id("cam-fov-inc").w(px(20.0)).h(px(18.0)).flex().items_center().justify_center()
                        .rounded_md().bg(colors::surface_raised()).text_color(colors::text_primary()).text_size(px(12.0)).cursor_pointer().child("+")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let fov = root.app.project.comps[ci].camera.fov_deg;
                            root.app.apply(Action::SetCameraFov(fov + 5.0)); cx.notify();
                        }))),
            )
            // --- Lights section ---
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .child("Lights"),
            )
            .when(has_lights, |d| d.children(light_rows))
            .when(!has_lights, |d| {
                d.child(
                    div()
                        .px_3()
                        .py_1()
                        .text_color(colors::text_secondary())
                        .text_size(px(10.0))
                        .child("No lights. Add one:"),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .children(add_light_btns),
            )
            // --- Motion Blur (comp shutter) section ---
            .child(
                div()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(colors::surface_border())
                    .text_color(colors::text_primary())
                    .text_size(px(11.0))
                    .child("Motion Blur"),
            )
            // Enable toggle
            .child(
                div()
                    .flex().items_center().gap_2().px_3().py_1()
                    .child(div().w(px(78.0)).text_color(colors::text_secondary()).text_size(px(10.0)).child("Enable"))
                    .child(
                        div()
                            .id("mb-enable")
                            .px_2().py_1().rounded_md().cursor_pointer().text_size(px(10.0))
                            .bg(if mb.enabled { rgb(EXPR_COLOR) } else { colors::surface_raised() })
                            .text_color(colors::text_primary())
                            .child(if mb.enabled { "ON" } else { "off" })
                            .on_click(cx.listener(move |root, _ev, _win, cx| {
                                root.app.apply(Action::SetMotionBlurEnabled(!mb.enabled));
                                cx.notify();
                            })),
                    ),
            )
            // Shutter angle
            .child(mb_stepper(cx, "mb-angle", "Shutter°", format!("{:.0}°", mb.angle),
                Action::SetMotionBlurAngle(mb.angle - 15.0), Action::SetMotionBlurAngle(mb.angle + 15.0)))
            // Shutter phase
            .child(mb_stepper(cx, "mb-phase", "Phase°", format!("{:.0}°", mb.phase),
                Action::SetMotionBlurPhase(mb.phase - 15.0), Action::SetMotionBlurPhase(mb.phase + 15.0)))
            // Samples
            .child(mb_stepper(cx, "mb-samples", "Samples", format!("{}", mb.samples),
                Action::SetMotionBlurSamples(mb.samples.saturating_sub(2)), Action::SetMotionBlurSamples(mb.samples + 2)));
    };

    let layer = &comp.layers[idx];
    let layer_name = layer.name.clone();
    let is_3d = layer.threed;
    let layer_mb = layer.motion_blur;
    let comp_mb_on = comp.motion_blur.enabled;
    let time_remap_enabled = layer.time_remap.enabled;
    let time_remap_active = layer.time_remap.is_active();
    let time_remap_source = if time_remap_active {
        layer.time_remap.track.sample(time, time as f32)
    } else {
        time
    };

    // ----- 2-D standard rows -----
    let rows_2d = ROWS
        .into_iter()
        .map(|prop| {
            let value = comp.layer_value(idx, prop, time);
            let keyed = comp.layers[idx].track(prop).keys.len();
            let step = step_for(prop);
            let pi = Prop::ALL.iter().position(|p| *p == prop).unwrap_or(0);
            // Check if this prop has an expression.
            let prop_name = format!("{:?}", prop);
            let has_expr = app.expressions.contains_key(&(idx, prop_name.clone()));

            let stepper = |id: (&'static str, usize), label: &'static str, delta: f32| {
                div()
                    .id(id)
                    .w(px(22.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(colors::surface_raised())
                    .text_color(colors::text_primary())
                    .text_size(px(13.0))
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |root, _ev, _win, cx| {
                        let app = &root.app;
                        let ci = app.active_comp_index();
                        if let Some(li) = app.selected_layer {
                            let cur = app.project.comps[ci].layer_value(li, prop, app.time);
                            root.app.apply(Action::SetTransform(prop, cur + delta));
                            cx.notify();
                        }
                    }))
            };

            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .child(
                    div()
                        .w(px(78.0))
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(prop.label()),
                )
                .child(stepper(("prop-dec", pi), "−", -step))
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(fmt_value(prop, value)),
                )
                .child(stepper(("prop-inc", pi), "+", step))
                .child(
                    // Stopwatch diamond.
                    div()
                        .id(("prop-kf", pi))
                        .flex()
                        .items_center()
                        .gap_1()
                        .cursor_pointer()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(if keyed > 0 {
                                    rgb(KEYFRAME)
                                } else {
                                    colors::text_secondary()
                                })
                                .child(if keyed > 0 { "◆" } else { "◇" }),
                        )
                        .child(
                            div()
                                .w(px(10.0))
                                .text_color(colors::text_secondary())
                                .text_size(px(10.0))
                                .child(if keyed > 0 {
                                    format!("{keyed}")
                                } else {
                                    String::new()
                                }),
                        )
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleKeyframe(prop));
                            cx.notify();
                        })),
                )
                // Expression badge: "=" in amber when an expression is active.
                .when(has_expr, |d| {
                    d.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(EXPR_COLOR))
                            .child("="),
                    )
                })
        })
        .collect::<Vec<_>>();

    // ----- Enable 3D toggle -----
    let threed_toggle = div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .child(
            div()
                .w(px(78.0))
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child("Enable 3D"),
        )
        .child(
            div()
                .id(("prop-3d-toggle", idx))
                .px_2()
                .py(px(1.0))
                .rounded_md()
                .bg(if is_3d { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                .text_color(if is_3d { rgb(0x37c8c0_u32) } else { colors::text_primary() })
                .text_size(px(11.0))
                .cursor_pointer()
                .child(if is_3d { "3D ✓" } else { "3D" })
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let cur = root.app.project.comps[root.app.active_comp_index()]
                        .layers
                        .get(idx)
                        .map(|l| l.threed)
                        .unwrap_or(false);
                    root.app.apply(Action::SetLayer3D(idx, !cur));
                    cx.notify();
                })),
        );

    // ----- 3D rows (only shown when 3D is on) -----
    // Z position stepper.
    let z_val = layer.z.sample(time, 0.0);
    let z_row = div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .child(
            div()
                .w(px(78.0))
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child("Position Z"),
        )
        .child(
            div()
                .id(("prop-z-dec", idx))
                .w(px(22.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(colors::text_primary())
                .text_size(px(13.0))
                .cursor_pointer()
                .child("−")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let ci = root.app.active_comp_index();
                    let cur = root.app.project.comps[ci].layers.get(idx).map(|l| l.z.sample(root.app.time, 0.0)).unwrap_or(0.0);
                    root.app.apply(Action::SetPositionZ(idx, cur - 10.0));
                    cx.notify();
                })),
        )
        .child(
            div()
                .flex_1()
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child(format!("{z_val:.1}px")),
        )
        .child(
            div()
                .id(("prop-z-inc", idx))
                .w(px(22.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .bg(colors::surface_raised())
                .text_color(colors::text_primary())
                .text_size(px(13.0))
                .cursor_pointer()
                .child("+")
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    let ci = root.app.active_comp_index();
                    let cur = root.app.project.comps[ci].layers.get(idx).map(|l| l.z.sample(root.app.time, 0.0)).unwrap_or(0.0);
                    root.app.apply(Action::SetPositionZ(idx, cur + 10.0));
                    cx.notify();
                })),
        );

    // Orientation X/Y/Z steppers (one per axis).
    let orient_rows = ["OrientX", "OrientY", "OrientZ"]
        .into_iter()
        .enumerate()
        .map(|(axis, label)| {
            let cur_val = match axis {
                0 => layer.orient_x.sample(time, 0.0),
                1 => layer.orient_y.sample(time, 0.0),
                _ => layer.orient_z.sample(time, 0.0),
            };
            let display = match axis { 0 => "Orient X", 1 => "Orient Y", _ => "Orient Z" };
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .child(
                    div()
                        .w(px(78.0))
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(display),
                )
                .child(
                    div()
                        .id((label, idx * 10 + axis))
                        .w(px(22.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(13.0))
                        .cursor_pointer()
                        .child("−")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let layer = root.app.project.comps[ci].layers.get(idx);
                            if let Some(l) = layer {
                                let t = root.app.time;
                                let (rx, ry, rz) = (
                                    l.orient_x.sample(t, 0.0),
                                    l.orient_y.sample(t, 0.0),
                                    l.orient_z.sample(t, 0.0),
                                );
                                let (rx, ry, rz) = match axis {
                                    0 => (rx - 15.0, ry, rz),
                                    1 => (rx, ry - 15.0, rz),
                                    _ => (rx, ry, rz - 15.0),
                                };
                                root.app.apply(Action::Set3DRotation(idx, rx, ry, rz));
                                cx.notify();
                            }
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_color(colors::text_primary())
                        .text_size(px(11.0))
                        .child(format!("{cur_val:.1}°")),
                )
                .child(
                    div()
                        .id((label, idx * 10 + axis + 100))
                        .w(px(22.0))
                        .h(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .bg(colors::surface_raised())
                        .text_color(colors::text_primary())
                        .text_size(px(13.0))
                        .cursor_pointer()
                        .child("+")
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            let ci = root.app.active_comp_index();
                            let layer = root.app.project.comps[ci].layers.get(idx);
                            if let Some(l) = layer {
                                let t = root.app.time;
                                let (rx, ry, rz) = (
                                    l.orient_x.sample(t, 0.0),
                                    l.orient_y.sample(t, 0.0),
                                    l.orient_z.sample(t, 0.0),
                                );
                                let (rx, ry, rz) = match axis {
                                    0 => (rx + 15.0, ry, rz),
                                    1 => (rx, ry + 15.0, rz),
                                    _ => (rx, ry, rz + 15.0),
                                };
                                root.app.apply(Action::Set3DRotation(idx, rx, ry, rz));
                                cx.notify();
                            }
                        })),
                )
        })
        .collect::<Vec<_>>();

    // ----- Parent dropdown row -----
    // Build the "None" button + one button per other layer. Pre-build the vec
    // before the panel div chain so closures don't need to re-borrow cx.
    let n_layers = comp.layers.len();
    let current_parent = comp.layers.get(idx).and_then(|l| l.parent);
    // "None" clear-parent button.
    let none_btn = div()
        .id(("parent-none", idx))
        .px_2()
        .py(px(1.0))
        .rounded_md()
        .bg(if current_parent.is_none() { rgb(BG_ACTIVE) } else { colors::surface_raised() })
        .text_color(if current_parent.is_none() { rgb(0x37c8c0_u32) } else { colors::text_secondary() })
        .text_size(px(10.0))
        .cursor_pointer()
        .child("None")
        .on_click(cx.listener(move |root, _ev, _win, cx| {
            root.app.apply(Action::ClearParent(idx));
            cx.notify();
        }));
    // One button per other layer (can't be self).
    let layer_names: Vec<(usize, String)> = (0..n_layers)
        .filter(|&i| i != idx)
        .map(|i| (i, comp.layers[i].name.clone()))
        .collect();
    let parent_btns: Vec<_> = layer_names
        .into_iter()
        .map(|(pi, pname)| {
            let is_active = current_parent == Some(pi);
            div()
                .id(("parent-layer", idx * 256 + pi))
                .px_2()
                .py(px(1.0))
                .rounded_md()
                .bg(if is_active { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                .text_color(if is_active { rgb(0x37c8c0_u32) } else { colors::text_secondary() })
                .text_size(px(10.0))
                .cursor_pointer()
                .child(pname)
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetParent(idx, pi));
                    cx.notify();
                }))
        })
        .collect();
    let parent_row = div()
        .flex()
        .items_center()
        .flex_wrap()
        .gap_1()
        .px_3()
        .py_1()
        .child(
            div()
                .w(px(48.0))
                .text_color(colors::text_secondary())
                .text_size(px(10.0))
                .child("Parent"),
        )
        .child(none_btn)
        .children(parent_btns);

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
                .child(layer_name),
        )
        .children(rows_2d)
        .child(threed_toggle)
        // ----- Per-layer Motion Blur toggle -----
        .child(
            div()
                .flex().items_center().gap_2().px_3().py_1()
                .child(
                    div().w(px(78.0)).text_color(colors::text_primary()).text_size(px(11.0))
                        .child("Motion Blur"),
                )
                .child(
                    div()
                        .id(("prop-mb-toggle", idx))
                        .px_2().py(px(1.0)).rounded_md().cursor_pointer().text_size(px(11.0))
                        .bg(if layer_mb { rgb(BG_ACTIVE) } else { colors::surface_raised() })
                        .text_color(if layer_mb { rgb(0x37c8c0_u32) } else { colors::text_primary() })
                        .child(if layer_mb { "MB ✓" } else { "MB" })
                        .on_click(cx.listener(move |root, _ev, _win, cx| {
                            root.app.apply(Action::ToggleLayerMotionBlur(idx));
                            cx.notify();
                        })),
                )
                // Hint that the comp master switch must also be on.
                .when(layer_mb && !comp_mb_on, |d| {
                    d.child(
                        div().text_color(colors::text_secondary()).text_size(px(9.0))
                            .child("(comp MB off)"),
                    )
                }),
        );

    // Time Remap section
    let time_remap_header = div()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .border_t_1()
        .border_color(colors::surface_border())
        .child(
            div()
                .w(px(78.0))
                .text_color(colors::text_primary())
                .text_size(px(11.0))
                .child("Time Remap"),
        )
        .child(
            div()
                .id(("time-remap-toggle", idx))
                .w(px(22.0))
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .bg(if time_remap_enabled { rgb(KEYFRAME) } else { colors::surface_raised() })
                .text_color(if time_remap_enabled { gpui::rgb(0x000000u32) } else { colors::text_secondary() })
                .text_size(px(9.0))
                .cursor_pointer()
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::SetTimeRemapEnabled {
                        layer_id: idx,
                        enabled: !root.app.project.comps[root.app.active_comp_index()]
                            .layers.get(idx)
                            .map(|l| l.time_remap.enabled)
                            .unwrap_or(false),
                    });
                    cx.notify();
                }))
                .child(if time_remap_enabled { "ON" } else { "off" }),
        );

    let time_remap_source_row = time_remap_active.then(|| {
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(
                div()
                    .w(px(78.0))
                    .text_color(colors::text_secondary())
                    .text_size(px(10.0))
                    .child("Source Time"),
            )
            .child(
                div()
                    .text_color(gpui::rgb(KEYFRAME))
                    .text_size(px(11.0))
                    .child(format!("{:.3}s", time_remap_source)),
            )
    });

    if is_3d {
        panel = panel
            .child(z_row)
            .children(orient_rows);
    }

    panel
        .child(time_remap_header)
        .children(time_remap_source_row)
        .child(parent_row)
}
