//! Lottie import/export domain for Drift.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum LottieImportStatus {
    Pending,
    Parsing,
    Done,
    Error,
}

#[derive(Clone, Debug)]
pub struct LottieKf {
    pub t: f32,
    pub s: Vec<f32>,
    pub e: Vec<f32>,
}

#[derive(Clone, Debug)]
pub enum LottiePropValue {
    Static(Vec<f32>),
    Animated(Vec<LottieKf>),
}

#[derive(Clone, Debug)]
pub struct LottieLayerKs {
    pub p: LottiePropValue,
    pub r: LottiePropValue,
    pub s: LottiePropValue,
    pub o: LottiePropValue,
}

#[derive(Clone, Debug)]
pub struct LottieLayerDef {
    pub ind: usize,
    pub nm: String,
    pub ty: u8,
    pub ip: usize,
    pub op: usize,
    pub ks: LottieLayerKs,
}

#[derive(Clone, Debug)]
pub struct LottieAnim {
    pub v: String,
    pub fr: f32,
    pub ip: usize,
    pub op: usize,
    pub w: u32,
    pub h: u32,
    pub layers: Vec<LottieLayerDef>,
}

#[derive(Clone, Debug)]
pub struct LottieExportConfig {
    pub layer_ids: Vec<usize>,
    pub target_path: String,
    pub pretty_print: bool,
}

#[derive(Clone, Debug)]
pub struct LottieImportJob {
    pub id: usize,
    pub source_path: String,
    pub layers_created: usize,
    pub status: LottieImportStatus,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_lottie(&mut self, action: &Action) {
        match action {
            Action::StartLottieExport { config } => {
                self.lottie_export_config = Some(config.clone());
                self.lottie_exporting = true;
            }
            Action::CompleteLottieExport => {
                self.lottie_exporting = false;
            }
            Action::CancelLottieExport => {
                self.lottie_exporting = false;
                self.lottie_export_config = None;
            }
            Action::StartLottieImport { source_path } => {
                let id = self.next_lottie_job_id;
                self.next_lottie_job_id += 1;
                self.lottie_import_jobs.push(LottieImportJob {
                    id,
                    source_path: source_path.clone(),
                    layers_created: 0,
                    status: LottieImportStatus::Pending,
                });
            }
            Action::UpdateLottieImport { job_id, layers_created } => {
                if let Some(job) = self.lottie_import_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.layers_created = *layers_created;
                    job.status = LottieImportStatus::Parsing;
                }
            }
            Action::CompleteLottieImport { job_id } => {
                if let Some(job) = self.lottie_import_jobs.iter_mut().find(|j| j.id == *job_id) {
                    job.status = LottieImportStatus::Done;
                }
            }
            Action::CancelLottieImport { job_id } => {
                self.lottie_import_jobs.retain(|j| j.id != *job_id);
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{LottieExportConfig, LottieImportStatus};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_start_lottie_export() {
        let mut a = app();
        let config = LottieExportConfig {
            layer_ids: vec![1, 2],
            target_path: "/out/anim.json".to_string(),
            pretty_print: true,
        };
        a.apply(Action::StartLottieExport { config: config.clone() });
        assert!(a.lottie_exporting);
        assert!(a.lottie_export_config.is_some());
        assert_eq!(a.lottie_export_config.as_ref().unwrap().target_path, "/out/anim.json");
    }

    #[test]
    fn test_complete_lottie_export() {
        let mut a = app();
        let config = LottieExportConfig {
            layer_ids: vec![],
            target_path: "/out/anim.json".to_string(),
            pretty_print: false,
        };
        a.apply(Action::StartLottieExport { config });
        a.apply(Action::CompleteLottieExport);
        assert!(!a.lottie_exporting);
        // config is preserved after complete (only cleared on cancel)
        assert!(a.lottie_export_config.is_some());
    }

    #[test]
    fn test_cancel_lottie_export() {
        let mut a = app();
        let config = LottieExportConfig {
            layer_ids: vec![3],
            target_path: "/out/anim.json".to_string(),
            pretty_print: false,
        };
        a.apply(Action::StartLottieExport { config });
        a.apply(Action::CancelLottieExport);
        assert!(!a.lottie_exporting);
        assert!(a.lottie_export_config.is_none());
    }

    #[test]
    fn test_start_lottie_import() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "/assets/hero.json".to_string() });
        assert_eq!(a.lottie_import_jobs.len(), 1);
        let job = &a.lottie_import_jobs[0];
        assert_eq!(job.source_path, "/assets/hero.json");
        assert_eq!(job.status, LottieImportStatus::Pending);
        assert_eq!(job.layers_created, 0);
    }

    #[test]
    fn test_start_multiple_imports_ids_are_unique() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "a.json".to_string() });
        a.apply(Action::StartLottieImport { source_path: "b.json".to_string() });
        let ids: Vec<_> = a.lottie_import_jobs.iter().map(|j| j.id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn test_update_lottie_import() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "x.json".to_string() });
        let job_id = a.lottie_import_jobs[0].id;
        a.apply(Action::UpdateLottieImport { job_id, layers_created: 5 });
        let job = &a.lottie_import_jobs[0];
        assert_eq!(job.layers_created, 5);
        assert_eq!(job.status, LottieImportStatus::Parsing);
    }

    #[test]
    fn test_complete_lottie_import() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "x.json".to_string() });
        let job_id = a.lottie_import_jobs[0].id;
        a.apply(Action::CompleteLottieImport { job_id });
        assert_eq!(a.lottie_import_jobs[0].status, LottieImportStatus::Done);
    }

    #[test]
    fn test_cancel_lottie_import_removes_job() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "x.json".to_string() });
        let job_id = a.lottie_import_jobs[0].id;
        a.apply(Action::CancelLottieImport { job_id });
        assert!(a.lottie_import_jobs.is_empty());
    }

    #[test]
    fn test_cancel_lottie_import_only_removes_target() {
        let mut a = app();
        a.apply(Action::StartLottieImport { source_path: "a.json".to_string() });
        a.apply(Action::StartLottieImport { source_path: "b.json".to_string() });
        let job_id = a.lottie_import_jobs[0].id;
        a.apply(Action::CancelLottieImport { job_id });
        assert_eq!(a.lottie_import_jobs.len(), 1);
        assert_eq!(a.lottie_import_jobs[0].source_path, "b.json");
    }

    #[test]
    fn test_update_unknown_job_is_noop() {
        let mut a = app();
        a.apply(Action::UpdateLottieImport { job_id: 999, layers_created: 3 });
        assert!(a.lottie_import_jobs.is_empty());
    }

    #[test]
    fn test_complete_unknown_job_is_noop() {
        let mut a = app();
        // Should not panic
        a.apply(Action::CompleteLottieImport { job_id: 999 });
    }
}
