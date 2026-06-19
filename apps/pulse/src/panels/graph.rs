//! The graph (value-curve) editor — an After-Effects-style plot of the selected
//! layer's animated transform properties, shown on the timeline in place of the
//! keyframe lanes when toggled on.
//!
//! Where the [`timeline`](super::timeline) lanes show keyframes as diamonds on a
//! time track, the graph plots each property as a curve of **value over time**
//! and lets you shape the motion directly:
//!
//! - **drag a keyframe dot** to retime (x) and revalue (y) it
//!   ([`Action::MoveKeyframeXY`]);
//! - **drag a Bézier ease handle** to shape the segment leaving / arriving at a
//!   key — the same `(out_x, out_y, in_x, in_y)` control points the engine's
//!   sampler evaluates ([`Action::SetInterp`]). Dragging a handle on a
//!   Linear/Hold segment promotes it to an editable [`Ease`] seeded at the
//!   straight diagonal (so the conversion is value-neutral), then reshapes it.
//!
//! This is the GPUI port of `crate::graph::show`: the same coordinate maps,
//! hit model, and drag math, reusing the engine's [`Track`]/[`Ease`]/[`Interp`]
//! types directly (no fork). The interaction is pointer-driven through gpui
//! `canvas` painting + mouse listeners, mirroring the timeline panel's style.

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, point, px, rgb, Context, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Path, StatefulInteractiveElement, Styled,
};

use crate::comp::{Comp, Ease, Handle, Interp, Prop, Track};

use prism_ui::colors;

use crate::app_state::{Action, App, GraphGrab};
use crate::Pulse;

/// Pixels reserved on the left for the value-axis labels.
const AXIS_W: f32 = 40.0;
/// Top/bottom padding inside the plot so curves and handles don't clip.
const PAD_Y: f32 = 14.0;
/// Hit radius (screen px) for grabbing a keyframe or handle.
const GRAB_R: f32 = 8.0;
/// Plot background.
const PLOT_BG: u32 = 0x141414;
/// Subtle gridline color.
const GRID: u32 = 0x2c2c2c;
/// Playhead guide color (teal, matching the timeline).
const PLAYHEAD: u32 = 0x37c8c0;

/// Per-property plot color (matches the egui graph editor's palette).
fn prop_color(prop: Prop) -> u32 {
    match prop {
        Prop::AnchorX => 0xD06B9C,
        Prop::AnchorY => 0x4EC2C2,
        Prop::X => 0xE56B6B,
        Prop::Y => 0x6BC27A,
        Prop::Scale => 0x4E9BE6,
        Prop::Rotation => 0xE6A13C,
        Prop::Opacity => 0xB87BE6,
    }
}

/// The plot's coordinate frame: the value/time axis bounds and the screen rect
/// (window-relative px) the curves map into. Built once per frame from the laid-
/// out canvas bounds and shared by the painter + the mouse hit-tests.
#[derive(Clone, Copy)]
struct Frame {
    /// Plot rect in window px (after the axis gutter + padding).
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    dur: f32,
    vlo: f32,
    vhi: f32,
}

impl Frame {
    fn t_to_x(&self, t: f32) -> f32 {
        let pw = (self.right - self.left).max(1.0);
        self.left + (t / self.dur).clamp(0.0, 1.0) * pw
    }
    fn x_to_t(&self, x: f32) -> f32 {
        let pw = (self.right - self.left).max(1.0);
        ((x - self.left) / pw).clamp(0.0, 1.0) * self.dur
    }
    fn v_to_y(&self, v: f32) -> f32 {
        let ph = (self.bottom - self.top).max(1.0);
        self.bottom - ((v - self.vlo) / (self.vhi - self.vlo)) * ph
    }
    fn y_to_v(&self, y: f32) -> f32 {
        let ph = (self.bottom - self.top).max(1.0);
        self.vlo + ((self.bottom - y) / ph) * (self.vhi - self.vlo)
    }
}

/// Which transform properties the graph plots for `app`'s selected layer:
/// the explicit `graph_shown` selection, else every property that has ≥1
/// keyframe. Empty when nothing is selected / keyed.
fn plotted_props(app: &App) -> Vec<Prop> {
    let Some(idx) = app.selected_layer else {
        return Vec::new();
    };
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    if idx >= comp.layers.len() {
        return Vec::new();
    }
    if app.graph_shown.is_empty() {
        Prop::ALL
            .into_iter()
            .filter(|&p| !comp.layers[idx].track(p).keys.is_empty())
            .collect()
    } else {
        app.graph_shown.clone()
    }
}

