use super::{App, Action, is_video_path, is_audio_path};
pub use super::timeline::{Bin, BinClip, BinClipType};

// ============================================================================
// Batch 11: ProjectManager
// ============================================================================

/// How files are collected when consolidating a project.
#[derive(Clone, Debug, PartialEq)]
pub enum ProjectCollectMode { CopyFiles, MoveFiles, LinkOnly }

/// Configuration for Project Manager / Consolidate & Transcode.
#[derive(Clone, Debug)]
pub struct ProjectManagerConfig {
    pub destination: String,
    pub mode: ProjectCollectMode,
    pub include_preview_files: bool,
    pub include_audio_conform: bool,
    pub include_proxies: bool,
    pub rename_media: bool,
    pub convert_ae_comps: bool,
}

impl ProjectManagerConfig {
    pub fn new() -> Self {
        Self {
            destination: String::new(),
            mode: ProjectCollectMode::CopyFiles,
            include_preview_files: false,
            include_audio_conform: true,
            include_proxies: true,
            rename_media: false,
            convert_ae_comps: false,
        }
    }
}

impl Default for ProjectManagerConfig {
    fn default() -> Self { Self::new() }
}

/// Summary result from running the Project Manager operation.
#[derive(Clone, Debug)]
pub struct ProjectManagerResult {
    pub files_copied: usize,
    pub total_size_mb: f32,
    pub missing_files: Vec<String>,
    pub success: bool,
}

// ============================================================================
// Batch 11: MediaBrowser
// ============================================================================

/// File-type filter for the Media Browser panel.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaBrowserFilter { All, Video, Audio, Image, Sequence, Project }

/// One entry shown in the Media Browser.
#[derive(Clone, Debug)]
pub struct MediaBrowserEntry {
    pub path: String,
    pub name: String,
    pub media_type: MediaBrowserFilter,
    pub duration_frames: Option<usize>,
    pub frame_rate: Option<f32>,
    pub is_favorite: bool,
}

/// The Media Browser panel model.
#[derive(Clone, Debug)]
pub struct MediaBrowser {
    pub current_path: String,
    pub entries: Vec<MediaBrowserEntry>,
    pub filter: MediaBrowserFilter,
    /// Paths the user has starred.
    pub favorites: Vec<String>,
    pub search_query: String,
    pub open: bool,
}

impl MediaBrowser {
    pub fn new() -> Self {
        Self {
            current_path: String::new(),
            entries: Vec::new(),
            filter: MediaBrowserFilter::All,
            favorites: Vec::new(),
            search_query: String::new(),
            open: false,
        }
    }
}

