//! The layer-stack panel: the add/select/reorder/delete layer list.

use super::PulseApp;
use crate::comp::{blend_label, BlendMode, LayerKind};
use crate::icons;

impl PulseApp {
    pub(super) fn layers_panel(&mut self, root: &mut egui::Ui) {
        egui::SidePanel::left("layers")
            .default_width(210.0)
            .show_inside(root, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.heading("Layers");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button(icons::ADD_LAYER)
                            .on_hover_text("Add layer")
                            .clicked()
                        {
                            self.add_layer();
                        }
                    });
                });
                ui.add_space(2.0);

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.comp.hide_shy, "Hide shy");
                });

                let mut to_delete: Option<usize> = None;
                let mut to_move: Option<(usize, bool)> = None;
                let n = self.comp.layers.len();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Top of the list = front of the stack (highest index).
                        for idx in (0..n).rev() {
                            // Hide shy layers when hide_shy is set
                            if self.comp.hide_shy && self.comp.layers[idx].shy {
                                continue;
                            }
                            let selected = self.selected == Some(idx);
                            ui.horizontal(|ui| {
                                let vis = self.comp.layers[idx].visible;
                                let eye = if vis { icons::EYE } else { icons::EYE_OFF };
                                if ui.button(eye).on_hover_text("Visibility").clicked() {
                                    self.comp.layers[idx].visible = !vis;
                                }
                                // Solo button
                                let solo_icon = if self.comp.layers[idx].solo { "◉" } else { "○" };
                                if ui.button(solo_icon).on_hover_text("Solo").clicked() {
                                    self.comp.layers[idx].solo = !self.comp.layers[idx].solo;
                                }
                                // Shy button
                                let shy_icon = if self.comp.layers[idx].shy { "H" } else { "h" };
                                if ui.button(shy_icon).on_hover_text("Shy").clicked() {
                                    self.comp.layers[idx].shy = !self.comp.layers[idx].shy;
                                }
                                if ui
                                    .selectable_label(selected, &self.comp.layers[idx].name)
                                    .clicked()
                                {
                                    self.selected = Some(idx);
                                }
                                // Null label
                                if self.comp.layers[idx].kind == LayerKind::Null {
                                    ui.weak("(null)");
                                }
                                // Mark a layer that is being used as the track-matte
                                // source for the layer directly below it.
                                if self.comp.is_matte_source(idx) {
                                    ui.weak(icons::MATTE)
                                        .on_hover_text("Used as a track matte for the layer below");
                                }
                                // A compact badge for any layer with a non-Normal
                                // blend mode (the Layers-panel blend column), so the
                                // stack's compositing reads at a glance.
                                let mode = self.comp.layers[idx].blend_mode();
                                if mode != BlendMode::Normal {
                                    ui.weak(egui::RichText::new(icons::BLEND).small())
                                        .on_hover_text(format!("Blend: {}", blend_label(mode)));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button(icons::TRASH).on_hover_text("Delete").clicked()
                                        {
                                            to_delete = Some(idx);
                                        }
                                        if ui
                                            .add_enabled(
                                                idx + 1 < n,
                                                egui::Button::new(icons::ARROW_UP),
                                            )
                                            .on_hover_text("Move up")
                                            .clicked()
                                        {
                                            to_move = Some((idx, true));
                                        }
                                        if ui
                                            .add_enabled(
                                                idx > 0,
                                                egui::Button::new(icons::ARROW_DOWN),
                                            )
                                            .on_hover_text("Move down")
                                            .clicked()
                                        {
                                            to_move = Some((idx, false));
                                        }
                                    },
                                );
                            });
                        }
                        if n == 0 {
                            ui.weak("No layers. Click + to add one.");
                        }
                    });

                if let Some(i) = to_delete {
                    self.delete_layer(i);
                }
                if let Some((i, up)) = to_move {
                    self.move_layer(i, up);
                }
            });
    }
}
