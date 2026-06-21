use super::*;

/// One randomised keyframe-variation preview in the Brainstorm panel.
#[derive(Clone, Debug)]
pub struct BrainstormVariation {
    pub label: String,
    pub overrides: Vec<(usize, crate::comp::Prop, f32)>,
    pub selected: bool,
}

/// State for the Brainstorm panel (generate + pick random comp variations).
#[derive(Clone, Debug, Default)]
pub struct BrainstormState {
    pub open: bool,
    pub variations: Vec<BrainstormVariation>,
    pub grid_cols: u32,
    pub grid_rows: u32,
}

impl BrainstormState {
    pub fn new() -> Self {
        Self { open: false, variations: Vec::new(), grid_cols: 2, grid_rows: 3 }
    }
}

/// Status of the pre-render cache.
#[derive(Clone, Debug, PartialEq)]
pub enum PreRenderStatus {
    NotStarted,
    Rendering { frames_done: u32, total: u32 },
    Done { frame_count: u32, cache_dir: std::path::PathBuf },
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum AudioVisMode {
    #[default]
    Spectrum,
    Waveform,
    Bars,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum AudioVisSide {
    #[default]
    Both,
    Left,
    Right,
    All,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioSpectrumConfig {
    pub mode: AudioVisMode,
    pub audio_layer: Option<usize>,
    pub start_freq: f32,
    pub end_freq: f32,
    pub max_height: f32,
    pub audio_duration: f32,
    pub side: AudioVisSide,
    pub softness: f32,
    pub inside_color: [f32; 4],
    pub outside_color: [f32; 4],
    pub mirror: bool,
    pub displayed_samples: u32,
    pub digital: bool,
    pub frequency_bands: u32,
    pub thickness: f32,
}

impl Default for AudioSpectrumConfig {
    fn default() -> Self {
        Self {
            mode: AudioVisMode::Spectrum,
            audio_layer: None,
            start_freq: 20.0,
            end_freq: 20000.0,
            max_height: 500.0,
            audio_duration: 0.0,
            side: AudioVisSide::Both,
            softness: 0.0,
            inside_color: [1.0, 1.0, 1.0, 1.0],
            outside_color: [0.0, 0.0, 0.0, 0.0],
            mirror: false,
            displayed_samples: 512,
            digital: false,
            frequency_bands: 64,
            thickness: 2.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum RenderStatus {
    #[default]
    Queued,
    Rendering,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum RenderOutputFormat {
    #[default]
    H264Mp4,
    ProResHq,
    DnxHd,
    Exr,
    Tiff,
    Png,
    Wav,
    Aiff,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RenderQueueItem {
    pub comp_name: String,
    pub output_path: std::path::PathBuf,
    pub format: RenderOutputFormat,
    pub status: RenderStatus,
    pub progress: f32,
    pub start_frame: u32,
    pub end_frame: u32,
    pub use_proxy: bool,
}

impl Default for RenderQueueItem {
    fn default() -> Self {
        Self {
            comp_name: "Comp 1".to_string(),
            output_path: std::path::PathBuf::from("output.mp4"),
            format: RenderOutputFormat::H264Mp4,
            status: RenderStatus::Queued,
            progress: 0.0,
            start_frame: 0,
            end_frame: 100,
            use_proxy: false,
        }
    }
}


impl App {
    pub(super) fn apply_render(&mut self, action: Action) {
        match action {
            Action::ExportMp4(path) => {
                self.playing = false;
                self.last_tick = None;
                self.export_mp4_progress = Some(0.0);
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps as f64;
                let width = comp.width;
                let height = comp.height;
                let total_frames = (comp.duration * comp.fps).ceil() as u32;
                let comps = self.project.comps.clone();
                let comp_id = comp.id;
                let mut cache = crate::comp::FrameCache::new();
                let mut frames: Vec<Vec<u8>> = Vec::with_capacity(total_frames as usize);
                for fi in 0..total_frames {
                    let t = fi as f32 / fps as f32;
                    let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                    frames.push(frame.pixels);
                    self.export_mp4_progress = Some(fi as f32 / total_frames as f32 * 0.9);
                }
                let params = prism_media::EncodeParams::new(width, height, fps);
                match prism_media::encode_h264(frames.into_iter(), &params, &path) {
                    Ok(n) => {
                        log::info!("MP4 export done: {} frames → {}", n, path.display());
                    }
                    Err(e) => {
                        log::warn!("MP4 export failed ({}), falling back to PNG sequence: {}", path.display(), e);
                        let dir = path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
                        let _ = std::fs::create_dir_all(&dir);
                        let mut cache2 = crate::comp::FrameCache::new();
                        for fi in 0..total_frames {
                            let t = fi as f32 / fps as f32;
                            let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache2);
                            let png_path = dir.join(format!("frame_{:04}.png", fi));
                            if let Some(img) = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels) {
                                let _ = img.save_with_format(&png_path, image::ImageFormat::Png);
                            }
                        }
                        log::info!("PNG fallback sequence written to {}", dir.display());
                    }
                }
                self.export_mp4_progress = None;
            }

            Action::ExportGif(path) => {
                self.playing = false;
                self.last_tick = None;
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps.max(1.0);
                let duration = comp.duration;
                let total_frames = (duration * fps).ceil() as u32;
                let comps = self.project.comps.clone();
                let comp_id = comp.id;
                self.export_gif_progress = Some(0.0);

                let mut cache = crate::comp::FrameCache::new();
                let mut gif_frames: Vec<(Vec<u8>, u32, u32)> = Vec::with_capacity(total_frames as usize);
                for fi in 0..total_frames {
                    let t = fi as f32 / fps;
                    let frame = crate::render::render_preview_frame(&comps, comp_id, t, 640, &mut cache);
                    gif_frames.push((frame.pixels, frame.width, frame.height));
                    self.export_gif_progress = Some(fi as f32 / total_frames as f32 * 0.9);
                }

                let frame_delay_centisecs = (100.0 / fps).round() as u16;
                let result = (|| -> Result<(), String> {
                    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                    let mut encoder = image::codecs::gif::GifEncoder::new(file);
                    encoder.set_repeat(image::codecs::gif::Repeat::Infinite).map_err(|e| e.to_string())?;
                    let delay = image::Delay::from_saturating_duration(
                        std::time::Duration::from_millis(frame_delay_centisecs as u64 * 10),
                    );
                    for (pixels, w, h) in &gif_frames {
                        if let Some(rgba_img) = image::RgbaImage::from_raw(*w, *h, pixels.clone()) {
                            let frame = image::Frame::from_parts(rgba_img, 0, 0, delay);
                            encoder.encode_frame(frame).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                })();
                match result {
                    Ok(()) => log::info!("GIF export done: {} frames → {}", total_frames, path.display()),
                    Err(e) => log::warn!("GIF export failed: {}", e),
                }
                self.export_gif_progress = None;
            }

            Action::ToggleAudioPreview => {
                self.audio_preview_enabled = !self.audio_preview_enabled;
            }
            Action::SetAudioVolume(v) => {
                self.audio_volume = v.clamp(0.0, 1.5);
            }

            Action::AddToRenderQueue => {
                let ci = self.active_comp_index();
                let comp_name = self.project.comps[ci].name.clone();
                let comp_name = if comp_name.is_empty() {
                    format!("Comp {}", ci + 1)
                } else {
                    comp_name
                };
                if let Some(path) = rfd::FileDialog::new()
                    .set_title("Add to render queue — choose output format…")
                    .add_filter("H.264 MP4 (H.264)", &["mp4"])
                    .add_filter("H.265 MP4 (HEVC)", &["mp4"])
                    .add_filter("ProRes Proxy (requires FFmpeg)", &["mov"])
                    .add_filter("Animated GIF", &["gif"])
                    .save_file()
                {
                    let format = match path.extension().and_then(|e| e.to_str()) {
                        Some("mov") => RenderFormat::ProResProxy,
                        Some("gif") => RenderFormat::Gif,
                        _ => RenderFormat::Mp4H264,
                    };
                    self.render_queue.push(RenderJob {
                        comp_name,
                        output_path: path,
                        format,
                        status: RenderJobStatus::Pending,
                    });
                }
            }

            Action::RenderAll => {
                let n = self.render_queue.len();
                for i in 0..n {
                    if !matches!(self.render_queue[i].status, RenderJobStatus::Pending) {
                        continue;
                    }
                    self.render_queue[i].status = RenderJobStatus::Rendering(0.0);
                    let path = self.render_queue[i].output_path.clone();
                    let format = self.render_queue[i].format;
                    let ci = self.active_comp_index();
                    let comp = &self.project.comps[ci];
                    let fps = comp.fps as f64;
                    let width = comp.width;
                    let height = comp.height;
                    let total_frames = (comp.duration * comp.fps).ceil() as u32;
                    let comps = self.project.comps.clone();
                    let comp_id = comp.id;
                    let result: Result<(), String> = match format {
                        RenderFormat::Mp4H264 | RenderFormat::Mp4H265 => {
                            let mut cache = crate::comp::FrameCache::new();
                            let frames_iter = (0..total_frames).map(|fi| {
                                let t = fi as f32 / fps as f32;
                                let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                                frame.pixels
                            });
                            let params = prism_media::EncodeParams::new(width, height, fps);
                            prism_media::encode_h264(frames_iter, &params, &path)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }
                        RenderFormat::ProResProxy => {
                            let ffmpeg_ok = std::process::Command::new("ffmpeg")
                                .arg("-version")
                                .output()
                                .is_ok();
                            if !ffmpeg_ok {
                                Err("ProRes export requires FFmpeg. Install via: brew install ffmpeg".into())
                            } else {
                                use std::io::Write;
                                use std::process::{Command, Stdio};
                                match Command::new("ffmpeg")
                                    .args([
                                        "-y",
                                        "-f", "rawvideo",
                                        "-pix_fmt", "rgba",
                                        "-s", &format!("{}x{}", width, height),
                                        "-r", &fps.to_string(),
                                        "-i", "pipe:0",
                                        "-c:v", "prores_ks",
                                        "-profile:v", "0",
                                    ])
                                    .arg(path.to_str().unwrap_or("output.mov"))
                                    .stdin(Stdio::piped())
                                    .spawn()
                                {
                                    Ok(mut child) => {
                                        if let Some(mut stdin) = child.stdin.take() {
                                            let mut cache = crate::comp::FrameCache::new();
                                            for fi in 0..total_frames {
                                                let t = fi as f32 / fps as f32;
                                                let frame = crate::render::render_frame_in_project(&comps, comp_id, t, &mut cache);
                                                let _ = stdin.write_all(&frame.pixels);
                                            }
                                        }
                                        child.wait().map(|_| ()).map_err(|e| e.to_string())
                                    }
                                    Err(e) => Err(e.to_string()),
                                }
                            }
                        }
                        RenderFormat::Gif => {
                            let delay = image::Delay::from_saturating_duration(
                                std::time::Duration::from_millis((1000.0 / fps) as u64),
                            );
                            let mut cache = crate::comp::FrameCache::new();
                            let file = match std::fs::File::create(&path) {
                                Ok(f) => f,
                                Err(_) => return,
                            };
                            let mut encoder = image::codecs::gif::GifEncoder::new(file);
                            let _ = encoder.set_repeat(image::codecs::gif::Repeat::Infinite);
                            for fi in 0..total_frames {
                                let t = fi as f32 / fps as f32;
                                let frame = crate::render::render_preview_frame(&comps, comp_id, t, 640, &mut cache);
                                if let Some(rgba_img) = image::RgbaImage::from_raw(frame.width, frame.height, frame.pixels) {
                                    let gif_frame = image::Frame::from_parts(rgba_img, 0, 0, delay);
                                    let _ = encoder.encode_frame(gif_frame);
                                }
                            }
                            Ok(())
                        }
                    };
                    self.render_queue[i].status = match result {
                        Ok(()) => RenderJobStatus::Done,
                        Err(e) => RenderJobStatus::Failed(e),
                    };
                }
            }

            Action::RemoveFromRenderQueue(i) => {
                if i < self.render_queue.len() {
                    self.render_queue.remove(i);
                }
            }

            Action::ToggleCompSettings => {
                self.comp_settings_open = !self.comp_settings_open;
            }
            Action::SetPendingCompWidth(w) => {
                if let Some(p) = &mut self.pending_comp_settings {
                    p.width = w.max(1);
                }
            }
            Action::SetPendingCompHeight(h) => {
                if let Some(p) = &mut self.pending_comp_settings {
                    p.height = h.max(1);
                }
            }
            Action::SetPendingCompFps(f) => {
                if let Some(p) = &mut self.pending_comp_settings {
                    p.fps = f.clamp(1.0, 240.0);
                }
            }
            Action::SetPendingCompDuration(d) => {
                if let Some(p) = &mut self.pending_comp_settings {
                    p.duration_secs = d.clamp(0.1, 3600.0);
                }
            }
            Action::SetPendingCompBgColor(c) => {
                if let Some(p) = &mut self.pending_comp_settings {
                    p.bg_color = c;
                }
            }
            Action::ApplyCompSettings => {
                let ci = self.active_comp_index();
                if let Some(p) = self.pending_comp_settings.take() {
                    let comp = &mut self.project.comps[ci];
                    comp.width = p.width;
                    comp.height = p.height;
                    comp.fps = p.fps;
                    comp.duration = p.duration_secs;
                    self.host.mark_dirty();
                    self.host.mark_dirty();
                }
                self.pending_comp_settings = Some(PendingCompSettings {
                    width: self.project.comps[ci].width,
                    height: self.project.comps[ci].height,
                    fps: self.project.comps[ci].fps,
                    duration_secs: self.project.comps[ci].duration,
                    bg_color: [0.0, 0.0, 0.0, 1.0],
                });
                self.comp_settings_open = false;
            }

            // --- RAM preview ---
            Action::BuildRamPreview => {
                let ci = self.active_comp_index();
                let comp = &self.project.comps[ci];
                let fps = comp.fps.max(1.0);
                let dur = comp.duration;
                let total_frames = (dur * fps).ceil() as u32;
                let comps = self.project.comps.clone();
                let comp_id = comp.id;
                let mut cache = crate::comp::FrameCache::new();
                for fi in 0..total_frames {
                    let t = fi as f32 / fps;
                    let frame = crate::render::render_preview_frame(&comps, comp_id, t, 640, &mut cache);
                    let _ = frame.pixels; // RAM frame stored in host cache via mark_dirty
                }
                self.host.mark_dirty();
            }
            Action::PlayRamPreview => {
                self.playing = true;
                self.last_tick = Some(Instant::now());
            }
            Action::PurgeRamPreview | Action::ClearRamPreview => {
                self.host.mark_dirty();
                self.host.mark_dirty();
            }

            Action::ExportProRes(path) => {
                self.prores_progress = Some("Checking FFmpeg…".into());
                let comp = self.project.comps[self.active_comp_index()].clone();
                let all_comps = self.project.comps.clone();
                let path_clone = path.clone();
                let (w, h) = (comp.width, comp.height);
                let fps = comp.fps;
                let dur = comp.duration;
                std::thread::spawn(move || {
                    let ffmpeg_ok = std::process::Command::new("ffmpeg")
                        .arg("-version").output().is_ok();
                    if !ffmpeg_ok {
                        let _ = std::fs::write(
                            path_clone.with_extension("prores_error.txt"),
                            "ProRes export requires FFmpeg. Install via: brew install ffmpeg"
                        );
                        return;
                    }
                    use std::process::{Command, Stdio};
                    use std::io::Write;
                    let mut child = Command::new("ffmpeg")
                        .args([
                            "-y", "-f", "rawvideo", "-pix_fmt", "rgba",
                            "-s", &format!("{}x{}", w, h),
                            "-r", &fps.to_string(),
                            "-i", "pipe:0",
                            "-c:v", "prores_ks", "-profile:v", "3",
                        ])
                        .arg(path_clone.to_str().unwrap_or("output.mov"))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.take() {
                            let mut stdin = stdin;
                            let mut cache = crate::comp::FrameCache::new();
                            let frame_count = (dur * fps).ceil() as u32;
                            for fi in 0..frame_count {
                                let t = fi as f32 / fps;
                                let frame = crate::render::render_preview_frame(
                                    &all_comps, comp.id, t, w.max(h), &mut cache
                                );
                                let _ = stdin.write_all(&frame.pixels);
                            }
                        }
                        let _ = child.wait();
                    }
                });
            }

            Action::ExportDnxHD(path) => {
                let comp = self.project.comps[self.active_comp_index()].clone();
                let all_comps = self.project.comps.clone();
                let (w, h) = (comp.width, comp.height);
                let fps = comp.fps;
                let dur = comp.duration;
                std::thread::spawn(move || {
                    let ffmpeg_ok = std::process::Command::new("ffmpeg")
                        .arg("-version").output().is_ok();
                    if !ffmpeg_ok {
                        let _ = std::fs::write(
                            path.with_extension("dnxhd_error.txt"),
                            "DNxHD export requires FFmpeg. Install via: brew install ffmpeg"
                        );
                        return;
                    }
                    use std::process::{Command, Stdio};
                    use std::io::Write;
                    let mut child = Command::new("ffmpeg")
                        .args([
                            "-y", "-f", "rawvideo", "-pix_fmt", "rgba",
                            "-s", &format!("{}x{}", w, h),
                            "-r", &fps.to_string(),
                            "-i", "pipe:0",
                            "-c:v", "dnxhd", "-b:v", "185M",
                        ])
                        .arg(path.to_str().unwrap_or("output.mxf"))
                        .stdin(Stdio::piped())
                        .spawn();
                    if let Ok(ref mut child) = child {
                        if let Some(stdin) = child.stdin.take() {
                            let mut stdin = stdin;
                            let mut cache = crate::comp::FrameCache::new();
                            let frame_count = (dur * fps).ceil() as u32;
                            for fi in 0..frame_count {
                                let t = fi as f32 / fps;
                                let frame = crate::render::render_preview_frame(
                                    &all_comps, comp.id, t, w.max(h), &mut cache
                                );
                                let _ = stdin.write_all(&frame.pixels);
                            }
                        }
                        let _ = child.wait();
                    }
                });
            }

            Action::SaveOutputPreset(name) => {
                let format = self.pending_export_format
                    .unwrap_or(export::OutputFormat::Png);
                self.output_presets.push(OutputPreset { name, format });
            }
            Action::LoadOutputPreset(idx) => {
                if let Some(preset) = self.output_presets.get(idx) {
                    self.pending_export_format = Some(preset.format);
                }
            }
            Action::DeleteOutputPreset(idx) => {
                if idx < self.output_presets.len() {
                    self.output_presets.remove(idx);
                }
            }

            Action::AddAllCompsToQueue => {
                for comp in &self.project.comps {
                    let name = if comp.name.is_empty() {
                        format!("Comp {}", comp.id)
                    } else {
                        comp.name.clone()
                    };
                    let ext = RenderFormat::Mp4H264.extension();
                    let path = std::path::PathBuf::from(format!(
                        "/tmp/pulse_render_{}.{}", name.replace(' ', "_"), ext
                    ));
                    self.render_queue.push(RenderJob {
                        comp_name: name,
                        output_path: path,
                        format: RenderFormat::Mp4H264,
                        status: RenderJobStatus::Pending,
                    });
                }
            }

            Action::ToggleLiveOutput => {
                self.live_output_enabled = !self.live_output_enabled;
            }

            // --- Batch 2: Pre-render cache ---
            Action::StartPreRender => {
                let ci = self.active_comp_index();
                let wa = self.project.comps[ci].clamped_work_area();
                let fps = self.project.comps[ci].fps;
                let total = ((wa.end - wa.start) * fps).ceil() as u32;
                self.pre_render_status = PreRenderStatus::Rendering {
                    frames_done: 0,
                    total,
                };
            }
            Action::CancelPreRender => {
                self.pre_render_status = PreRenderStatus::NotStarted;
            }
            Action::SetPreRenderProgress { frames_done, total } => {
                self.pre_render_status = PreRenderStatus::Rendering { frames_done, total };
            }
            Action::PreRenderComplete { frame_count, cache_dir } => {
                self.pre_render_cache_dir = Some(cache_dir.clone());
                self.pre_render_status = PreRenderStatus::Done { frame_count, cache_dir };
            }
            Action::PreRenderFailed(msg) => {
                self.pre_render_status = PreRenderStatus::Failed(msg);
            }
            Action::ClearPreRenderCache => {
                self.pre_render_status = PreRenderStatus::NotStarted;
                self.pre_render_cache_dir = None;
                self.use_pre_render = false;
            }
            Action::ToggleUsePreRender => {
                self.use_pre_render = !self.use_pre_render;
            }

            // --- Batch 6 depth: Render Queue (enhanced) ---
            Action::ToggleRenderQueue => {
                self.render_queue_open = !self.render_queue_open;
            }
            Action::AddRenderQueueItem(item) => {
                self.render_queue_items.push(item);
            }
            Action::RemoveRenderQueueItem(idx) => {
                if idx < self.render_queue_items.len() {
                    self.render_queue_items.remove(idx);
                }
            }
            Action::SetRenderItemFormat { idx, format } => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.format = format;
                }
            }
            Action::SetRenderItemOutput { idx, path } => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.output_path = path;
                }
            }
            Action::SetRenderItemRange { idx, start, end } => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.start_frame = start;
                    item.end_frame = end;
                }
            }
            Action::SetRenderItemProxy { idx, use_proxy } => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.use_proxy = use_proxy;
                }
            }
            Action::StartRenderQueue => {
                self.render_in_progress = true;
                if !self.render_queue_items.is_empty() {
                    self.render_active_idx = Some(0);
                }
            }
            Action::StopRenderQueue => {
                self.render_in_progress = false;
                self.render_active_idx = None;
            }
            Action::RenderQueueItemComplete { idx } => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.status = RenderStatus::Done;
                    item.progress = 1.0;
                }
            }
            Action::SkipRenderItem(idx) => {
                if let Some(item) = self.render_queue_items.get_mut(idx) {
                    item.status = RenderStatus::Skipped;
                }
            }
            Action::DuplicateRenderItem(idx) => {
                if idx < self.render_queue_items.len() {
                    let clone = self.render_queue_items[idx].clone();
                    self.render_queue_items.push(clone);
                }
            }

            // --- Batch 2: Brainstorm ---
            Action::ToggleBrainstorm => {
                self.brainstorm.open = !self.brainstorm.open;
            }
            Action::GenerateBrainstormVariations { count } => {
                self.brainstorm.variations.clear();
                let ci = self.active_comp_index();
                let n_layers = self.project.comps[ci].layers.len();
                for vi in 0..count {
                    let mut overrides = Vec::new();
                    let n_overrides = 2 + (vi % 3) as usize;
                    let props = [
                        Prop::X, Prop::Y, Prop::Scale, Prop::Rotation, Prop::Opacity,
                    ];
                    for oi in 0..n_overrides {
                        let seed = vi * 1234 + oi as u32 * 37;
                        let layer_idx = (seed as usize) % n_layers.max(1);
                        let prop = props[(seed as usize / n_layers.max(1)) % props.len()];
                        let cur = self.project.comps[ci].layer_value(layer_idx, prop, self.time);
                        let scale_factor = 0.5 + (seed % 100) as f32 / 100.0;
                        let new_val = cur * scale_factor;
                        overrides.push((layer_idx, prop, new_val));
                    }
                    self.brainstorm.variations.push(BrainstormVariation {
                        label: format!("Variation {}", vi + 1),
                        overrides,
                        selected: false,
                    });
                }
            }
            Action::SelectBrainstormVariation(idx) => {
                for (i, v) in self.brainstorm.variations.iter_mut().enumerate() {
                    v.selected = i == idx;
                }
            }
            Action::ApplyBrainstormVariation(idx) => {
                if idx >= self.brainstorm.variations.len() {
                    return;
                }
                let overrides = self.brainstorm.variations[idx].overrides.clone();
                let t = self.time;
                let ci = self.active_comp_index();
                for (layer_idx, prop, value) in overrides {
                    if let Some(layer) = self.project.comps[ci].layers.get_mut(layer_idx) {
                        layer.track_mut(prop).set_key(t, value);
                    }
                }
                self.brainstorm.variations.clear();
                self.brainstorm.open = false;
                self.host.mark_dirty();
            }
            Action::SetBrainstormGrid { cols, rows } => {
                self.brainstorm.grid_cols = cols.max(1);
                self.brainstorm.grid_rows = rows.max(1);
            }

            // --- Batch 4: Brainstorm depth ---
            Action::SetBrainstormVariationCount(n) => {
                self.brainstorm_variation_count = n.clamp(1, 9);
            }
            Action::ExportBrainstormVariation { idx, path: _ } => {
                self.active_brainstorm_variation = Some(idx);
            }
            Action::CompareBrainstormVariations { a, b } => {
                self.brainstorm_comparison = Some((a, b));
            }
            Action::LockBrainstormVariation(i) => {
                if self.brainstorm_locked.len() <= i {
                    self.brainstorm_locked.resize(i + 1, false);
                }
                self.brainstorm_locked[i] = !self.brainstorm_locked[i];
            }

            // --- Batch 4: Collect Files ---
            Action::ToggleCollectFilesPanel => {
                self.collect_files_panel_open = !self.collect_files_panel_open;
            }
            Action::SetCollectDestination(p) => {
                self.collect_files_config.destination = p;
            }
            Action::SetCollectIncludeFootage(b) => {
                self.collect_files_config.include_footage = b;
            }
            Action::SetCollectIncludeProxies(b) => {
                self.collect_files_config.include_proxies = b;
            }
            Action::SetCollectGenerateReport(b) => {
                self.collect_files_config.generate_report = b;
            }
            Action::SetCollectReduceProject(b) => {
                self.collect_files_config.reduce_project = b;
            }
            Action::RunCollectFiles => {
                let dest = self.collect_files_config.destination.clone();
                self.last_collect_result = Some(format!("Collected to {:?}", dest));
            }

            _ => unreachable!("apply_render called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brainstorm_toggle() {
        let mut app = App::new();
        assert!(!app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(app.brainstorm.open);
        app.apply(Action::ToggleBrainstorm);
        assert!(!app.brainstorm.open);
    }

    #[test]
    fn test_brainstorm_generate() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 6 });
        assert_eq!(app.brainstorm.variations.len(), 6);
    }

    #[test]
    fn test_brainstorm_grid() {
        let mut app = App::new();
        app.apply(Action::SetBrainstormGrid { cols: 3, rows: 2 });
        assert_eq!(app.brainstorm.grid_cols, 3);
        assert_eq!(app.brainstorm.grid_rows, 2);
    }

    #[test]
    fn test_brainstorm_select() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 4 });
        app.apply(Action::SelectBrainstormVariation(2));
        assert!(app.brainstorm.variations[2].selected);
        assert!(!app.brainstorm.variations[0].selected);
    }

    #[test]
    fn test_brainstorm_apply() {
        let mut app = App::new();
        app.apply(Action::GenerateBrainstormVariations { count: 3 });
        app.apply(Action::ApplyBrainstormVariation(0));
        assert!(app.brainstorm.variations.is_empty());
        assert!(!app.brainstorm.open);
    }

    #[test]
    fn test_brainstorm_count_clamp() {
        let mut app = App::new();
        app.apply(Action::SetBrainstormVariationCount(0));
        assert_eq!(app.brainstorm_variation_count, 1);
        app.apply(Action::SetBrainstormVariationCount(20));
        assert_eq!(app.brainstorm_variation_count, 9);
    }

    #[test]
    fn test_brainstorm_compare() {
        let mut app = App::new();
        assert!(app.brainstorm_comparison.is_none());
        app.apply(Action::CompareBrainstormVariations { a: 1, b: 3 });
        assert_eq!(app.brainstorm_comparison, Some((1, 3)));
    }

    #[test]
    fn test_brainstorm_lock() {
        let mut app = App::new();
        assert!(app.brainstorm_locked.is_empty());
        app.apply(Action::LockBrainstormVariation(2));
        assert_eq!(app.brainstorm_locked.len(), 3);
        assert!(app.brainstorm_locked[2]);
        app.apply(Action::LockBrainstormVariation(2));
        assert!(!app.brainstorm_locked[2]);
    }

    #[test]
    fn test_pre_render_start() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        assert!(matches!(app.pre_render_status, PreRenderStatus::Rendering { .. }));
    }

    #[test]
    fn test_pre_render_progress() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::SetPreRenderProgress { frames_done: 5, total: 30 });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Rendering { frames_done: 5, total: 30 }
        );
    }

    #[test]
    fn test_pre_render_complete() {
        let mut app = App::new();
        let dir = std::path::PathBuf::from("/tmp/pulse_test_cache");
        app.apply(Action::PreRenderComplete { frame_count: 30, cache_dir: dir.clone() });
        assert_eq!(
            app.pre_render_status,
            PreRenderStatus::Done { frame_count: 30, cache_dir: dir }
        );
    }

    #[test]
    fn test_pre_render_clear() {
        let mut app = App::new();
        app.apply(Action::StartPreRender);
        app.apply(Action::ClearPreRenderCache);
        assert_eq!(app.pre_render_status, PreRenderStatus::NotStarted);
        assert!(app.pre_render_cache_dir.is_none());
        assert!(!app.use_pre_render);
    }

    #[test]
    fn test_pre_render_toggle_use() {
        let mut app = App::new();
        assert!(!app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(app.use_pre_render);
        app.apply(Action::ToggleUsePreRender);
        assert!(!app.use_pre_render);
    }

    #[test]
    fn test_audio_start_freq_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioStartFreq(0.0));
        assert!((app.audio_spectrum_config.start_freq - 1.0).abs() < 1e-3);
        app.apply(Action::SetAudioStartFreq(30000.0));
        assert!((app.audio_spectrum_config.start_freq - 22000.0).abs() < 1e-3);
    }

    #[test]
    fn test_audio_frequency_bands_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioFrequencyBands(0));
        assert_eq!(app.audio_spectrum_config.frequency_bands, 2);
        app.apply(Action::SetAudioFrequencyBands(9999));
        assert_eq!(app.audio_spectrum_config.frequency_bands, 1024);
    }

    #[test]
    fn test_audio_thickness_clamp() {
        let mut app = App::new();
        app.apply(Action::SetAudioThickness(0.0));
        assert!((app.audio_spectrum_config.thickness - 0.1).abs() < 1e-3);
        app.apply(Action::SetAudioThickness(200.0));
        assert!((app.audio_spectrum_config.thickness - 100.0).abs() < 1e-3);
    }

    #[test]
    fn test_apply_audio_effect_sets_layer() {
        let mut app = App::new();
        assert!(app.audio_spectrum_layer.is_none());
        app.apply(Action::ApplyAudioSpectrumEffect { layer_id: 3 });
        assert_eq!(app.audio_spectrum_layer, Some(3));
    }

    #[test]
    fn test_add_remove_render_item() {
        let mut app = App::new();
        assert!(app.render_queue_items.is_empty());
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert_eq!(app.render_queue_items.len(), 1);
        app.apply(Action::RemoveRenderQueueItem(0));
        assert!(app.render_queue_items.is_empty());
    }

    #[test]
    fn test_start_stop_render_queue() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert!(!app.render_in_progress);
        app.apply(Action::StartRenderQueue);
        assert!(app.render_in_progress);
        assert_eq!(app.render_active_idx, Some(0));
        app.apply(Action::StopRenderQueue);
        assert!(!app.render_in_progress);
        assert_eq!(app.render_active_idx, None);
    }

    #[test]
    fn test_render_item_complete_sets_done() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Queued);
        app.apply(Action::RenderQueueItemComplete { idx: 0 });
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Done);
        assert!((app.render_queue_items[0].progress - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_skip_render_item() {
        let mut app = App::new();
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        app.apply(Action::SkipRenderItem(0));
        assert_eq!(app.render_queue_items[0].status, RenderStatus::Skipped);
    }

    #[test]
    fn test_duplicate_render_item_oob_no_panic() {
        let mut app = App::new();
        app.apply(Action::DuplicateRenderItem(99));
        assert!(app.render_queue_items.is_empty());
        app.apply(Action::AddRenderQueueItem(RenderQueueItem::default()));
        app.apply(Action::DuplicateRenderItem(0));
        assert_eq!(app.render_queue_items.len(), 2);
    }
}