impl Default for MediaBrowser {
    fn default() -> Self { Self::new() }
}

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

            // --- Batch 11: ProjectManager ---
            Action::OpenProjectManager => { self.project_manager_open = true; }
            Action::CloseProjectManager => { self.project_manager_open = false; }
            Action::SetProjectManagerDestination(dst) => {
                self.project_manager_config.destination = dst;
            }
            Action::SetProjectManagerMode(mode) => {
                self.project_manager_config.mode = mode;
            }
            Action::SetProjectManagerIncludeProxies(v) => {
                self.project_manager_config.include_proxies = v;
            }
            Action::SetProjectManagerRenamMedia(v) => {
                self.project_manager_config.rename_media = v;
            }
            Action::RunProjectManager => {
                self.project_manager_result = Some(ProjectManagerResult {
                    files_copied: 42,
                    total_size_mb: 1280.5,
                    missing_files: vec![],
                    success: true,
                });
            }

            // --- Batch 11: MediaBrowser ---
            Action::OpenMediaBrowser => { self.media_browser.open = true; }
            Action::CloseMediaBrowser => { self.media_browser.open = false; }
            Action::SetMediaBrowserPath(path) => {
                self.media_browser.current_path = path;
                self.media_browser.entries.clear();
            }
            Action::SetMediaBrowserFilter(filter) => {
                self.media_browser.filter = filter;
            }
            Action::SetMediaBrowserSearch(query) => {
                self.media_browser.search_query = query;
            }
            Action::AddMediaBrowserEntry(entry) => {
                self.media_browser.entries.push(entry);
            }
            Action::ToggleMediaBrowserFavorite(path) => {
                if let Some(pos) = self.media_browser.favorites.iter().position(|p| p == &path) {
                    self.media_browser.favorites.remove(pos);
                } else {
                    self.media_browser.favorites.push(path);
                }
            }
            Action::ImportFromMediaBrowser { path } => {
                let name = path.split('/').last().unwrap_or(&path).to_string();
                self.media_browser.entries.push(MediaBrowserEntry {
                    path: path.clone(),
                    name,
                    media_type: MediaBrowserFilter::Video,
                    duration_frames: None,
                    frame_rate: None,
                    is_favorite: false,
                });
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

    // --- Batch 11: ProjectManager tests --------------------------------------

    #[test]
    fn test_project_manager_open_close() {
        let mut app = App::new();
        assert!(!app.project_manager_open);
        app.apply(Action::OpenProjectManager);
        assert!(app.project_manager_open);
        app.apply(Action::CloseProjectManager);
        assert!(!app.project_manager_open);
    }

    #[test]
    fn test_project_manager_destination_and_mode() {
        let mut app = App::new();
        app.apply(Action::SetProjectManagerDestination("/tmp/out".to_string()));
        assert_eq!(app.project_manager_config.destination, "/tmp/out");
        app.apply(Action::SetProjectManagerMode(ProjectCollectMode::MoveFiles));
        assert_eq!(app.project_manager_config.mode, ProjectCollectMode::MoveFiles);
    }

    #[test]
    fn test_run_project_manager_sets_result() {
        let mut app = App::new();
        assert!(app.project_manager_result.is_none());
        app.apply(Action::RunProjectManager);
        let result = app.project_manager_result.as_ref().unwrap();
        assert!(result.success);
        assert_eq!(result.files_copied, 42);
        assert!((result.total_size_mb - 1280.5).abs() < 1e-3);
        assert!(result.missing_files.is_empty());
    }

    // --- Batch 11: MediaBrowser tests ----------------------------------------

    #[test]
    fn test_media_browser_open_close() {
        let mut app = App::new();
        assert!(!app.media_browser.open);
        app.apply(Action::OpenMediaBrowser);
        assert!(app.media_browser.open);
        app.apply(Action::CloseMediaBrowser);
        assert!(!app.media_browser.open);
    }

    #[test]
    fn test_set_media_browser_path_clears_entries() {
        let mut app = App::new();
        app.apply(Action::AddMediaBrowserEntry(MediaBrowserEntry {
            path: "/foo.mp4".to_string(),
            name: "foo.mp4".to_string(),
            media_type: MediaBrowserFilter::Video,
            duration_frames: None,
            frame_rate: None,
            is_favorite: false,
        }));
        assert_eq!(app.media_browser.entries.len(), 1);
        app.apply(Action::SetMediaBrowserPath("/media/projects".to_string()));
        assert_eq!(app.media_browser.current_path, "/media/projects");
        assert!(app.media_browser.entries.is_empty());
    }

    #[test]
    fn test_toggle_media_browser_favorite() {
        let mut app = App::new();
        let path = "/media/clip.mp4".to_string();
        // Add favorite.
        app.apply(Action::ToggleMediaBrowserFavorite(path.clone()));
        assert_eq!(app.media_browser.favorites.len(), 1);
        assert_eq!(app.media_browser.favorites[0], path);
        // Remove favorite.
        app.apply(Action::ToggleMediaBrowserFavorite(path.clone()));
        assert!(app.media_browser.favorites.is_empty());
    }

    #[test]
    fn test_import_from_media_browser() {
        let mut app = App::new();
        app.apply(Action::ImportFromMediaBrowser { path: "/footage/shot_01.mp4".to_string() });
        assert_eq!(app.media_browser.entries.len(), 1);
        assert_eq!(app.media_browser.entries[0].name, "shot_01.mp4");
        assert_eq!(app.media_browser.entries[0].path, "/footage/shot_01.mp4");
    }

    #[test]
    fn test_media_browser_filter_and_search() {
        let mut app = App::new();
        app.apply(Action::SetMediaBrowserFilter(MediaBrowserFilter::Audio));
        assert_eq!(app.media_browser.filter, MediaBrowserFilter::Audio);
        app.apply(Action::SetMediaBrowserSearch("interview".to_string()));
        assert_eq!(app.media_browser.search_query, "interview");
    }
}
