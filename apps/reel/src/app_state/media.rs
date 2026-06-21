use super::{App, Action, is_video_path, is_audio_path};
pub use super::timeline::{Bin, BinClip, BinClipType};

pub trait AppMediaExt {
    fn apply_media(&mut self, action: Action);
}

impl AppMediaExt for App {
    fn apply_media(&mut self, action: Action) {
        match action {
            // --- Bins (Media Browser) ---
            Action::ToggleBins => { self.bins_open = !self.bins_open; }
            Action::AddBin(name) => { self.bins.push(Bin { name, clips: Vec::new() }); }
            Action::SelectBin(idx) => {
                if idx < self.bins.len() {
                    self.selected_bin = idx;
                    self.selected_bin_clip = None;
                }
            }
            Action::ImportToBin { bin_idx, path } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    let name = path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "media".into());
                    let clip_type = if is_video_path(&path) {
                        BinClipType::Video
                    } else if is_audio_path(&path) {
                        BinClipType::Audio
                    } else {
                        BinClipType::Image
                    };
                    bin.clips.push(BinClip { name, path, duration: 0.0, clip_type });
                }
            }
            Action::RemoveFromBin { bin_idx, clip_idx } => {
                if let Some(bin) = self.bins.get_mut(bin_idx) {
                    if clip_idx < bin.clips.len() {
                        bin.clips.remove(clip_idx);
                        if self.selected_bin_clip == Some(clip_idx) {
                            self.selected_bin_clip = None;
                        }
                    }
                }
            }
            Action::SelectBinClip { bin_idx, clip_idx } => {
                if bin_idx < self.bins.len() && clip_idx < self.bins[bin_idx].clips.len() {
                    self.selected_bin = bin_idx;
                    self.selected_bin_clip = Some(clip_idx);
                }
            }
            Action::InsertClipFromBin { bin_clip_idx, track_idx, at_t } => {
                let bin_clip = self.bins.get(self.selected_bin)
                    .and_then(|b| b.clips.get(bin_clip_idx))
                    .cloned();
                if let Some(bc) = bin_clip {
                    if let Some(clip) = self.build_imported_clip(&bc.path) {
                        let mut clip = clip;
                        clip.track = track_idx.min(self.project.tracks.len().saturating_sub(1));
                        clip.start = self.snap_to_frame(at_t);
                        self.project.clips.push(clip);
                        self.selected = Some(self.project.clips.len() - 1);
                        self.host.mark_dirty();
                    }
                }
            }

            // --- Dual Viewer (Source Monitor) ---
            Action::ToggleDualViewer => { self.dual_viewer = !self.dual_viewer; }
            Action::SeekSource(t) => {
                self.source_playhead = t.clamp(0.0, self.project.duration);
            }
            Action::ToggleSourcePlay => { self.source_playing = !self.source_playing; }
            Action::SetSourceIn(t) => { self.source_in = t.clamp(0.0, self.project.duration); }
            Action::SetSourceOut(t) => { self.source_out = t.clamp(0.0, self.project.duration); }
            Action::InsertFromSource { clip_id, in_t, out_t } => {
                log::info!("reel-gpui: InsertFromSource clip={clip_id} in={in_t:.2} out={out_t:.2}");
            }

            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Action;

    #[test]
    fn test_add_bin_and_select() {
        let mut app = App::new();
        let initial_count = app.bins.len();
        app.apply(Action::AddBin("Footage".to_string()));
        assert_eq!(app.bins.len(), initial_count + 1);
        assert_eq!(app.bins.last().unwrap().name, "Footage");

        app.apply(Action::SelectBin(initial_count));
        assert_eq!(app.selected_bin, initial_count);
        assert!(app.selected_bin_clip.is_none());

        // Out-of-bounds select is a no-op.
        app.apply(Action::SelectBin(999));
        assert_eq!(app.selected_bin, initial_count);
    }

    #[test]
    fn test_toggle_bins_panel() {
        let mut app = App::new();
        assert!(!app.bins_open);
        app.apply(Action::ToggleBins);
        assert!(app.bins_open);
        app.apply(Action::ToggleBins);
        assert!(!app.bins_open);
    }

    #[test]
    fn test_import_to_bin_classifies_file_type() {
        let mut app = App::new();
        // Import a video file.
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/footage/scene.mp4"),
        });
        assert_eq!(app.bins[0].clips.len(), 1);
        assert_eq!(app.bins[0].clips[0].clip_type, BinClipType::Video);
        assert_eq!(app.bins[0].clips[0].name, "scene");

        // Import an audio file.
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/audio/music.wav"),
        });
        assert_eq!(app.bins[0].clips[1].clip_type, BinClipType::Audio);

        // Import an image file.
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/images/bg.png"),
        });
        assert_eq!(app.bins[0].clips[2].clip_type, BinClipType::Image);
    }

    #[test]
    fn test_remove_from_bin() {
        let mut app = App::new();
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/footage/a.mp4"),
        });
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/footage/b.mp4"),
        });
        assert_eq!(app.bins[0].clips.len(), 2);

        app.apply(Action::SelectBinClip { bin_idx: 0, clip_idx: 0 });
        assert_eq!(app.selected_bin_clip, Some(0));

        app.apply(Action::RemoveFromBin { bin_idx: 0, clip_idx: 0 });
        assert_eq!(app.bins[0].clips.len(), 1);
        // Selection cleared since the selected clip was removed.
        assert!(app.selected_bin_clip.is_none());

        // Out-of-bounds remove is a no-op.
        app.apply(Action::RemoveFromBin { bin_idx: 0, clip_idx: 999 });
        assert_eq!(app.bins[0].clips.len(), 1);
    }

    #[test]
    fn test_select_bin_clip() {
        let mut app = App::new();
        app.apply(Action::ImportToBin {
            bin_idx: 0,
            path: std::path::PathBuf::from("/footage/clip.mov"),
        });
        app.apply(Action::SelectBinClip { bin_idx: 0, clip_idx: 0 });
        assert_eq!(app.selected_bin, 0);
        assert_eq!(app.selected_bin_clip, Some(0));

        // Out-of-bounds bin or clip is a no-op.
        app.apply(Action::SelectBinClip { bin_idx: 999, clip_idx: 0 });
        assert_eq!(app.selected_bin, 0);
        app.apply(Action::SelectBinClip { bin_idx: 0, clip_idx: 999 });
        assert_eq!(app.selected_bin_clip, Some(0));
    }

    #[test]
    fn test_dual_viewer_toggle() {
        let mut app = App::new();
        assert!(!app.dual_viewer);
        app.apply(Action::ToggleDualViewer);
        assert!(app.dual_viewer);
        app.apply(Action::ToggleDualViewer);
        assert!(!app.dual_viewer);
    }

    #[test]
    fn test_seek_source_clamps_to_duration() {
        let mut app = App::new();
        let dur = app.project.duration;
        app.apply(Action::SeekSource(dur + 100.0));
        assert!((app.source_playhead - dur).abs() < 1e-4);
        app.apply(Action::SeekSource(-5.0));
        assert!((app.source_playhead - 0.0).abs() < 1e-4);
        app.apply(Action::SeekSource(10.0));
        assert!((app.source_playhead - 10.0).abs() < 1e-4);
    }

    #[test]
    fn test_source_in_out_set() {
        let mut app = App::new();
        app.apply(Action::SetSourceIn(3.5));
        assert!((app.source_in - 3.5).abs() < 1e-5);
        app.apply(Action::SetSourceOut(8.0));
        assert!((app.source_out - 8.0).abs() < 1e-5);
        // Clamp to 0.
        app.apply(Action::SetSourceIn(-1.0));
        assert!((app.source_in - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_toggle_source_play() {
        let mut app = App::new();
        assert!(!app.source_playing);
        app.apply(Action::ToggleSourcePlay);
        assert!(app.source_playing);
        app.apply(Action::ToggleSourcePlay);
        assert!(!app.source_playing);
    }
}
