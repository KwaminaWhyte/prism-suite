//! Lottie JSON builder, export format queues, JS runtime stub, web publish,
//! WebSocket live preview, and CRDT collaboration session stub.

use super::{App, Action};

// ── Lottie JSON builder ───────────────────────────────────────────────────────

/// Holds the result of a Lottie JSON build pass (string builder — no serde).
#[derive(Clone, Debug)]
pub struct LottieJsonBuilder {
    pub scene_id: Option<usize>,
    pub output_path: String,
    pub built: bool,
    pub json_preview: String,
}

impl LottieJsonBuilder {
    /// Produce a minimal Lottie v5.9 JSON string for preview/testing.
    pub fn preview(name: &str, fps: f32, dur: usize) -> String {
        format!(
            r#"{{"v":"5.9","fr":{fps},"ip":0,"op":{dur},"w":1920,"h":1080,"nm":"{name}","ddd":0,"assets":[],"layers":[]}}"#,
            fps = fps,
            dur = dur,
            name = name
        )
    }
}

// ── Lottie import parser stub ─────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum WebLottieImportStatus {
    Idle,
    Parsing,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct WebLottieImportSession {
    pub id: usize,
    pub source_path: String,
    pub status: WebLottieImportStatus,
    pub layers_created: usize,
    pub error: Option<String>,
}

// ── Per-format media export queue ─────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum MediaExportFormat {
    Mp4,
    Gif,
    WebM,
    Apng,
    Spritesheet,
    PngSequence,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaExportStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct MediaExportJob {
    pub id: usize,
    pub format: MediaExportFormat,
    pub output_path: String,
    pub fps: f32,
    pub start_frame: usize,
    pub end_frame: usize,
    pub scale: f32,
    pub status: MediaExportStatus,
    pub progress: f32,
    pub error: Option<String>,
}

// ── JS runtime bridge stub ────────────────────────────────────────────────────

/// Configuration for the Lottie-web compatible JS runtime bridge.
#[derive(Clone, Debug)]
pub struct JsRuntimeConfig {
    pub enabled: bool,
    pub bundle_path: String,
    pub auto_reload: bool,
}

impl Default for JsRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bundle_path: String::new(),
            auto_reload: true,
        }
    }
}

// ── Web publish ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum WebPublishStatus {
    Idle,
    Bundling,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct WebPublishJob {
    pub id: usize,
    pub output_dir: String,
    pub include_player: bool,
    pub minify: bool,
    pub status: WebPublishStatus,
}

// ── WebSocket live preview ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct WsLivePreviewConfig {
    pub enabled: bool,
    pub port: u16,
    pub connected_clients: usize,
}

impl Default for WsLivePreviewConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8765,
            connected_clients: 0,
        }
    }
}

// ── CRDT collaboration session stub ──────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum CollabStatus {
    Offline,
    Connecting,
    Connected,
}

#[derive(Clone, Debug)]
pub struct CollabSession {
    pub enabled: bool,
    pub room_id: String,
    pub peer_count: usize,
    pub status: CollabStatus,
}

impl Default for CollabSession {
    fn default() -> Self {
        Self {
            enabled: false,
            room_id: String::new(),
            peer_count: 0,
            status: CollabStatus::Offline,
        }
    }
}

