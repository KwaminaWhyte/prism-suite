use super::{App, Action};

/// Motion graphics template parameter value.
#[derive(Debug, Clone)]
pub enum MogrParamValue {
    Text(String),
    Color([f32; 4]),
    Number(f32),
    Boolean(bool),
}

impl Default for MogrParamValue {
    fn default() -> Self { MogrParamValue::Text(String::new()) }
}

/// Motion graphics template parameter.
#[derive(Debug, Clone, Default)]
pub struct MogrParam {
    pub name: String,
    pub value: MogrParamValue,
}

/// A motion graphics template (.mogrt).
#[derive(Debug, Clone, Default)]
pub struct MogrTemplate {
    pub name: String,
    pub file_path: std::path::PathBuf,
    pub duration_frames: u32,
    pub params: Vec<MogrParam>,
}

/// Sequence frame rate options.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SeqFrameRate {
    Fps23_976, Fps24, Fps25,
    #[default] Fps29_97,
    Fps30, Fps50, Fps59_94, Fps60,
}

/// Sequence pixel aspect ratio options.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SeqPixelAspect {
    #[default] Square, D1Ntsc, D1Pal, Anamorphic,
}

/// Extended sequence settings.
#[derive(Debug, Clone, Default)]
pub struct SequenceSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: SeqFrameRate,
    pub pixel_aspect: SeqPixelAspect,
    pub audio_sample_rate: u32,
    pub audio_channels: u8,
    pub preview_codec: String,
}

impl SequenceSettings {
    pub fn hd_1080p() -> Self {
        Self {
            width: 1920,
            height: 1080,
            audio_sample_rate: 48000,
            audio_channels: 2,
            preview_codec: "I-Frame Only MPEG".to_string(),
            ..Default::default()
        }
    }
}

/// A nested sub-sequence.
#[derive(Debug, Clone, Default)]
pub struct NestedSequence {
    pub name: String,
    pub clip_indices: Vec<usize>,
    pub start_time: f32,
    pub duration: f32,
}

pub trait AppGraphicsExt {
    fn apply_graphics(&mut self, action: Action);
}

