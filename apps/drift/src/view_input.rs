//! Keyboard input handling for the Drift root view.
//!
//! Houses the `on_key` dispatcher (transport/tool shortcuts, arrow-key nudge).
//! This is a method on [`Drift`]; the root view in `main.rs` wires `on_key`
//! into `on_key_down`.
//!
//! Note: numeric inspector / document fields are real `prism_ui::TextField`s
//! and the AI prompts / script editor are `prism_ui::TextArea`s. Each owns its
//! own key handling, so the root view no longer intercepts typing — it only
//! handles global transport, tool, and nudge shortcuts.

use gpui::{Context, KeyDownEvent};

use crate::app_state::{Action, DriftTool};
use crate::Drift;

impl Drift {
    pub(crate) fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;

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
}