/// Compute the shared value-axis bounds across the plotted tracks (with headroom),
/// or `None` when no track has keyframes.
fn value_axis(comp: &Comp, idx: usize, props: &[Prop]) -> Option<(f32, f32)> {
    let (mut vlo, mut vhi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &p in props {
        if let Some((lo, hi)) = comp.layers[idx].track(p).value_bounds() {
            vlo = vlo.min(lo);
            vhi = vhi.max(hi);
        }
    }
    if !vlo.is_finite() || !vhi.is_finite() {
        return None;
    }
    if (vhi - vlo).abs() < 1e-3 {
        vlo -= 1.0;
        vhi += 1.0;
    }
    let pad = (vhi - vlo) * 0.08;
    Some((vlo - pad, vhi + pad))
}

/// Screen positions (window px) of a segment's two ease handles, mapped from the
/// normalized curve space onto the `[a, b]` time/value rectangle.
fn handle_screen_pos(e: Ease, a: (f32, f32), b: (f32, f32), f: &Frame) -> ((f32, f32), (f32, f32)) {
    let (at, av) = a;
    let (bt, bv) = b;
    let lerp = |s: f32, lo: f32, hi: f32| lo + (hi - lo) * s;
    let out = (
        f.t_to_x(lerp(e.out_x, at, bt)),
        f.v_to_y(lerp(e.out_y, av, bv)),
    );
    let inp = (
        f.t_to_x(lerp(e.in_x, at, bt)),
        f.v_to_y(lerp(e.in_y, av, bv)),
    );
    (out, inp)
}

/// The graph editor element for the timeline's plot area (the property chips are
/// rendered by the timeline header). Returns a hint when nothing is plottable.
pub fn render(app: &App, cx: &mut Context<Pulse>) -> gpui::AnyElement {
    use gpui::IntoElement as _;
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    let dur = comp.duration.max(0.001);
    let time = app.time;

    let Some(idx) = app.selected_layer.filter(|&i| comp.layers.get(i).is_some()) else {
        return hint("Select a layer to edit its value curves.").into_any_element();
    };
    let props = plotted_props(app);
    let Some((vlo, vhi)) = value_axis(comp, idx, &props) else {
        return hint("This layer has no keyframes yet. Keyframe a property to graph it.")
            .into_any_element();
    };

    // Snapshot the data the painter needs (it can't borrow `app`).
    let layer = comp.layers[idx].clone();
    let plotted = props.clone();
    let grab = app.graph_grab;
    let graph_rect = app.graph_rect.clone();

    // The plot canvas: records its bounds (for hit-testing), then paints grid,
    // curves, keys, ease handles, and the playhead.
    let plot = canvas(
        move |bounds, _win, _cx| {
            graph_rect.set(Some(bounds));
        },
        move |bounds, _state, window, _cx| {
            let frame = Frame {
                left: f32::from(bounds.origin.x) + AXIS_W,
                top: f32::from(bounds.origin.y) + PAD_Y,
                right: f32::from(bounds.origin.x) + f32::from(bounds.size.width) - 8.0,
                bottom: f32::from(bounds.origin.y) + f32::from(bounds.size.height) - PAD_Y,
                dur,
                vlo,
                vhi,
            };
            paint(window, &layer, &plotted, &frame, time, grab);
        },
    )
    .size_full();

    // Mouse interaction over the plot: arm a grab on press, drag it, drop on up.
    div()
        .id("graph-plot")
        .flex_1()
        .relative()
        .bg(rgb(PLOT_BG))
        .child(plot)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |root, ev: &gpui::MouseDownEvent, _win, cx| {
                let pos = (f32::from(ev.position.x), f32::from(ev.position.y));
                if let Some(frame) = root.app.graph_frame() {
                    if let Some(g) = root.app.graph_pick(pos, &frame) {
                        root.app.graph_grab = Some(g);
                        root.app.apply(Action::Pause);
                    } else if in_plot(pos, &frame) {
                        // Empty-area press scrubs the playhead.
                        root.app.apply(Action::SetTime(frame.x_to_t(pos.0)));
                    }
                    cx.notify();
                }
            }),
        )
        .on_mouse_move(cx.listener(move |root, ev: &gpui::MouseMoveEvent, _win, cx| {
            if root.app.graph_grab.is_none() {
                return;
            }
            let pos = (f32::from(ev.position.x), f32::from(ev.position.y));
            if let Some(frame) = root.app.graph_frame() {
                root.app.graph_drag(pos, &frame);
                cx.notify();
            }
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|root, _ev, _win, cx| {
                if root.app.graph_grab.take().is_some() {
                    cx.notify();
                }
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|root, _ev, _win, cx| {
                if root.app.graph_grab.take().is_some() {
                    cx.notify();
                }
            }),
        )
        .into_any_element()
}

/// A row of toggle chips choosing which properties the graph plots (none selected
/// = every keyed property). Rendered in the timeline header when the graph is open.
pub fn property_chips(app: &App, cx: &mut Context<Pulse>) -> impl IntoElement {
    let mut row = div().flex().items_center().gap_1().flex_wrap();
    row = row.child(
        div()
            .text_color(colors::text_secondary())
            .text_size(px(10.0))
            .child("Show:"),
    );
    for prop in Prop::ALL {
        let on = app.graph_shown.contains(&prop);
        let pi = Prop::ALL.iter().position(|p| *p == prop).unwrap_or(0);
        row = row.child(
            div()
                .id(("graph-chip", pi))
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(10.0))
                .text_color(if on { rgb(0x141414) } else { colors::text_primary() })
                .when(on, |d| d.bg(rgb(prop_color(prop))))
                .when(!on, |d| d.bg(colors::surface_raised()))
                .child(prop.label())
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    root.app.apply(Action::ToggleGraphProp(prop));
                    cx.notify();
                })),
        );
    }
    if !app.graph_shown.is_empty() {
        row = row.child(
            div()
                .id("graph-chip-all")
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .bg(colors::surface_raised())
                .text_size(px(10.0))
                .text_color(colors::text_primary())
                .child("all")
                .on_click(cx.listener(|root, _ev, _win, cx| {
                    root.app.apply(Action::ClearGraphProps);
                    cx.notify();
                })),
        );
    }
    row
}

