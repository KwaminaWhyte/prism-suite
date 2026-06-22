//! Spectral editing domain — FFT view, brush strokes, time-stretch jobs.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum SpectralTool {
    Select,
    Heal,
    Erase,
    Amplify,
}

#[derive(Clone, Debug)]
pub struct SpectralBrushStroke {
    pub id: usize,
    pub clip_id: usize,
    pub freq_low: f32,
    pub freq_high: f32,
    pub time_start: f32,
    pub time_end: f32,
    pub tool: SpectralTool,
    pub strength: f32,
}

#[derive(Clone, Debug)]
pub struct SpectralStretchJob {
    pub id: usize,
    pub clip_id: usize,
    pub time_ratio: f32,
    pub preserve_pitch: bool,
    pub status_done: bool,
}

#[derive(Clone, Debug)]
pub struct SpectralViewConfig {
    pub enabled: bool,
    pub fft_size: usize,
    pub window: String,
    pub color_map: String,
}

impl Default for SpectralViewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            fft_size: 2048,
            window: "Hann".to_string(),
            color_map: "Plasma".to_string(),
        }
    }
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_spectral(&mut self, action: Action) {
        match action {
            Action::ToggleSpectralView => {
                self.spectral_view.enabled = !self.spectral_view.enabled;
            }
            Action::SetSpectralFftSize { size } => {
                let valid = [512usize, 1024, 2048, 4096];
                self.spectral_view.fft_size = valid
                    .iter()
                    .copied()
                    .min_by_key(|&s| (s as i64 - size as i64).unsigned_abs())
                    .unwrap_or(2048);
            }
            Action::SetSpectralColorMap { name } => {
                self.spectral_view.color_map = name;
            }
            Action::ApplySpectralBrush {
                clip_id,
                freq_low,
                freq_high,
                time_start,
                time_end,
                tool,
                strength,
            } => {
                let id = self.next_spectral_brush_id;
                self.next_spectral_brush_id += 1;
                self.spectral_strokes.push(SpectralBrushStroke {
                    id,
                    clip_id,
                    freq_low,
                    freq_high,
                    time_start,
                    time_end,
                    tool,
                    strength,
                });
            }
            Action::UndoLastSpectralBrush => {
                self.spectral_strokes.pop();
            }
            Action::QueueSpectralStretch { clip_id, time_ratio, preserve_pitch } => {
                let id = self.next_spectral_stretch_id;
                self.next_spectral_stretch_id += 1;
                self.spectral_stretch_jobs.push(SpectralStretchJob {
                    id,
                    clip_id,
                    time_ratio,
                    preserve_pitch,
                    status_done: false,
                });
            }
            Action::CompleteSpectralStretch { job_id } => {
                if let Some(job) = self.spectral_stretch_jobs.iter_mut().find(|j| j.id == job_id) {
                    job.status_done = true;
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn toggle_spectral_view_enables() {
        let mut app = fresh();
        assert!(!app.spectral_view.enabled);
        app.apply(Action::ToggleSpectralView);
        assert!(app.spectral_view.enabled);
    }

    #[test]
    fn toggle_spectral_view_disables() {
        let mut app = fresh();
        app.apply(Action::ToggleSpectralView);
        app.apply(Action::ToggleSpectralView);
        assert!(!app.spectral_view.enabled);
    }

    #[test]
    fn set_spectral_fft_size_exact() {
        let mut app = fresh();
        app.apply(Action::SetSpectralFftSize { size: 1024 });
        assert_eq!(app.spectral_view.fft_size, 1024);
    }

    #[test]
    fn set_spectral_fft_size_snaps_to_nearest() {
        let mut app = fresh();
        app.apply(Action::SetSpectralFftSize { size: 700 });
        // 700 is closer to 512 (diff=188) than 1024 (diff=324)
        assert_eq!(app.spectral_view.fft_size, 512);
    }

    #[test]
    fn set_spectral_color_map() {
        let mut app = fresh();
        app.apply(Action::SetSpectralColorMap { name: "Viridis".to_string() });
        assert_eq!(app.spectral_view.color_map, "Viridis");
    }

    #[test]
    fn apply_spectral_brush_adds_stroke() {
        let mut app = fresh();
        app.apply(Action::ApplySpectralBrush {
            clip_id: 1,
            freq_low: 200.0,
            freq_high: 4000.0,
            time_start: 0.0,
            time_end: 1.0,
            tool: SpectralTool::Heal,
            strength: 0.8,
        });
        assert_eq!(app.spectral_strokes.len(), 1);
        assert_eq!(app.spectral_strokes[0].tool, SpectralTool::Heal);
    }

    #[test]
    fn apply_multiple_brushes_unique_ids() {
        let mut app = fresh();
        for _ in 0..3 {
            app.apply(Action::ApplySpectralBrush {
                clip_id: 1, freq_low: 0.0, freq_high: 1000.0,
                time_start: 0.0, time_end: 1.0,
                tool: SpectralTool::Erase, strength: 1.0,
            });
        }
        let ids: Vec<usize> = app.spectral_strokes.iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn undo_last_spectral_brush_removes_last() {
        let mut app = fresh();
        app.apply(Action::ApplySpectralBrush {
            clip_id: 1, freq_low: 0.0, freq_high: 1000.0,
            time_start: 0.0, time_end: 1.0,
            tool: SpectralTool::Select, strength: 1.0,
        });
        app.apply(Action::UndoLastSpectralBrush);
        assert!(app.spectral_strokes.is_empty());
    }

    #[test]
    fn queue_spectral_stretch_job() {
        let mut app = fresh();
        app.apply(Action::QueueSpectralStretch { clip_id: 2, time_ratio: 1.5, preserve_pitch: true });
        assert_eq!(app.spectral_stretch_jobs.len(), 1);
        assert!(!app.spectral_stretch_jobs[0].status_done);
        assert!(app.spectral_stretch_jobs[0].preserve_pitch);
    }

    #[test]
    fn complete_spectral_stretch_marks_done() {
        let mut app = fresh();
        app.apply(Action::QueueSpectralStretch { clip_id: 2, time_ratio: 1.2, preserve_pitch: false });
        let jid = app.spectral_stretch_jobs[0].id;
        app.apply(Action::CompleteSpectralStretch { job_id: jid });
        assert!(app.spectral_stretch_jobs[0].status_done);
    }

    #[test]
    fn spectral_view_defaults() {
        let app = fresh();
        assert_eq!(app.spectral_view.fft_size, 2048);
        assert_eq!(app.spectral_view.color_map, "Plasma");
        assert_eq!(app.spectral_view.window, "Hann");
    }
}
