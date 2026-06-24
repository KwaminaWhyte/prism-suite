//! Keyboard input handling for the Drift root view.
//!
//! Houses the `on_key` dispatcher (inspector-field editing, AI-prompt typing,
//! transport/tool shortcuts, arrow-key nudge) and `apply_field_edit`, the
//! commit path for inline inspector numeric edits. These are methods on
//! [`Drift`]; the root view in `main.rs` wires `on_key` into `on_key_down`.

use gpui::{Context, KeyDownEvent};

use crate::app_state::{Action, DriftTool, LayerTransform};
use crate::{Drift, InspectorField};

impl Drift {
    pub(crate) fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;

        // Inspector field editing — intercept all input first
        if self.editing_field.is_some() {
            match ks.key.as_str() {
                "escape" => {
                    self.editing_field = None;
                    self.field_buffer.clear();
                    cx.notify();
                }
                "enter" => {
                    self.apply_field_edit(cx);
                }
                "backspace" => {
                    self.field_buffer.pop();
                    cx.notify();
                }
                k if k.len() == 1 => {
                    let ch = k.chars().next().unwrap_or('\0');
                    if ch.is_ascii_digit()
                        || ch == '.'
                        || (ch == '-' && self.field_buffer.is_empty())
                    {
                        self.field_buffer.push(ch);
                        cx.notify();
                    }
                }
                _ => {}
            }
            return;
        }

        // The AI motion prompt is now a real `prism_ui::TextField` that owns its
        // own key handling, so the root view no longer intercepts prompt typing.

        if m.platform && !m.alt && !m.control {
            match ks.key.as_str() {
                "z" if m.shift => {
                    self.app.apply(Action::Redo);
                    cx.notify();
                    return;
                }
                "z" => {
                    self.app.apply(Action::Undo);
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
        // Spacebar: play/pause
        if !m.platform && !m.control && !m.alt && !m.shift {
            match ks.key.as_str() {
                " " => {
                    if self.app.playing {
                        self.app.apply(Action::Pause);
                    } else {
                        self.app.apply(Action::Play);
                    }
                    cx.notify();
                }
                "delete" | "backspace" => {
                    if let Some(lid) = self.app.active_layer {
                        self.app.apply(Action::DeleteLayer(lid));
                        cx.notify();
                    }
                }
                "left" => {
                    self.app.apply(Action::StepBackward);
                    cx.notify();
                }
                "right" => {
                    self.app.apply(Action::StepForward);
                    cx.notify();
                }
                "home" => {
                    self.app.apply(Action::GoToFirstFrame);
                    cx.notify();
                }
                "end" => {
                    self.app.apply(Action::GoToLastFrame);
                    cx.notify();
                }
                // Tool shortcuts (advertised in hint bar)
                "v" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Select));
                    cx.notify();
                }
                "m" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Move));
                    cx.notify();
                }
                "p" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Pen));
                    cx.notify();
                }
                "r" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Rect));
                    cx.notify();
                }
                "e" => {
                    self.app.apply(Action::SetActiveTool(DriftTool::Ellipse));
                    cx.notify();
                }
                // Arrow-key nudge when Move tool is active
                "up" | "down" | "left" | "right"
                    if self.app.active_tool == DriftTool::Move =>
                {
                    if let Some(id) = self.app.active_layer {
                        let t = self.app.transforms.get(&id)
                            .cloned()
                            .unwrap_or_else(crate::app_state::LayerTransform::new);
                        let step = if ks.modifiers.shift { 10.0_f32 } else { 1.0_f32 };
                        let (nx, ny) = match ks.key.as_str() {
                            "up"    => (t.x, t.y - step),
                            "down"  => (t.x, t.y + step),
                            "left"  => (t.x - step, t.y),
                            _       => (t.x + step, t.y),
                        };
                        self.app.apply(Action::SetLayerPosition { id, x: nx, y: ny });
                        cx.notify();
                    }
                }
                _ => {}
            }
        }
    }

    /// Apply the buffered text as a numeric value to the currently edited inspector field.
    pub(crate) fn apply_field_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(field) = self.editing_field.take() {
            let val = self.field_buffer.parse::<f32>().unwrap_or(0.0);
            self.field_buffer.clear();
            if let Some(id) = self.app.active_layer {
                let t = self.app.transforms.get(&id).cloned()
                    .unwrap_or_else(LayerTransform::new);
                match field {
                    InspectorField::PositionX => self.app.apply(Action::SetLayerPosition {
                        id, x: val, y: t.y,
                    }),
                    InspectorField::PositionY => self.app.apply(Action::SetLayerPosition {
                        id, x: t.x, y: val,
                    }),
                    InspectorField::ScaleX => self.app.apply(Action::SetLayerScale {
                        id, x: val.max(0.001), y: t.scale_y,
                    }),
                    InspectorField::ScaleY => self.app.apply(Action::SetLayerScale {
                        id, x: t.scale_x, y: val.max(0.001),
                    }),
                    InspectorField::Rotation => self.app.apply(Action::SetLayerRotation {
                        id, degrees: val,
                    }),
                    InspectorField::Opacity => self.app.apply(Action::SetLayerOpacity {
                        id, opacity: (val / 100.0).clamp(0.0, 1.0),
                    }),
                }
            }
            cx.notify();
        }
    }
}