/// A muted hint occupying the plot area when nothing is plottable.
fn hint(msg: &str) -> gpui::Div {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .bg(rgb(PLOT_BG))
        .child(
            div()
                .text_color(colors::text_secondary())
                .text_size(px(11.0))
                .child(msg.to_string()),
        )
}

/// Whether a screen point is inside the plot rect.
fn in_plot(p: (f32, f32), f: &Frame) -> bool {
    p.0 >= f.left && p.0 <= f.right && p.1 >= f.top && p.1 <= f.bottom
}

/// Paint the grid, every plotted curve + its keyframe dots + ease handles, and
/// the playhead guide.
fn paint(
    window: &mut gpui::Window,
    layer: &crate::comp::PulseLayer,
    plotted: &[Prop],
    f: &Frame,
    time: f32,
    grab: Option<GraphGrab>,
) {
    // Horizontal value gridlines (5 divisions).
    for i in 0..=4 {
        let y = f.top + (f.bottom - f.top) * (i as f32 / 4.0);
        hline(window, f.left, f.right, y, GRID);
    }
    // Vertical per-second ticks.
    let secs = f.dur.ceil() as i32;
    for s in 0..=secs {
        let t = s as f32;
        if t > f.dur + 1e-3 {
            break;
        }
        let x = f.t_to_x(t);
        vline(window, x, f.top, f.bottom, GRID);
    }

    for &prop in plotted {
        let color = prop_color(prop);
        let track = layer.track(prop);
        draw_curve(window, track, prop, color, f);

        let keys = &track.keys;
        for (i, k) in keys.iter().enumerate() {
            let kp = (f.t_to_x(k.t), f.v_to_y(k.value));

            // Ease handles for an eased outgoing segment.
            if let Interp::Ease(e) = k.interp {
                if let Some(next) = keys.get(i + 1) {
                    let a = (k.t, k.value);
                    let b = (next.t, next.value);
                    let (hout, hin) = handle_screen_pos(e, a, b, f);
                    seg(window, kp, hout, color);
                    let np = (f.t_to_x(next.t), f.v_to_y(next.value));
                    seg(window, np, hin, color);
                    fill_dot(window, hout, 3.0, color);
                    fill_dot(window, hin, 3.0, color);
                }
            }

            let grabbed = matches!(
                grab,
                Some(GraphGrab::Key { prop: gp, key_index }) if gp == prop && key_index == i
            );
            let r = if grabbed { GRAB_R * 0.7 } else { 4.5 };
            fill_dot(window, kp, r, color);
        }
    }

    // Playhead guide.
    let px_t = f.t_to_x(time);
    vline(window, px_t, f.top, f.bottom, PLAYHEAD);
}

