use super::{App, Action, ExportConfig};

/// A saved export configuration with a name.
#[derive(Clone, Debug)]
pub struct ExportPreset {
    pub id: usize,
    pub name: String,
    pub config: ExportConfig,
}

/// Status of a batch export job.
#[derive(Clone, Debug, PartialEq)]
pub enum BatchJobStatus {
    Queued,
    Running,
    Done,
    Error,
}

/// One export job within a render batch.
#[derive(Clone, Debug)]
pub struct BatchExportJob {
    pub id: usize,
    pub preset_id: usize,
    pub output_path: String,
    pub status: BatchJobStatus,
    pub progress: f32,
}

/// A named batch of export jobs that can be run together.
#[derive(Clone, Debug)]
pub struct RenderQueueBatch {
    pub id: usize,
    pub jobs: Vec<BatchExportJob>,
    pub running: bool,
}

impl App {
    pub(super) fn apply_export_presets(&mut self, action: Action) {
        match action {
            Action::SaveExportPreset { name, config } => {
                let id = self.next_preset_id;
                self.next_preset_id += 1;
                self.export_presets.push(ExportPreset { id, name, config });
            }
            Action::DeleteExportPreset { preset_id } => {
                self.export_presets.retain(|p| p.id != preset_id);
            }
            Action::RenameExportPreset { preset_id, name } => {
                if let Some(p) = self.export_presets.iter_mut().find(|p| p.id == preset_id) {
                    p.name = name;
                }
            }
            Action::CreateRenderBatch => {
                let id = self.next_batch_id;
                self.next_batch_id += 1;
                self.render_batches.push(RenderQueueBatch {
                    id,
                    jobs: vec![],
                    running: false,
                });
            }
            Action::AddJobToBatch { batch_id, preset_id, output_path } => {
                if let Some(b) = self.render_batches.iter_mut().find(|b| b.id == batch_id) {
                    let id = self.next_batch_job_id;
                    self.next_batch_job_id += 1;
                    b.jobs.push(BatchExportJob {
                        id,
                        preset_id,
                        output_path,
                        status: BatchJobStatus::Queued,
                        progress: 0.0,
                    });
                }
            }
            Action::StartRenderBatch { batch_id } => {
                if let Some(b) = self.render_batches.iter_mut().find(|b| b.id == batch_id) {
                    b.running = true;
                    if let Some(j) = b.jobs.first_mut() {
                        j.status = BatchJobStatus::Running;
                    }
                }
            }
            Action::UpdateBatchJobProgress { job_id, progress } => {
                for b in &mut self.render_batches {
                    if let Some(j) = b.jobs.iter_mut().find(|j| j.id == job_id) {
                        j.progress = progress.clamp(0.0, 1.0);
                        break;
                    }
                }
            }
            Action::CompleteBatchJob { job_id } => {
                for b in &mut self.render_batches {
                    if let Some(j) = b.jobs.iter_mut().find(|j| j.id == job_id) {
                        j.status = BatchJobStatus::Done;
                        j.progress = 1.0;
                        break;
                    }
                }
            }
            Action::CancelRenderBatch { batch_id } => {
                if let Some(b) = self.render_batches.iter_mut().find(|b| b.id == batch_id) {
                    b.running = false;
                    for j in &mut b.jobs {
                        if j.status == BatchJobStatus::Running {
                            j.status = BatchJobStatus::Queued;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::super::export::ExportFormat;
    use super::BatchJobStatus;

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_save_export_preset() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "Web GIF".to_string(), config });
        assert_eq!(a.export_presets.len(), 1);
        assert_eq!(a.export_presets[0].name, "Web GIF");
        assert_eq!(a.export_presets[0].id, 1);
    }

    #[test]
    fn test_save_multiple_presets_unique_ids() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "Preset A".to_string(), config: config.clone() });
        a.apply(Action::SaveExportPreset { name: "Preset B".to_string(), config });
        assert_eq!(a.export_presets.len(), 2);
        assert_ne!(a.export_presets[0].id, a.export_presets[1].id);
    }

