//! Per-artboard export metadata (Photoshop *Export As → Artboards* parity).
//!
//! The core [`Artboard`](super::Artboard) model (named rect regions + CRUD) lives
//! in `selections.rs`. This module layers **export metadata** on top: each
//! artboard can carry its own [`ArtboardExportConfig`] (format, scale, filename
//! prefix/suffix, enabled flag). [`App::artboard_export_plan`] turns the current
//! artboard list + the per-artboard configs into a deterministic, ordered list of
//! [`ArtboardExportItem`]s (filename + pixel size) that an exporter can iterate —
//! no GPU and no disk I/O required to compute the plan, so it is fully testable.

use std::collections::HashMap;

use super::{Action, App};

/// Output image format for an artboard export. Self-contained (does not depend on
/// the document-wide `ExportFormat`) so the per-artboard config stays decoupled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArtboardExportFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
    Tiff,
}

impl ArtboardExportFormat {
    /// Lowercase file extension (no dot).
    pub fn extension(self) -> &'static str {
        match self {
            ArtboardExportFormat::Png => "png",
            ArtboardExportFormat::Jpeg => "jpg",
            ArtboardExportFormat::Webp => "webp",
            ArtboardExportFormat::Tiff => "tiff",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ArtboardExportFormat::Png => "PNG",
            ArtboardExportFormat::Jpeg => "JPEG",
            ArtboardExportFormat::Webp => "WebP",
            ArtboardExportFormat::Tiff => "TIFF",
        }
    }
}

/// Per-artboard export settings.
#[derive(Debug, Clone, PartialEq)]
pub struct ArtboardExportConfig {
    /// Output format.
    pub format: ArtboardExportFormat,
    /// Scale multiplier applied to the artboard's pixel size (e.g. 2.0 = @2x).
    pub scale: f32,
    /// Filename prefix prepended to the artboard name.
    pub prefix: String,
    /// Filename suffix appended after the artboard name (before the extension),
    /// e.g. "@2x".
    pub suffix: String,
    /// JPEG/WebP quality 1..=100 (ignored for PNG).
    pub quality: u8,
    /// Whether this artboard participates in a batch export.
    pub enabled: bool,
}

impl Default for ArtboardExportConfig {
    fn default() -> Self {
        Self {
            format: ArtboardExportFormat::Png,
            scale: 1.0,
            prefix: String::new(),
            suffix: String::new(),
            quality: 90,
            enabled: true,
        }
    }
}

impl ArtboardExportConfig {
    /// Clamp scale/quality to sane ranges.
    pub fn clamp(&mut self) {
        self.scale = self.scale.clamp(0.1, 8.0);
        self.quality = self.quality.clamp(1, 100);
    }

    /// Build the output filename for an artboard of `name`, sanitizing the name to
    /// a filesystem-safe slug (spaces/illegal chars → `_`).
    pub fn filename(&self, name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '_',
            })
            .collect();
        format!("{}{}{}.{}", self.prefix, slug, self.suffix, self.format.extension())
    }
}

/// One resolved export item produced by [`App::artboard_export_plan`].
#[derive(Debug, Clone, PartialEq)]
pub struct ArtboardExportItem {
    pub artboard_id: u64,
    pub filename: String,
    /// Output pixel width after applying the config's scale.
    pub out_width: u32,
    /// Output pixel height after applying the config's scale.
    pub out_height: u32,
    pub format: ArtboardExportFormat,
    pub quality: u8,
}

impl App {
    /// Get (or lazily default) the export config for an artboard id.
    pub fn artboard_export_config(&self, id: u64) -> ArtboardExportConfig {
        self.artboard_export_configs.get(&id).cloned().unwrap_or_default()
    }

    /// Build the ordered export plan over all *enabled* artboards. Disabled
    /// artboards (per their config) are skipped. Deterministic — no I/O.
    pub fn artboard_export_plan(&self) -> Vec<ArtboardExportItem> {
        let mut plan = Vec::new();
        for ab in &self.artboards {
            let cfg = self.artboard_export_config(ab.id);
            if !cfg.enabled {
                continue;
            }
            let s = cfg.scale.clamp(0.1, 8.0);
            let out_w = ((ab.width as f32 * s).round() as u32).max(1);
            let out_h = ((ab.height as f32 * s).round() as u32).max(1);
            plan.push(ArtboardExportItem {
                artboard_id: ab.id,
                filename: cfg.filename(&ab.name),
                out_width: out_w,
                out_height: out_h,
                format: cfg.format,
                quality: cfg.quality.clamp(1, 100),
            });
        }
        plan
    }

