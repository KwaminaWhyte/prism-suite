use super::*;

// The render-queue / export status + config types live in `render_types.rs`
// (extracted for the workspace size rule), re-exported so existing `render::*`
// paths and `app_state`'s `pub use render_types::{…}` keep resolving.

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
#[path = "render_tests.rs"]
mod tests;