    #[test]
    fn test_delete_export_preset() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "To Delete".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::DeleteExportPreset { preset_id: pid });
        assert!(a.export_presets.is_empty());
    }

    #[test]
    fn test_rename_export_preset() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "Old Name".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::RenameExportPreset { preset_id: pid, name: "New Name".to_string() });
        assert_eq!(a.export_presets[0].name, "New Name");
    }

    #[test]
    fn test_preset_stores_config() {
        let mut a = app();
        a.apply(Action::SetExportFormat(ExportFormat::Gif));
        a.apply(Action::SetExportFps(15.0));
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "GIF 15fps".to_string(), config });
        assert_eq!(a.export_presets[0].config.format, ExportFormat::Gif);
        assert_eq!(a.export_presets[0].config.fps, 15.0);
    }

    #[test]
    fn test_create_render_batch() {
        let mut a = app();
        a.apply(Action::CreateRenderBatch);
        assert_eq!(a.render_batches.len(), 1);
        assert_eq!(a.render_batches[0].id, 1);
        assert!(!a.render_batches[0].running);
        assert!(a.render_batches[0].jobs.is_empty());
    }

    #[test]
    fn test_add_job_to_batch() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/scene1.mp4".to_string(),
        });
        assert_eq!(a.render_batches[0].jobs.len(), 1);
        assert_eq!(a.render_batches[0].jobs[0].status, BatchJobStatus::Queued);
        assert_eq!(a.render_batches[0].jobs[0].progress, 0.0);
        assert_eq!(a.render_batches[0].jobs[0].output_path, "/out/scene1.mp4");
    }

    #[test]
    fn test_start_render_batch() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/a.mp4".to_string(),
        });
        a.apply(Action::StartRenderBatch { batch_id: bid });
        assert!(a.render_batches[0].running);
        assert_eq!(a.render_batches[0].jobs[0].status, BatchJobStatus::Running);
    }

    #[test]
    fn test_update_batch_job_progress() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/a.mp4".to_string(),
        });
        let jid = a.render_batches[0].jobs[0].id;
        a.apply(Action::UpdateBatchJobProgress { job_id: jid, progress: 0.5 });
        assert_eq!(a.render_batches[0].jobs[0].progress, 0.5);
    }

    #[test]
    fn test_update_progress_clamps() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/b.mp4".to_string(),
        });
        let jid = a.render_batches[0].jobs[0].id;
        a.apply(Action::UpdateBatchJobProgress { job_id: jid, progress: 2.5 });
        assert_eq!(a.render_batches[0].jobs[0].progress, 1.0);
        a.apply(Action::UpdateBatchJobProgress { job_id: jid, progress: -1.0 });
        assert_eq!(a.render_batches[0].jobs[0].progress, 0.0);
    }

    #[test]
    fn test_complete_batch_job() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/c.mp4".to_string(),
        });
        let jid = a.render_batches[0].jobs[0].id;
        a.apply(Action::CompleteBatchJob { job_id: jid });
        assert_eq!(a.render_batches[0].jobs[0].status, BatchJobStatus::Done);
        assert_eq!(a.render_batches[0].jobs[0].progress, 1.0);
    }

    #[test]
    fn test_cancel_render_batch() {
        let mut a = app();
        let config = a.export_config.clone();
        a.apply(Action::SaveExportPreset { name: "P1".to_string(), config });
        let pid = a.export_presets[0].id;
        a.apply(Action::CreateRenderBatch);
        let bid = a.render_batches[0].id;
        a.apply(Action::AddJobToBatch {
            batch_id: bid,
            preset_id: pid,
            output_path: "/out/d.mp4".to_string(),
        });
        a.apply(Action::StartRenderBatch { batch_id: bid });
        a.apply(Action::CancelRenderBatch { batch_id: bid });
        assert!(!a.render_batches[0].running);
        // Running job reverts to Queued
        assert_eq!(a.render_batches[0].jobs[0].status, BatchJobStatus::Queued);
    }
}