impl AppGraphicsExt for App {
    fn apply_graphics(&mut self, action: Action) {
        match action {
            Action::ToggleMogrLibrary => { self.mogr_library_open = !self.mogr_library_open; }
            Action::AddMogrTemplate(t) => { self.mogr_templates.push(t); }
            Action::RemoveMogrTemplate(idx) => {
                if idx < self.mogr_templates.len() {
                    self.mogr_templates.remove(idx);
                    self.mogr_applied_clips.retain(|&(_, ti)| ti != idx);
                    for entry in &mut self.mogr_applied_clips {
                        if entry.1 > idx { entry.1 -= 1; }
                    }
                    if let Some(a) = self.active_mogr {
                        if a == idx { self.active_mogr = None; }
                        else if a > idx { self.active_mogr = Some(a - 1); }
                    }
                }
            }
            Action::SetActiveMogr(opt) => { self.active_mogr = opt; }
            Action::SetMogrParamText { template_idx, param_idx, text } => {
                if let Some(t) = self.mogr_templates.get_mut(template_idx) {
                    if let Some(p) = t.params.get_mut(param_idx) {
                        p.value = MogrParamValue::Text(text);
                    }
                }
            }
            Action::SetMogrParamNumber { template_idx, param_idx, value } => {
                if let Some(t) = self.mogr_templates.get_mut(template_idx) {
                    if let Some(p) = t.params.get_mut(param_idx) {
                        p.value = MogrParamValue::Number(value);
                    }
                }
            }
            Action::SetMogrParamColor { template_idx, param_idx, color } => {
                if let Some(t) = self.mogr_templates.get_mut(template_idx) {
                    if let Some(p) = t.params.get_mut(param_idx) {
                        p.value = MogrParamValue::Color(color);
                    }
                }
            }
            Action::ApplyMogrToClip { clip_idx, template_idx } => {
                if let Some(entry) = self.mogr_applied_clips.iter_mut().find(|(ci, _)| *ci == clip_idx) {
                    entry.1 = template_idx;
                } else {
                    self.mogr_applied_clips.push((clip_idx, template_idx));
                }
            }
            Action::DetachMogrFromClip { clip_idx } => {
                self.mogr_applied_clips.retain(|&(ci, _)| ci != clip_idx);
            }
            Action::ToggleSequenceSettingsPanel => { self.sequence_settings_open = !self.sequence_settings_open; }
            Action::SetSeqResolution { w, h } => {
                self.sequence_settings.width = w.max(1);
                self.sequence_settings.height = h.max(1);
            }
            Action::SetSeqFrameRate(fr) => { self.sequence_settings.frame_rate = fr; }
            Action::SetSeqPixelAspect(pa) => { self.sequence_settings.pixel_aspect = pa; }
            Action::SetSeqAudioSampleRate(rate) => { self.sequence_settings.audio_sample_rate = rate; }
            Action::SetSeqAudioChannels(ch) => {
                self.sequence_settings.audio_channels = ch.clamp(1, 8);
            }
            Action::SetSeqPreviewCodec(codec) => { self.sequence_settings.preview_codec = codec; }
            Action::ApplySequenceSettings => {}
            Action::NestSelectedClipsB9 { name } => {
                let n = self.project.clips.len();
                let end = 2_usize.min(n);
                let clip_indices: Vec<usize> = (0..end).collect();
                let start_time = self.project.clips.first().map(|c| c.start).unwrap_or(0.0);
                let duration = clip_indices.iter()
                    .filter_map(|&i| self.project.clips.get(i))
                    .map(|c| c.end())
                    .fold(0.0_f32, f32::max) - start_time;
                self.nested_sequences.push(NestedSequence {
                    name, clip_indices, start_time, duration: duration.max(0.0),
                });
            }
            Action::UnnestSequence(idx) => {
                if idx < self.nested_sequences.len() { self.nested_sequences.remove(idx); }
            }
            Action::EnterNestedSequence(idx) => {
                if idx < self.nested_sequences.len() { self.active_nested_seq = Some(idx); }
            }
            Action::ExitNestedSequence => { self.active_nested_seq = None; }
            Action::RenameNestedSequence { idx, name } => {
                if let Some(ns) = self.nested_sequences.get_mut(idx) { ns.name = name; }
            }
            Action::DuplicateNestedSequence(idx) => {
                if idx < self.nested_sequences.len() {
                    let clone = self.nested_sequences[idx].clone();
                    self.nested_sequences.push(clone);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn test_add_remove_mogr_template() {
        let mut app = App::new();
        assert_eq!(app.mogr_templates.len(), 0);
        app.apply(Action::AddMogrTemplate(MogrTemplate {
            name: "Lower Third".to_string(),
            file_path: std::path::PathBuf::from("templates/lower_third.mogrt"),
            duration_frames: 90,
            params: Vec::new(),
        }));
        assert_eq!(app.mogr_templates.len(), 1);
        assert_eq!(app.mogr_templates[0].name, "Lower Third");
        app.apply(Action::RemoveMogrTemplate(0));
        assert_eq!(app.mogr_templates.len(), 0);
        // Out-of-bounds remove is a no-op.
        app.apply(Action::RemoveMogrTemplate(99));
        assert_eq!(app.mogr_templates.len(), 0);
    }

    #[test]
    fn test_apply_mogr_to_clip() {
        let mut app = App::new();
        app.apply(Action::AddMogrTemplate(MogrTemplate { name: "T1".to_string(), ..Default::default() }));
        app.apply(Action::ApplyMogrToClip { clip_idx: 3, template_idx: 0 });
        assert_eq!(app.mogr_applied_clips.len(), 1);
        assert_eq!(app.mogr_applied_clips[0], (3, 0));
        // Applying again to same clip replaces entry.
        app.apply(Action::AddMogrTemplate(MogrTemplate { name: "T2".to_string(), ..Default::default() }));
        app.apply(Action::ApplyMogrToClip { clip_idx: 3, template_idx: 1 });
        assert_eq!(app.mogr_applied_clips.len(), 1);
        assert_eq!(app.mogr_applied_clips[0], (3, 1));
    }

    #[test]
    fn test_detach_mogr_from_clip() {
        let mut app = App::new();
        app.apply(Action::AddMogrTemplate(MogrTemplate { name: "T".to_string(), ..Default::default() }));
        app.apply(Action::ApplyMogrToClip { clip_idx: 2, template_idx: 0 });
        assert_eq!(app.mogr_applied_clips.len(), 1);
        app.apply(Action::DetachMogrFromClip { clip_idx: 2 });
        assert_eq!(app.mogr_applied_clips.len(), 0);
        // Detaching a clip with no mogr is a no-op.
        app.apply(Action::DetachMogrFromClip { clip_idx: 99 });
        assert_eq!(app.mogr_applied_clips.len(), 0);
    }

    #[test]
    fn test_mogr_library_toggle() {
        let mut app = App::new();
        assert!(!app.mogr_library_open);
        app.apply(Action::ToggleMogrLibrary);
        assert!(app.mogr_library_open);
        app.apply(Action::ToggleMogrLibrary);
        assert!(!app.mogr_library_open);
    }

    #[test]
    fn test_seq_resolution_min_1() {
        let mut app = App::new();
        app.apply(Action::SetSeqResolution { w: 0, h: 0 });
        assert_eq!(app.sequence_settings.width, 1);
        assert_eq!(app.sequence_settings.height, 1);
    }

    #[test]
    fn test_seq_audio_channels_clamp() {
        let mut app = App::new();
        app.apply(Action::SetSeqAudioChannels(0));
        assert_eq!(app.sequence_settings.audio_channels, 1);
        app.apply(Action::SetSeqAudioChannels(10));
        assert_eq!(app.sequence_settings.audio_channels, 8);
    }

    #[test]
    fn test_seq_frame_rate_set() {
        let mut app = App::new();
        assert_eq!(app.sequence_settings.frame_rate, SeqFrameRate::Fps29_97);
        app.apply(Action::SetSeqFrameRate(SeqFrameRate::Fps60));
        assert_eq!(app.sequence_settings.frame_rate, SeqFrameRate::Fps60);
        app.apply(Action::SetSeqFrameRate(SeqFrameRate::Fps24));
        assert_eq!(app.sequence_settings.frame_rate, SeqFrameRate::Fps24);
    }

    #[test]
    fn test_nest_adds_sequence() {
        let mut app = App::new();
        assert_eq!(app.nested_sequences.len(), 0);
        app.apply(Action::NestSelectedClipsB9 { name: "MyNest".to_string() });
        assert_eq!(app.nested_sequences.len(), 1);
        assert_eq!(app.nested_sequences[0].name, "MyNest");
    }

    #[test]
    fn test_unnest_removes() {
        let mut app = App::new();
        app.apply(Action::NestSelectedClipsB9 { name: "Nest1".to_string() });
        app.apply(Action::NestSelectedClipsB9 { name: "Nest2".to_string() });
        assert_eq!(app.nested_sequences.len(), 2);
        app.apply(Action::UnnestSequence(0));
        assert_eq!(app.nested_sequences.len(), 1);
        assert_eq!(app.nested_sequences[0].name, "Nest2");
        // Out-of-bounds is a no-op.
        app.apply(Action::UnnestSequence(99));
        assert_eq!(app.nested_sequences.len(), 1);
    }

    #[test]
    fn test_enter_exit_nested() {
        let mut app = App::new();
        app.apply(Action::NestSelectedClipsB9 { name: "N".to_string() });
        assert!(app.active_nested_seq.is_none());
        app.apply(Action::EnterNestedSequence(0));
        assert_eq!(app.active_nested_seq, Some(0));
        app.apply(Action::ExitNestedSequence);
        assert!(app.active_nested_seq.is_none());
        // Out-of-bounds enter is a no-op.
        app.apply(Action::EnterNestedSequence(99));
        assert!(app.active_nested_seq.is_none());
    }

    #[test]
    fn test_rename_nested() {
        let mut app = App::new();
        app.apply(Action::NestSelectedClipsB9 { name: "OldName".to_string() });
        app.apply(Action::RenameNestedSequence { idx: 0, name: "MySeq".to_string() });
        assert_eq!(app.nested_sequences[0].name, "MySeq");
    }
}
