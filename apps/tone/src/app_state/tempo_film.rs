//! Tempo-film domain — tempo automation, time sig changes, video lock.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct TempoChange {
    pub id: usize,
    pub bar: u32,
    pub bpm: f32,
    pub smooth: bool,
}

#[derive(Clone, Debug)]
pub struct TimeSigChange2 {
    pub id: usize,
    pub bar: u32,
    pub num: u8,
    pub denom: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VideoLockStatus {
    Unlinked,
    Linked,
    Syncing,
}

#[derive(Clone, Debug)]
pub struct VideoFilmConfig {
    pub video_path: Option<String>,
    pub offset_frames: i32,
    pub fps: f32,
    pub lock_status: VideoLockStatus,
    pub playback_linked: bool,
}

impl Default for VideoFilmConfig {
    fn default() -> Self {
        Self {
            video_path: None,
            offset_frames: 0,
            fps: 24.0,
            lock_status: VideoLockStatus::Unlinked,
            playback_linked: false,
        }
    }
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_tempo_film(&mut self, action: Action) {
        match action {
            Action::AddTempoChange2 { bar, bpm, smooth } => {
                let id = self.next_tempo_change_id;
                self.next_tempo_change_id += 1;
                self.tempo_changes.push(TempoChange {
                    id,
                    bar,
                    bpm: bpm.clamp(20.0, 400.0),
                    smooth,
                });
            }
            Action::RemoveTempoChange { change_id } => {
                self.tempo_changes.retain(|c| c.id != change_id);
            }
            Action::SetTempoChangeBpm { change_id, bpm } => {
                if let Some(change) = self.tempo_changes.iter_mut().find(|c| c.id == change_id) {
                    change.bpm = bpm.clamp(20.0, 400.0);
                }
            }
            Action::AddTimeSigChange2 { bar, num, denom } => {
                let id = self.next_tsig_change_id;
                self.next_tsig_change_id += 1;
                self.tsig_changes.push(TimeSigChange2 {
                    id,
                    bar,
                    num,
                    denom,
                });
            }
            Action::RemoveTimeSigChange { change_id } => {
                self.tsig_changes.retain(|c| c.id != change_id);
            }
            Action::LinkVideo { path, fps } => {
                self.video_film.video_path = Some(path);
                self.video_film.fps = fps;
                self.video_film.lock_status = VideoLockStatus::Linked;
            }
            Action::UnlinkVideo => {
                self.video_film.video_path = None;
                self.video_film.lock_status = VideoLockStatus::Unlinked;
            }
            Action::SetVideoOffset { frames } => {
                self.video_film.offset_frames = frames;
            }
            Action::ToggleVideoPlaybackLink => {
                self.video_film.playback_linked = !self.video_film.playback_linked;
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
    fn add_tempo_change() {
        let mut app = fresh();
        app.apply(Action::AddTempoChange2 { bar: 8, bpm: 140.0, smooth: false });
        assert_eq!(app.tempo_changes.len(), 1);
        assert!((app.tempo_changes[0].bpm - 140.0).abs() < 0.001);
    }

    #[test]
    fn add_tempo_change_bpm_clamped_low() {
        let mut app = fresh();
        app.apply(Action::AddTempoChange2 { bar: 1, bpm: 5.0, smooth: false });
        assert!((app.tempo_changes[0].bpm - 20.0).abs() < 0.001);
    }

    #[test]
    fn add_tempo_change_bpm_clamped_high() {
        let mut app = fresh();
        app.apply(Action::AddTempoChange2 { bar: 1, bpm: 9999.0, smooth: false });
        assert!((app.tempo_changes[0].bpm - 400.0).abs() < 0.001);
    }

    #[test]
    fn remove_tempo_change() {
        let mut app = fresh();
        app.apply(Action::AddTempoChange2 { bar: 4, bpm: 120.0, smooth: true });
        let cid = app.tempo_changes[0].id;
        app.apply(Action::RemoveTempoChange { change_id: cid });
        assert!(app.tempo_changes.is_empty());
    }

    #[test]
    fn set_tempo_change_bpm() {
        let mut app = fresh();
        app.apply(Action::AddTempoChange2 { bar: 4, bpm: 120.0, smooth: false });
        let cid = app.tempo_changes[0].id;
        app.apply(Action::SetTempoChangeBpm { change_id: cid, bpm: 160.0 });
        assert!((app.tempo_changes[0].bpm - 160.0).abs() < 0.001);
    }

    #[test]
    fn add_time_sig_change() {
        let mut app = fresh();
        app.apply(Action::AddTimeSigChange2 { bar: 5, num: 3, denom: 4 });
        assert_eq!(app.tsig_changes.len(), 1);
        assert_eq!(app.tsig_changes[0].num, 3);
        assert_eq!(app.tsig_changes[0].denom, 4);
    }

    #[test]
    fn remove_time_sig_change() {
        let mut app = fresh();
        app.apply(Action::AddTimeSigChange2 { bar: 5, num: 6, denom: 8 });
        let cid = app.tsig_changes[0].id;
        app.apply(Action::RemoveTimeSigChange { change_id: cid });
        assert!(app.tsig_changes.is_empty());
    }

    #[test]
    fn link_video_sets_path_and_fps() {
        let mut app = fresh();
        app.apply(Action::LinkVideo { path: "/media/film.mp4".to_string(), fps: 29.97 });
        assert_eq!(app.video_film.video_path, Some("/media/film.mp4".to_string()));
        assert!((app.video_film.fps - 29.97).abs() < 0.01);
        assert_eq!(app.video_film.lock_status, VideoLockStatus::Linked);
    }

    #[test]
    fn unlink_video_clears_path() {
        let mut app = fresh();
        app.apply(Action::LinkVideo { path: "/media/film.mp4".to_string(), fps: 24.0 });
        app.apply(Action::UnlinkVideo);
        assert!(app.video_film.video_path.is_none());
        assert_eq!(app.video_film.lock_status, VideoLockStatus::Unlinked);
    }

    #[test]
    fn set_video_offset() {
        let mut app = fresh();
        app.apply(Action::SetVideoOffset { frames: -12 });
        assert_eq!(app.video_film.offset_frames, -12);
    }

    #[test]
    fn toggle_video_playback_link() {
        let mut app = fresh();
        assert!(!app.video_film.playback_linked);
        app.apply(Action::ToggleVideoPlaybackLink);
        assert!(app.video_film.playback_linked);
        app.apply(Action::ToggleVideoPlaybackLink);
        assert!(!app.video_film.playback_linked);
    }

    #[test]
    fn video_film_defaults() {
        let app = fresh();
        assert!(app.video_film.video_path.is_none());
        assert!((app.video_film.fps - 24.0).abs() < 0.001);
        assert_eq!(app.video_film.lock_status, VideoLockStatus::Unlinked);
    }
}