// ── App impl ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_export_web(&mut self, action: &Action) {
        match action {
            Action::BuildLottieJson { scene_id, output_path } => {
                let preview = LottieJsonBuilder::preview(
                    &self.document.name,
                    self.document.fps as f32,
                    self.document.duration_frames,
                );
                self.lottie_builds.push(LottieJsonBuilder {
                    scene_id: Some(*scene_id),
                    output_path: output_path.clone(),
                    built: true,
                    json_preview: preview,
                });
                self.next_lottie_build_id += 1;
            }
            Action::StartWebLottieImport { source_path } => {
                let id = self.next_lottie_import_id;
                self.next_lottie_import_id += 1;
                self.lottie_imports.push(WebLottieImportSession {
                    id,
                    source_path: source_path.clone(),
                    status: WebLottieImportStatus::Parsing,
                    layers_created: 0,
                    error: None,
                });
            }
            Action::CompleteWebLottieImport { session_id, layers_created } => {
                if let Some(s) = self.lottie_imports.iter_mut().find(|s| s.id == *session_id) {
                    s.status = WebLottieImportStatus::Done;
                    s.layers_created = *layers_created;
                }
            }
            Action::FailWebLottieImport { session_id, error } => {
                if let Some(s) = self.lottie_imports.iter_mut().find(|s| s.id == *session_id) {
                    s.status = WebLottieImportStatus::Error;
                    s.error = Some(error.clone());
                }
            }
            Action::QueueMediaExport { format, output_path, fps, start_frame, end_frame, scale } => {
                let id = self.next_media_export_id;
                self.next_media_export_id += 1;
                self.media_export_jobs.push(MediaExportJob {
                    id,
                    format: format.clone(),
                    output_path: output_path.clone(),
                    fps: *fps,
                    start_frame: *start_frame,
                    end_frame: *end_frame,
                    scale: *scale,
                    status: MediaExportStatus::Queued,
                    progress: 0.0,
                    error: None,
                });
            }
            Action::StartMediaExport { job_id } => {
                if let Some(j) = self.media_export_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = MediaExportStatus::Running;
                }
            }
            Action::UpdateMediaExportProgress { job_id, progress } => {
                if let Some(j) = self.media_export_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.progress = progress.clamp(0.0, 1.0);
                }
            }
            Action::CompleteMediaExport { job_id } => {
                if let Some(j) = self.media_export_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = MediaExportStatus::Done;
                    j.progress = 1.0;
                }
            }
            Action::FailMediaExport { job_id, error } => {
                if let Some(j) = self.media_export_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = MediaExportStatus::Error;
                    j.error = Some(error.clone());
                }
            }
            Action::SetJsRuntime { enabled, bundle_path, auto_reload } => {
                self.js_runtime.enabled = *enabled;
                self.js_runtime.bundle_path = bundle_path.clone();
                self.js_runtime.auto_reload = *auto_reload;
            }
            Action::StartWebPublish { output_dir, include_player, minify } => {
                let id = self.next_web_publish_id;
                self.next_web_publish_id += 1;
                self.web_publish_jobs.push(WebPublishJob {
                    id,
                    output_dir: output_dir.clone(),
                    include_player: *include_player,
                    minify: *minify,
                    status: WebPublishStatus::Bundling,
                });
            }
            Action::CompleteWebPublish { job_id } => {
                if let Some(j) = self.web_publish_jobs.iter_mut().find(|j| j.id == *job_id) {
                    j.status = WebPublishStatus::Done;
                }
            }
            Action::SetWsLivePreview { enabled, port } => {
                self.ws_live_preview.enabled = *enabled;
                self.ws_live_preview.port = *port;
            }
            Action::SetWsClientCount { count } => {
                self.ws_live_preview.connected_clients = *count;
            }
            Action::StartCollabSession { room_id } => {
                self.collab_session.room_id = room_id.clone();
                self.collab_session.status = CollabStatus::Connecting;
                self.collab_session.enabled = true;
            }
            Action::CollabSessionConnected { peer_count } => {
                self.collab_session.status = CollabStatus::Connected;
                self.collab_session.peer_count = *peer_count;
            }
            Action::StopCollabSession => {
                self.collab_session = CollabSession::default();
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::{App, Action};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_build_lottie_json() {
        let mut a = app();
        a.apply(Action::BuildLottieJson { scene_id: 0, output_path: "/tmp/out.json".to_string() });
        assert_eq!(a.lottie_builds.len(), 1);
        assert!(a.lottie_builds[0].built);
        assert!(a.lottie_builds[0].json_preview.contains("5.9"));
    }

    #[test]
    fn test_lottie_json_preview_format() {
        let preview = LottieJsonBuilder::preview("MyAnim", 24.0, 120);
        assert!(preview.contains("\"fr\":24"));
        assert!(preview.contains("\"op\":120"));
        assert!(preview.contains("MyAnim"));
    }

    #[test]
    fn test_start_lottie_import() {
        let mut a = app();
        a.apply(Action::StartWebLottieImport { source_path: "/tmp/input.json".to_string() });
        assert_eq!(a.lottie_imports.len(), 1);
        assert_eq!(a.lottie_imports[0].status, WebLottieImportStatus::Parsing);
    }

    #[test]
    fn test_complete_lottie_import() {
        let mut a = app();
        a.apply(Action::StartWebLottieImport { source_path: "/tmp/input.json".to_string() });
        let sid = a.lottie_imports[0].id;
        a.apply(Action::CompleteWebLottieImport { session_id: sid, layers_created: 5 });
        assert_eq!(a.lottie_imports[0].status, WebLottieImportStatus::Done);
        assert_eq!(a.lottie_imports[0].layers_created, 5);
    }

    #[test]
    fn test_fail_lottie_import() {
        let mut a = app();
        a.apply(Action::StartWebLottieImport { source_path: "/bad.json".to_string() });
        let sid = a.lottie_imports[0].id;
        a.apply(Action::FailWebLottieImport { session_id: sid, error: "parse error".to_string() });
        assert_eq!(a.lottie_imports[0].status, WebLottieImportStatus::Error);
        assert_eq!(a.lottie_imports[0].error.as_deref(), Some("parse error"));
    }

    #[test]
    fn test_queue_media_export() {
        let mut a = app();
        a.apply(Action::QueueMediaExport {
            format: MediaExportFormat::Gif,
            output_path: "/tmp/out.gif".to_string(),
            fps: 24.0,
            start_frame: 0,
            end_frame: 100,
            scale: 1.0,
        });
        assert_eq!(a.media_export_jobs.len(), 1);
        assert_eq!(a.media_export_jobs[0].status, MediaExportStatus::Queued);
        assert_eq!(a.media_export_jobs[0].format, MediaExportFormat::Gif);
    }

    #[test]
    fn test_media_export_lifecycle() {
        let mut a = app();
        a.apply(Action::QueueMediaExport {
            format: MediaExportFormat::Mp4,
            output_path: "/tmp/out.mp4".to_string(),
            fps: 30.0,
            start_frame: 0,
            end_frame: 60,
            scale: 1.0,
        });
        let jid = a.media_export_jobs[0].id;
        a.apply(Action::StartMediaExport { job_id: jid });
        assert_eq!(a.media_export_jobs[0].status, MediaExportStatus::Running);
        a.apply(Action::UpdateMediaExportProgress { job_id: jid, progress: 0.5 });
        assert!((a.media_export_jobs[0].progress - 0.5).abs() < 0.001);
        a.apply(Action::CompleteMediaExport { job_id: jid });
        assert_eq!(a.media_export_jobs[0].status, MediaExportStatus::Done);
        assert!((a.media_export_jobs[0].progress - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fail_media_export() {
        let mut a = app();
        a.apply(Action::QueueMediaExport {
            format: MediaExportFormat::WebM,
            output_path: "/tmp/out.webm".to_string(),
            fps: 25.0,
            start_frame: 0,
            end_frame: 50,
            scale: 0.5,
        });
        let jid = a.media_export_jobs[0].id;
        a.apply(Action::FailMediaExport { job_id: jid, error: "codec error".to_string() });
        assert_eq!(a.media_export_jobs[0].status, MediaExportStatus::Error);
        assert_eq!(a.media_export_jobs[0].error.as_deref(), Some("codec error"));
    }

    #[test]
    fn test_progress_clamped() {
        let mut a = app();
        a.apply(Action::QueueMediaExport {
            format: MediaExportFormat::PngSequence,
            output_path: "/tmp/frames/".to_string(),
            fps: 12.0,
            start_frame: 0,
            end_frame: 24,
            scale: 2.0,
        });
        let jid = a.media_export_jobs[0].id;
        a.apply(Action::UpdateMediaExportProgress { job_id: jid, progress: 2.5 });
        assert!((a.media_export_jobs[0].progress - 1.0).abs() < 0.001);
        a.apply(Action::UpdateMediaExportProgress { job_id: jid, progress: -1.0 });
        assert!((a.media_export_jobs[0].progress - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_set_js_runtime() {
        let mut a = app();
        a.apply(Action::SetJsRuntime {
            enabled: true,
            bundle_path: "/dist/player.js".to_string(),
            auto_reload: false,
        });
        assert!(a.js_runtime.enabled);
        assert_eq!(a.js_runtime.bundle_path, "/dist/player.js");
        assert!(!a.js_runtime.auto_reload);
    }

    #[test]
    fn test_web_publish_lifecycle() {
        let mut a = app();
        a.apply(Action::StartWebPublish {
            output_dir: "/dist/web".to_string(),
            include_player: true,
            minify: true,
        });
        assert_eq!(a.web_publish_jobs.len(), 1);
        assert_eq!(a.web_publish_jobs[0].status, WebPublishStatus::Bundling);
        let jid = a.web_publish_jobs[0].id;
        a.apply(Action::CompleteWebPublish { job_id: jid });
        assert_eq!(a.web_publish_jobs[0].status, WebPublishStatus::Done);
    }

    #[test]
    fn test_ws_live_preview() {
        let mut a = app();
        a.apply(Action::SetWsLivePreview { enabled: true, port: 9000 });
        assert!(a.ws_live_preview.enabled);
        assert_eq!(a.ws_live_preview.port, 9000);
        a.apply(Action::SetWsClientCount { count: 3 });
        assert_eq!(a.ws_live_preview.connected_clients, 3);
    }

    #[test]
    fn test_collab_session_lifecycle() {
        let mut a = app();
        assert_eq!(a.collab_session.status, CollabStatus::Offline);
        a.apply(Action::StartCollabSession { room_id: "abc123".to_string() });
        assert_eq!(a.collab_session.status, CollabStatus::Connecting);
        assert!(a.collab_session.enabled);
        a.apply(Action::CollabSessionConnected { peer_count: 2 });
        assert_eq!(a.collab_session.status, CollabStatus::Connected);
        assert_eq!(a.collab_session.peer_count, 2);
        a.apply(Action::StopCollabSession);
        assert_eq!(a.collab_session.status, CollabStatus::Offline);
        assert!(!a.collab_session.enabled);
    }

    #[test]
    fn test_multiple_export_formats() {
        let mut a = app();
        let formats = vec![
            MediaExportFormat::Mp4,
            MediaExportFormat::Gif,
            MediaExportFormat::Apng,
            MediaExportFormat::Spritesheet,
        ];
        for fmt in &formats {
            a.apply(Action::QueueMediaExport {
                format: fmt.clone(),
                output_path: "/tmp/x".to_string(),
                fps: 24.0,
                start_frame: 0,
                end_frame: 10,
                scale: 1.0,
            });
        }
        assert_eq!(a.media_export_jobs.len(), 4);
    }

    #[test]
    fn test_ids_increment() {
        let mut a = app();
        a.apply(Action::StartWebLottieImport { source_path: "/a.json".to_string() });
        a.apply(Action::StartWebLottieImport { source_path: "/b.json".to_string() });
        assert_ne!(a.lottie_imports[0].id, a.lottie_imports[1].id);
    }

    #[test]
    fn test_web_publish_multiple() {
        let mut a = app();
        a.apply(Action::StartWebPublish { output_dir: "/d1".to_string(), include_player: true, minify: false });
        a.apply(Action::StartWebPublish { output_dir: "/d2".to_string(), include_player: false, minify: true });
        assert_eq!(a.web_publish_jobs.len(), 2);
        assert_ne!(a.web_publish_jobs[0].id, a.web_publish_jobs[1].id);
    }
}
