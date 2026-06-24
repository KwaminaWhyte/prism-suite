// Batch 5 apply logic — all new Action variants are routed here from mod.rs.

use super::{App, Action};
use super::reel_project::{
    BinColor, ExportContainer, ExportPresetB5, HistogramChannel,
    MediaItem, MediaKind, ParadeType, ProjectBin, ScopeColorspace, ScopeKind, ScopeLayout,
    SequenceSettingsB5, VectorscopeType, VideoCodecB5, AudioCodecB5, WaveformType,
};
use super::timeline::{PagePeelDirection, SlideDirection, CubeDirection, TransitionKind};
use super::DEFAULT_TRANSITION_DUR;

pub trait AppBatch5Ext {
    fn apply_batch5(&mut self, action: Action);
}

impl AppBatch5Ext for App {
    fn apply_batch5(&mut self, action: Action) {
        match action {
            // ---- A. Lumetri Scopes ------------------------------------------
            Action::ToggleScopesPanel => {
                self.scopes_config.enabled = !self.scopes_config.enabled;
            }
            Action::SetScopeKind(k) => { self.scopes_config.scope_kind = k; }
            Action::SetScopeLayout(l) => { self.scopes_config.layout = l; }
            Action::SetWaveformType(wt) => { self.scopes_config.waveform_type = wt; }
            Action::SetParadeType(pt) => { self.scopes_config.parade_type = pt; }
            Action::SetVectorscopeType(vt) => { self.scopes_config.vectorscope_type = vt; }
            Action::SetHistogramChannel(hc) => { self.scopes_config.histogram_channel = hc; }
            Action::SetScopeIntensity(v) => {
                self.scopes_config.intensity = v.clamp(10.0, 300.0);
            }
            Action::SetScopeColorspace(cs) => { self.scopes_config.colorspace = cs; }
            Action::SetScopeShowClipping(v) => { self.scopes_config.show_clipping = v; }

            // ---- B. Project Bins --------------------------------------------
            Action::CreateBin { name, parent_id } => {
                let id = self.next_bin_id;
                self.next_bin_id += 1;
                self.project_bins.push(ProjectBin {
                    id, name, parent_id,
                    color_label: BinColor::None,
                    item_ids: Vec::new(),
                    expanded: true,
                });
            }
            Action::RenameBin { bin_id, name } => {
                if let Some(b) = self.project_bins.iter_mut().find(|b| b.id == bin_id) {
                    b.name = name;
                }
            }
            Action::DeleteBin { bin_id } => {
                self.project_bins.retain(|b| b.id != bin_id);
            }
            Action::SetBinColor { bin_id, color } => {
                if let Some(b) = self.project_bins.iter_mut().find(|b| b.id == bin_id) {
                    b.color_label = color;
                }
            }
            Action::ToggleBinExpanded { bin_id } => {
                if let Some(b) = self.project_bins.iter_mut().find(|b| b.id == bin_id) {
                    b.expanded = !b.expanded;
                }
            }
            Action::ImportMedia2 { path, bin_id } => {
                let id = self.next_media_item_id;
                self.next_media_item_id += 1;
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let item = MediaItem {
                    id, name, path,
                    duration_s: 0.0, frame_rate: 30.0,
                    width: 0, height: 0,
                    has_audio: false, has_video: false,
                    media_kind: MediaKind::Video,
                    proxy_path: None,
                    label: BinColor::None,
                    offline: false,
                    log_note: String::new(),
                };
                if let Some(bid) = bin_id {
                    if let Some(b) = self.project_bins.iter_mut().find(|b| b.id == bid) {
                        b.item_ids.push(id);
                    }
                }
                self.media_items.push(item);
            }
            Action::RemoveMedia { item_id } => {
                self.media_items.retain(|m| m.id != item_id);
                for b in &mut self.project_bins {
                    b.item_ids.retain(|&i| i != item_id);
                }
            }
            Action::MoveMediaToBin { item_id, bin_id } => {
                // Remove from all bins first
                for b in &mut self.project_bins {
                    b.item_ids.retain(|&i| i != item_id);
                }
                if let Some(bid) = bin_id {
                    if let Some(b) = self.project_bins.iter_mut().find(|b| b.id == bid) {
                        b.item_ids.push(item_id);
                    }
                }
            }
            Action::SetMediaLabel { item_id, label } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.label = label;
                }
            }
            Action::SetMediaLogNote { item_id, note } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.log_note = note;
                }
            }
            Action::SetMediaOffline { item_id, offline } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.offline = offline;
                }
            }
            Action::RelinkMedia { item_id, new_path } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.path = new_path;
                    m.offline = false;
                }
            }
            Action::SetProjectSearch(q) => { self.project_search_query = q; }
            Action::SetMediaBrowserPath2(p) => { self.media_browser_path = p; }
            Action::AttachMediaProxy { item_id, proxy_path } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.proxy_path = Some(proxy_path);
                }
            }
            Action::DetachMediaProxy { item_id } => {
                if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
                    m.proxy_path = None;
                }
            }

            // ---- C. Transitions ---------------------------------------------
            Action::SetTransitionKind { transition_id, kind } => {
                if let Some(t) = self.project.transitions.get_mut(transition_id) {
                    t.kind = kind;
                }
            }
            Action::AddWipeTransition { clip_id, direction, duration_s } => {
                let t = self.time;
                let dur = if duration_s > 0.0 { duration_s as f32 } else { DEFAULT_TRANSITION_DUR };
                let slide_dir = match direction {
                    SlideDirection::Left  => SlideDirection::Left,
                    SlideDirection::Right => SlideDirection::Right,
                    SlideDirection::Up    => SlideDirection::Up,
                    SlideDirection::Down  => SlideDirection::Down,
                };
                self.project.add_transition(clip_id, t, TransitionKind::Slide(slide_dir), dur);
            }
            Action::AddPagePeelTransition { clip_id, direction } => {
                let t = self.time;
                self.project.add_transition(
                    clip_id, t,
                    TransitionKind::PagePeel { direction, softness: 0.1 },
                    DEFAULT_TRANSITION_DUR,
                );
            }
            Action::AddZoomTransition { clip_id, grow } => {
                let t = self.time;
                self.project.add_transition(
                    clip_id, t,
                    TransitionKind::Zoom { grow },
                    DEFAULT_TRANSITION_DUR,
                );
            }
            Action::AddDipTransition { clip_id, color: _ } => {
                let t = self.time;
                self.project.add_transition(
                    clip_id, t,
                    TransitionKind::DipToBlack,
                    DEFAULT_TRANSITION_DUR,
                );
            }
            Action::AddCubeTransition { clip_id, direction } => {
                let t = self.time;
                self.project.add_transition(
                    clip_id, t,
                    TransitionKind::Cube { direction, lighting: true },
                    DEFAULT_TRANSITION_DUR,
                );
            }

            // ---- D. Export Presets B5 ---------------------------------------
            Action::SelectExportPresetB5 { preset_id } => {
                self.active_export_preset_b5 = Some(preset_id);
            }
            Action::AddCustomExportPresetB5 { mut preset } => {
                preset.id = self.next_preset_id;
                self.next_preset_id += 1;
                self.export_presets_b5.push(preset);
            }
            Action::DeleteCustomExportPresetB5 { preset_id } => {
                // Only allow deleting custom presets (id > 7)
                self.export_presets_b5.retain(|p| p.id <= 7 || p.id != preset_id);
            }
            Action::DuplicateExportPresetB5 { preset_id } => {
                if let Some(p) = self.export_presets_b5.iter().find(|p| p.id == preset_id).cloned() {
                    let new_id = self.next_preset_id;
                    self.next_preset_id += 1;
                    let mut copy = p.clone();
                    copy.id = new_id;
                    copy.name = format!("{} (copy)", copy.name);
                    self.export_presets_b5.push(copy);
                }
            }
            Action::SetExportWidthB5(w) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.width = w;
                    }
                }
            }
            Action::SetExportHeightB5(h) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.height = h;
                    }
                }
            }
            Action::SetExportFrameRateB5(fr) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.frame_rate = fr;
                    }
                }
            }
            Action::SetExportVideoBitrateB5(bps) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.video_bitrate_kbps = bps;
                    }
                }
            }
            Action::SetExportAudioBitrateB5(bps) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.audio_bitrate_kbps = bps;
                    }
                }
            }
            Action::SetExportContainerB5(c) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.container = c;
                    }
                }
            }
            Action::SetExportVideoCodecB5(vc) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.video_codec = vc;
                    }
                }
            }
            Action::SetExportAudioCodecB5(ac) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.audio_codec = ac;
                    }
                }
            }
            Action::SetExportTwoPassB5(v) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.two_pass = v;
                    }
                }
            }
            Action::SetExportHardwareEncodeB5(v) => {
                if let Some(id) = self.active_export_preset_b5 {
                    if let Some(p) = self.export_presets_b5.iter_mut().find(|p| p.id == id) {
                        p.hardware_encode = v;
                    }
                }
            }

            // ---- E. Sequence Settings B5 ------------------------------------
            Action::NewSequenceB5 { name, width, height, frame_rate } => {
                let id = self.next_sequence_id;
                self.next_sequence_id += 1;
                let mut seq = SequenceSettingsB5::default_1080p(id, name);
                seq.width = width;
                seq.height = height;
                seq.frame_rate = frame_rate;
                seq.timebase = frame_rate;
                self.sequences_b5.push(seq);
                self.active_sequence_id = id;
            }
            Action::DuplicateSequenceB5 { sequence_id } => {
                if let Some(s) = self.sequences_b5.iter().find(|s| s.id == sequence_id).cloned() {
                    let id = self.next_sequence_id;
                    self.next_sequence_id += 1;
                    let mut copy = s.clone();
                    copy.id = id;
                    copy.name = format!("{} (copy)", copy.name);
                    self.sequences_b5.push(copy);
                }
            }
            Action::DeleteSequenceB5 { sequence_id } => {
                // Must always have at least one sequence
                if self.sequences_b5.len() > 1 {
                    self.sequences_b5.retain(|s| s.id != sequence_id);
                    if self.active_sequence_id == sequence_id {
                        self.active_sequence_id =
                            self.sequences_b5.first().map(|s| s.id).unwrap_or(0);
                    }
                }
            }
            Action::SetActiveSequenceB5 { sequence_id } => {
                if self.sequences_b5.iter().any(|s| s.id == sequence_id) {
                    self.active_sequence_id = sequence_id;
                }
            }
            Action::UpdateSequenceSettingsB5 { sequence_id, width, height, frame_rate } => {
                if let Some(s) = self.sequences_b5.iter_mut().find(|s| s.id == sequence_id) {
                    if let Some(w) = width { s.width = w; }
                    if let Some(h) = height { s.height = h; }
                    if let Some(fr) = frame_rate { s.frame_rate = fr; s.timebase = fr; }
                }
            }
            Action::NestSequenceB5 { sequence_id, into_sequence_id: _, at_time_s: _ } => {
                // Stub: acknowledge that sequence_id is nested
                let _ = sequence_id;
            }
            Action::RenameSequence { sequence_id, name } => {
                // Ignore a blank name so a fully-cleared field never wipes the
                // sequence's label; trim surrounding whitespace from typed input.
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    if let Some(s) = self.sequences_b5.iter_mut().find(|s| s.id == sequence_id) {
                        s.name = trimmed.to_string();
                    }
                }
            }

            _ => {}
        }
    }
}

