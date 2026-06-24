use super::*;

impl App {
    pub(super) fn apply_extended4(&mut self, action: Action) {
        match action {
            // --- Batch 11: Pathfinder depth ---
            Action::ApplyPathfinderOp(op) => {
                self.apply_pathfinder_op(op);
            }
            Action::SetPathfinderPrecision(v) => {
                self.pathfinder_precision = v.clamp(0.001, 10.0);
            }
            Action::SetPathfinderRemoveRedundant(v) => {
                self.pathfinder_remove_redundant = v;
            }
            Action::SetPathfinderDivideStroke(v) => {
                self.pathfinder_divide_stroke = v;
            }
            Action::RepeatPathfinder => {
                // Re-apply the last recorded op to the current selection.
                if let Some(op) = self.last_pathfinder_op {
                    self.apply_pathfinder_op(op);
                }
            }

            // --- Batch 11: 3D Extrude depth ---
            Action::ToggleExtrudePanel => {
                self.extrude_panel_open = !self.extrude_panel_open;
            }
            Action::SetExtrudeDepth(v) => {
                self.extrude_config.depth = v.clamp(0.0, 2000.0);
            }
            Action::SetExtrudeRotation { x, y, z } => {
                self.extrude_config.rotation_x = x.clamp(-180.0, 180.0);
                self.extrude_config.rotation_y = y.clamp(-180.0, 180.0);
                self.extrude_config.rotation_z = z.clamp(-180.0, 180.0);
            }
            Action::SetExtrudePerspective(v) => {
                self.extrude_config.perspective = v.clamp(0.0, 160.0);
            }
            Action::SetExtrudeSurface(s) => {
                self.extrude_config.surface = s;
            }
            Action::SetExtrudeCapStyle(c) => {
                self.extrude_config.cap_style = c;
            }
            Action::SetExtrudeBevelHeight(v) => {
                self.extrude_config.bevel_height = v.clamp(0.0, 100.0);
            }
            Action::SetExtrudeLighting { intensity, ambient, specular, gloss } => {
                self.extrude_config.light_intensity = intensity.clamp(0.0, 100.0);
                self.extrude_config.ambient_light = ambient.clamp(0.0, 100.0);
                self.extrude_config.specular_highlight = specular.clamp(0.0, 100.0);
                self.extrude_config.gloss = gloss.clamp(0.0, 100.0);
            }
            Action::SetExtrudeMapArt(v) => {
                self.extrude_config.map_art = v;
            }
            Action::ApplyExtrude => {
                if let Some(idx) = self.selected {
                    if !self.extrude_applied_shapes.contains(&idx) {
                        self.extrude_applied_shapes.push(idx);
                    }
                }
            }
            Action::ExpandExtrude => {
                self.extrude_applied_shapes.clear();
            }

            // --- Batch 11: Chart depth ---
            Action::ToggleChartPanel => {
                self.chart_panel_open = !self.chart_panel_open;
            }
            Action::AddChartDataSet(ds) => {
                self.chart_config.datasets.push(ds);
            }
            Action::RemoveChartDataSet(idx) => {
                if idx < self.chart_config.datasets.len() {
                    self.chart_config.datasets.remove(idx);
                }
            }
            Action::SetChartDataSetValues { idx, values } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.values = values;
                }
            }
            Action::SetChartDataSetLabel { idx, label } => {
                if let Some(ds) = self.chart_config.datasets.get_mut(idx) {
                    ds.label = label;
                }
            }
            Action::SetChartCategoryLabels(labels) => {
                self.chart_config.category_labels = labels;
            }
            Action::SetChartTitle(title) => {
                self.chart_config.title = title;
            }
            Action::SetChartShowLegend(v) => {
                self.chart_config.show_legend = v;
            }
            Action::SetChartShowGrid(v) => {
                self.chart_config.show_grid = v;
            }
            Action::SetChartColumnWidth(v) => {
                self.chart_config.column_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartClusterWidth(v) => {
                self.chart_config.cluster_width = v.clamp(20.0, 100.0);
            }
            Action::SetChartValueRange { min, max } => {
                self.chart_config.value_axis_min = min;
                self.chart_config.value_axis_max = max;
            }
            Action::ApplyChartData => {
                // Generate real vector geometry from the chart config.
                self.apply_chart_data_geometry();
            }

            // --- Batch 11: Envelope Distort depth ---
            Action::ToggleEnvelopePanel => {
                self.envelope_panel_open = !self.envelope_panel_open;
            }
            Action::SetEnvelopeWarpStyle(s) => {
                self.envelope_config.warp_style = s;
            }
            Action::SetEnvelopeAxis(h) => {
                self.envelope_config.horizontal = h;
            }
            Action::SetEnvelopeBend(v) => {
                self.envelope_config.bend = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeHDistortion(v) => {
                self.envelope_config.h_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeVDistortion(v) => {
                self.envelope_config.v_distortion = v.clamp(-100.0, 100.0);
            }
            Action::SetEnvelopeFidelity(v) => {
                self.envelope_config.fidelity = v.clamp(0.0, 100.0);
            }
            Action::SetEnvelopeEditMode(m) => {
                self.envelope_config.edit_mode = m;
            }
            Action::MakeEnvelopeWithWarpPreset => {
                self.apply_make_envelope_warp();
            }
            Action::MakeEnvelopeWithMeshPreset => {
                // A mesh-preset envelope deforms through the same bilinear mesh as
                // the warp preset (the warp style chosen in the config seeds it).
                self.apply_make_envelope_warp();
            }
            Action::ReleaseEnvelopeAll => {
                self.apply_expand_envelope();
            }
            Action::ExpandEnvelope => {
                self.apply_expand_envelope();
            }


            a => self.apply_waves_wn(a),
        }
    }
}