    pub(super) fn apply_artboards_export(&mut self, action: Action) {
        match action {
            Action::SetArtboardExportFormat { id, format } => {
                let mut cfg = self.artboard_export_config(id);
                cfg.format = format;
                self.artboard_export_configs.insert(id, cfg);
            }
            Action::SetArtboardExportScale { id, scale } => {
                let mut cfg = self.artboard_export_config(id);
                cfg.scale = scale;
                cfg.clamp();
                self.artboard_export_configs.insert(id, cfg);
            }
            Action::SetArtboardExportNaming { id, prefix, suffix } => {
                let mut cfg = self.artboard_export_config(id);
                cfg.prefix = prefix;
                cfg.suffix = suffix;
                self.artboard_export_configs.insert(id, cfg);
            }
            Action::SetArtboardExportQuality { id, quality } => {
                let mut cfg = self.artboard_export_config(id);
                cfg.quality = quality.clamp(1, 100);
                self.artboard_export_configs.insert(id, cfg);
            }
            Action::SetArtboardExportEnabled { id, enabled } => {
                let mut cfg = self.artboard_export_config(id);
                cfg.enabled = enabled;
                self.artboard_export_configs.insert(id, cfg);
            }
            Action::PrepareArtboardExport => {
                let plan = self.artboard_export_plan();
                self.status_message = Some(format!("{} artboard(s) ready to export", plan.len()));
                self.last_artboard_export_plan = plan;
            }
            _ => {}
        }
    }
}

/// Field-less helper used to seed an artboard for tests in other modules.
pub fn default_export_configs() -> HashMap<u64, ArtboardExportConfig> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Action;

    fn app_with_artboards() -> App {
        let mut app = App::new();
        app.apply(Action::AddArtboard {
            name: "Home Screen".into(),
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        });
        app.apply(Action::AddArtboard {
            name: "Detail".into(),
            x: 820,
            y: 0,
            width: 400,
            height: 300,
        });
        app
    }

    #[test]
    fn format_extension_and_label() {
        assert_eq!(ArtboardExportFormat::Png.extension(), "png");
        assert_eq!(ArtboardExportFormat::Jpeg.extension(), "jpg");
        assert_eq!(ArtboardExportFormat::Webp.label(), "WebP");
    }

    #[test]
    fn config_default_clamp() {
        let mut c = ArtboardExportConfig { scale: 99.0, quality: 200, ..Default::default() };
        c.clamp();
        assert_eq!(c.scale, 8.0);
        assert_eq!(c.quality, 100);
    }

    #[test]
    fn filename_slugifies_name() {
        let cfg = ArtboardExportConfig {
            prefix: "ui_".into(),
            suffix: "@2x".into(),
            format: ArtboardExportFormat::Png,
            ..Default::default()
        };
        assert_eq!(cfg.filename("Home Screen"), "ui_Home_Screen@2x.png");
    }

    #[test]
    fn export_plan_covers_all_enabled() {
        let app = app_with_artboards();
        let plan = app.artboard_export_plan();
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].out_width, 800);
        assert_eq!(plan[0].out_height, 600);
        assert!(plan[0].filename.ends_with(".png"));
    }

    #[test]
    fn scale_affects_output_size() {
        let mut app = app_with_artboards();
        let id = app.artboards[0].id;
        app.apply(Action::SetArtboardExportScale { id, scale: 2.0 });
        let plan = app.artboard_export_plan();
        let item = plan.iter().find(|p| p.artboard_id == id).unwrap();
        assert_eq!(item.out_width, 1600);
        assert_eq!(item.out_height, 1200);
    }

    #[test]
    fn disabled_artboard_skipped() {
        let mut app = app_with_artboards();
        let id = app.artboards[1].id;
        app.apply(Action::SetArtboardExportEnabled { id, enabled: false });
        let plan = app.artboard_export_plan();
        assert_eq!(plan.len(), 1);
        assert!(plan.iter().all(|p| p.artboard_id != id));
    }

    #[test]
    fn naming_and_format_actions() {
        let mut app = app_with_artboards();
        let id = app.artboards[0].id;
        app.apply(Action::SetArtboardExportFormat { id, format: ArtboardExportFormat::Jpeg });
        app.apply(Action::SetArtboardExportNaming {
            id,
            prefix: "p-".into(),
            suffix: "-s".into(),
        });
        app.apply(Action::SetArtboardExportQuality { id, quality: 75 });
        let cfg = app.artboard_export_config(id);
        assert_eq!(cfg.format, ArtboardExportFormat::Jpeg);
        assert_eq!(cfg.quality, 75);
        let plan = app.artboard_export_plan();
        let item = plan.iter().find(|p| p.artboard_id == id).unwrap();
        assert!(item.filename.starts_with("p-"));
        assert!(item.filename.ends_with("-s.jpg"));
        assert_eq!(item.quality, 75);
    }

    #[test]
    fn prepare_export_records_plan() {
        let mut app = app_with_artboards();
        app.apply(Action::PrepareArtboardExport);
        assert_eq!(app.last_artboard_export_plan.len(), 2);
        assert!(app.status_message.as_deref().unwrap().contains("ready to export"));
    }

    #[test]
    fn default_configs_empty() {
        assert!(default_export_configs().is_empty());
    }
}