/// Draw a property's value curve by densely sampling the track.
fn draw_curve(window: &mut gpui::Window, track: &Track, prop: Prop, color: u32, f: &Frame) {
    if track.keys.is_empty() {
        return;
    }
    let default = prop.default_value();
    let n = 200;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=n {
        let t = f.dur * (i as f32 / n as f32);
        let v = track.sample(t, default);
        let p = (f.t_to_x(t), f.v_to_y(v));
        if let Some(pp) = prev {
            seg_w(window, pp, p, 1.5, color);
        }
        prev = Some(p);
    }
}

/// Resolve the nearest grabbable element under `pos` (handles before keys).
/// Public on [`App`] via `graph_pick`; see [`App::graph_pick`].
fn pick(
    app: &App,
    pos: (f32, f32),
    f: &Frame,
) -> Option<GraphGrab> {
    let idx = app.selected_layer?;
    let ci = app.active_comp_index();
    let comp = &app.project.comps[ci];
    if idx >= comp.layers.len() {
        return None;
    }
    let props = plotted_props(app);
    let mut best: Option<(f32, GraphGrab)> = None;
    let mut consider = |d: f32, g: GraphGrab| {
        if d <= GRAB_R && best.map(|(bd, _)| d < bd).unwrap_or(true) {
            best = Some((d, g));
        }
    };
    // Handles first (smaller, drawn on top).
    for &prop in &props {
        let keys = &comp.layers[idx].track(prop).keys;
        for (i, k) in keys.iter().enumerate() {
            if let Interp::Ease(e) = k.interp {
                if let Some(next) = keys.get(i + 1) {
                    let a = (k.t, k.value);
                    let b = (next.t, next.value);
                    let (hout, hin) = handle_screen_pos(e, a, b, f);
                    consider(
                        dist(pos, hout),
                        GraphGrab::Handle {
                            prop,
                            key_index: i,
                            which: Handle::Out,
                        },
                    );
                    consider(
                        dist(pos, hin),
                        GraphGrab::Handle {
                            prop,
                            key_index: i,
                            which: Handle::In,
                        },
                    );
                }
            }
        }
    }
    // Keys.
    for &prop in &props {
        let keys = &comp.layers[idx].track(prop).keys;
        for (i, k) in keys.iter().enumerate() {
            let kp = (f.t_to_x(k.t), f.v_to_y(k.value));
            consider(dist(pos, kp), GraphGrab::Key { prop, key_index: i });
        }
    }
    best.map(|(_, g)| g)
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

// --- Painting primitives (filled quads/dots; gpui paths fill, not stroke) ----

fn hline(window: &mut gpui::Window, x0: f32, x1: f32, y: f32, color: u32) {
    seg_w(window, (x0, y), (x1, y), 1.0, color);
}
fn vline(window: &mut gpui::Window, x: f32, y0: f32, y1: f32, color: u32) {
    seg_w(window, (x, y0), (x, y1), 1.0, color);
}
fn seg(window: &mut gpui::Window, a: (f32, f32), b: (f32, f32), color: u32) {
    seg_w(window, a, b, 1.0, color);
}

/// Stroke a segment as a thin filled quad.
fn seg_w(window: &mut gpui::Window, a: (f32, f32), b: (f32, f32), width: f32, color: u32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-4 {
        return;
    }
    let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
    let p0 = point(px(a.0 + nx), px(a.1 + ny));
    let mut path = Path::new(p0);
    path.line_to(point(px(b.0 + nx), px(b.1 + ny)));
    path.line_to(point(px(b.0 - nx), px(b.1 - ny)));
    path.line_to(point(px(a.0 - nx), px(a.1 - ny)));
    path.line_to(p0);
    window.paint_path(path, rgb(color));
}

/// Fill a small diamond dot centred at `c` with half-extent `r` (cheap circle
/// stand-in; gpui's filled paths render a diamond crisply at these sizes).
fn fill_dot(window: &mut gpui::Window, c: (f32, f32), r: f32, color: u32) {
    let mut path = Path::new(point(px(c.0), px(c.1 - r)));
    path.line_to(point(px(c.0 + r), px(c.1)));
    path.line_to(point(px(c.0), px(c.1 + r)));
    path.line_to(point(px(c.0 - r), px(c.1)));
    path.line_to(point(px(c.0), px(c.1 - r)));
    window.paint_path(path, rgb(color));
}

impl App {
    /// Rebuild the graph plot's coordinate [`Frame`] from the last-painted plot
    /// bounds, so a mouse listener can hit-test in the same space the painter used.
    /// Returns `None` until the plot has painted (or nothing is plottable).
    fn graph_frame(&self) -> Option<Frame> {
        let b = self.preview_graph_rect()?;
        let idx = self.selected_layer?;
        let ci = self.active_comp_index();
        let comp = &self.project.comps[ci];
        if idx >= comp.layers.len() {
            return None;
        }
        let props = plotted_props(self);
        let (vlo, vhi) = value_axis(comp, idx, &props)?;
        Some(Frame {
            left: f32::from(b.origin.x) + AXIS_W,
            top: f32::from(b.origin.y) + PAD_Y,
            right: f32::from(b.origin.x) + f32::from(b.size.width) - 8.0,
            bottom: f32::from(b.origin.y) + f32::from(b.size.height) - PAD_Y,
            dur: comp.duration.max(0.001),
            vlo,
            vhi,
        })
    }

    /// The plot-area bounds recorded by the graph canvas (window-relative). Shared
    /// through `self.track_bounds`-style cell; see [`graph_rect`](Self::graph_rect).
    fn preview_graph_rect(&self) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.graph_rect.get()
    }

    /// Pick the grabbable graph element under a screen pointer (handles first).
    fn graph_pick(&self, pos: (f32, f32), f: &Frame) -> Option<GraphGrab> {
        pick(self, pos, f)
    }

    /// Drive an in-flight graph drag: a key drag retimes+revalues the keyframe; a
    /// handle drag reshapes (and promotes) the segment's ease.
    fn graph_drag(&mut self, pos: (f32, f32), f: &Frame) {
        let Some(grab) = self.app_graph_grab() else {
            return;
        };
        match grab {
            GraphGrab::Key { prop, key_index } => {
                self.apply(Action::MoveKeyframeXY {
                    prop,
                    key_index,
                    time: f.x_to_t(pos.0),
                    value: f.y_to_v(pos.1),
                });
            }
            GraphGrab::Handle {
                prop,
                key_index,
                which,
            } => {
                let Some(idx) = self.selected_layer else {
                    return;
                };
                let ci = self.active_comp_index();
                let track = self.project.comps[ci].layers[idx].track(prop);
                let (a, b) = match (track.keys.get(key_index), track.keys.get(key_index + 1)) {
                    (Some(ka), Some(kb)) => ((ka.t, ka.value), (kb.t, kb.value)),
                    _ => return,
                };
                let (at, av) = a;
                let (bt, bv) = b;
                let nx = if (bt - at).abs() > f32::EPSILON {
                    (f.x_to_t(pos.0) - at) / (bt - at)
                } else {
                    0.0
                };
                let ny = if (bv - av).abs() > f32::EPSILON {
                    (f.y_to_v(pos.1) - av) / (bv - av)
                } else {
                    0.0
                };
                let base = match track.keys[key_index].interp {
                    Interp::Ease(e) => e,
                    _ => Ease::LINEAR,
                };
                let updated = match which {
                    Handle::Out => base.with_out(nx, ny),
                    Handle::In => base.with_in(nx, ny),
                };
                self.apply(Action::SetInterp {
                    prop,
                    key_index,
                    interp: Interp::Ease(updated),
                });
            }
        }
    }

    /// Borrow the current graph grab (helper to keep `graph_drag` borrow-clean).
    fn app_graph_grab(&self) -> Option<GraphGrab> {
        self.graph_grab
    }
}
