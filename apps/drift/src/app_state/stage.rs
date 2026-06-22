use super::{App, Action};

/// Unit used for ruler display on the stage.
#[derive(Clone, Debug, PartialEq)]
pub enum StageRulerUnit {
    Pixels,
    Points,
    Centimeters,
    Inches,
}

/// Document/stage-level settings (canvas size, fps, snapping, auto-save, etc.).
#[derive(Clone, Debug)]
pub struct StageSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: f32,
    pub background_color: [u8; 4],
    pub ruler_unit: StageRulerUnit,
    pub snap_to_objects: bool,
    pub snap_to_pixel: bool,
    pub snap_tolerance: u32,
    pub auto_save: bool,
    pub auto_save_interval_min: u32,
    pub undo_levels: u32,
}

impl StageSettings {
    pub fn new() -> Self {
        Self {
            width: 1920,
            height: 1080,
            frame_rate: 24.0,
            background_color: [0, 0, 0, 255],
            ruler_unit: StageRulerUnit::Pixels,
            snap_to_objects: false,
            snap_to_pixel: false,
            snap_tolerance: 5,
            auto_save: false,
            auto_save_interval_min: 10,
            undo_levels: 50,
        }
    }
}

/// Color label for a scene.
#[derive(Clone, Debug, PartialEq)]
pub enum LabelColor {
    None,
    Red,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
}

/// Extended properties for a single scene.
#[derive(Clone, Debug)]
pub struct SceneProperties {
    pub scene_id: usize,
    pub label_color: LabelColor,
    pub frame_count: u32,
    pub description: String,
}

impl App {
    pub fn apply_stage(&mut self, action: Action) {
        match action {
            Action::SetStageDimensions { width, height } => {
                self.stage.width = width.max(1);
                self.stage.height = height.max(1);
            }
            Action::SetStageFrameRate(fps) => {
                self.stage.frame_rate = fps.clamp(1.0, 120.0);
            }
            Action::SetStageBackgroundColor(color) => {
                self.stage.background_color = color;
            }
            Action::SetStageRulerUnit(unit) => {
                self.stage.ruler_unit = unit;
            }
            Action::SetSnapToObjects(v) => {
                self.stage.snap_to_objects = v;
            }
            Action::SetSnapToPixel(v) => {
                self.stage.snap_to_pixel = v;
            }
            Action::SetAutoSave { enabled, interval_min } => {
                self.stage.auto_save = enabled;
                self.stage.auto_save_interval_min = interval_min.max(1);
            }
            Action::SetUndoLevels(levels) => {
                self.stage.undo_levels = levels.clamp(1, 9999);
            }
            Action::SetSceneLabel { scene_id, color } => {
                let entry = self.scene_properties.entry(scene_id).or_insert_with(|| {
                    SceneProperties {
                        scene_id,
                        label_color: LabelColor::None,
                        frame_count: 0,
                        description: String::new(),
                    }
                });
                entry.label_color = color;
            }
            Action::SetSceneDescription { scene_id, description } => {
                let entry = self.scene_properties.entry(scene_id).or_insert_with(|| {
                    SceneProperties {
                        scene_id,
                        label_color: LabelColor::None,
                        frame_count: 0,
                        description: String::new(),
                    }
                });
                entry.description = description;
            }
            Action::SetSceneFrameCount { scene_id, count } => {
                let entry = self.scene_properties.entry(scene_id).or_insert_with(|| {
                    SceneProperties {
                        scene_id,
                        label_color: LabelColor::None,
                        frame_count: 0,
                        description: String::new(),
                    }
                });
                entry.frame_count = count;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{StageRulerUnit, LabelColor};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_stage_defaults() {
        let a = app();
        assert_eq!(a.stage.width, 1920);
        assert_eq!(a.stage.height, 1080);
        assert_eq!(a.stage.frame_rate, 24.0);
        assert_eq!(a.stage.ruler_unit, StageRulerUnit::Pixels);
        assert!(!a.stage.snap_to_objects);
        assert!(!a.stage.snap_to_pixel);
        assert_eq!(a.stage.undo_levels, 50);
        assert!(!a.stage.auto_save);
        assert_eq!(a.stage.auto_save_interval_min, 10);
    }

    #[test]
    fn test_set_stage_dimensions() {
        let mut a = app();
        a.apply(Action::SetStageDimensions { width: 1280, height: 720 });
        assert_eq!(a.stage.width, 1280);
        assert_eq!(a.stage.height, 720);
    }

    #[test]
    fn test_set_stage_dimensions_min_one() {
        let mut a = app();
        a.apply(Action::SetStageDimensions { width: 0, height: 0 });
        assert_eq!(a.stage.width, 1);
        assert_eq!(a.stage.height, 1);
    }

    #[test]
    fn test_set_stage_frame_rate() {
        let mut a = app();
        a.apply(Action::SetStageFrameRate(60.0));
        assert_eq!(a.stage.frame_rate, 60.0);
    }

    #[test]
    fn test_set_stage_frame_rate_clamped() {
        let mut a = app();
        a.apply(Action::SetStageFrameRate(0.0));
        assert_eq!(a.stage.frame_rate, 1.0);
        a.apply(Action::SetStageFrameRate(500.0));
        assert_eq!(a.stage.frame_rate, 120.0);
    }

    #[test]
    fn test_set_stage_background_color() {
        let mut a = app();
        a.apply(Action::SetStageBackgroundColor([255, 255, 255, 255]));
        assert_eq!(a.stage.background_color, [255, 255, 255, 255]);
    }

    #[test]
    fn test_set_stage_ruler_unit() {
        let mut a = app();
        a.apply(Action::SetStageRulerUnit(StageRulerUnit::Inches));
        assert_eq!(a.stage.ruler_unit, StageRulerUnit::Inches);
    }

    #[test]
    fn test_set_snap_to_objects() {
        let mut a = app();
        a.apply(Action::SetSnapToObjects(true));
        assert!(a.stage.snap_to_objects);
    }

    #[test]
    fn test_set_snap_to_pixel() {
        let mut a = app();
        a.apply(Action::SetSnapToPixel(true));
        assert!(a.stage.snap_to_pixel);
    }

    #[test]
    fn test_set_auto_save() {
        let mut a = app();
        a.apply(Action::SetAutoSave { enabled: true, interval_min: 5 });
        assert!(a.stage.auto_save);
        assert_eq!(a.stage.auto_save_interval_min, 5);
    }

    #[test]
    fn test_set_auto_save_interval_min_clamped() {
        let mut a = app();
        a.apply(Action::SetAutoSave { enabled: true, interval_min: 0 });
        assert_eq!(a.stage.auto_save_interval_min, 1);
    }

    #[test]
    fn test_set_undo_levels() {
        let mut a = app();
        a.apply(Action::SetUndoLevels(100));
        assert_eq!(a.stage.undo_levels, 100);
    }

    #[test]
    fn test_set_undo_levels_clamped() {
        let mut a = app();
        a.apply(Action::SetUndoLevels(0));
        assert_eq!(a.stage.undo_levels, 1);
    }

    #[test]
    fn test_set_scene_label() {
        let mut a = app();
        a.apply(Action::SetSceneLabel { scene_id: 0, color: LabelColor::Red });
        assert_eq!(a.scene_properties[&0].label_color, LabelColor::Red);
    }

    #[test]
    fn test_set_scene_description() {
        let mut a = app();
        a.apply(Action::SetSceneDescription { scene_id: 0, description: "Intro".to_string() });
        assert_eq!(a.scene_properties[&0].description, "Intro");
    }

    #[test]
    fn test_set_scene_frame_count() {
        let mut a = app();
        a.apply(Action::SetSceneFrameCount { scene_id: 0, count: 120 });
        assert_eq!(a.scene_properties[&0].frame_count, 120);
    }

    #[test]
    fn test_scene_properties_independent_per_scene() {
        let mut a = app();
        a.apply(Action::SetSceneLabel { scene_id: 0, color: LabelColor::Blue });
        a.apply(Action::SetSceneLabel { scene_id: 1, color: LabelColor::Green });
        assert_eq!(a.scene_properties[&0].label_color, LabelColor::Blue);
        assert_eq!(a.scene_properties[&1].label_color, LabelColor::Green);
    }
}
