//! Surround sound domain — format, bus config, spatial panning, Atmos objects.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum SurroundFormat {
    Stereo,
    Surround51,
    Surround71,
    Atmos,
}

#[derive(Clone, Debug)]
pub struct SurroundPan {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub size: f32,
}

impl Default for SurroundPan {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0, size: 0.0 }
    }
}

#[derive(Clone, Debug)]
pub struct SurroundBusConfig {
    pub format: SurroundFormat,
    pub enabled: bool,
    pub binaural_monitor: bool,
}

#[derive(Clone, Debug)]
pub struct AtmosObject {
    pub id: usize,
    pub track_id: usize,
    pub pan: SurroundPan,
    pub gain: f32,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_surround(&mut self, action: Action) {
        match action {
            Action::SetSurroundFormat { format } => {
                self.surround_bus.format = format;
            }
            Action::ToggleSurroundBus => {
                self.surround_bus.enabled = !self.surround_bus.enabled;
            }
            Action::ToggleBinauralMonitor => {
                self.surround_bus.binaural_monitor = !self.surround_bus.binaural_monitor;
            }
            Action::SetTrackSurroundPan { track_id, pan } => {
                if let Some(entry) = self.surround_pans.iter_mut().find(|(tid, _)| *tid == track_id) {
                    entry.1 = pan;
                } else {
                    self.surround_pans.push((track_id, pan));
                }
            }
            Action::CreateAtmosObject { track_id } => {
                let id = self.next_atmos_id;
                self.next_atmos_id += 1;
                self.atmos_objects.push(AtmosObject {
                    id,
                    track_id,
                    pan: SurroundPan::default(),
                    gain: 1.0,
                });
            }
            Action::SetAtmosPan { object_id, pan } => {
                if let Some(obj) = self.atmos_objects.iter_mut().find(|o| o.id == object_id) {
                    obj.pan = pan;
                }
            }
            Action::RemoveAtmosObject { object_id } => {
                self.atmos_objects.retain(|o| o.id != object_id);
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
    fn set_surround_format_stereo_to_51() {
        let mut app = fresh();
        app.apply(Action::SetSurroundFormat { format: SurroundFormat::Surround51 });
        assert_eq!(app.surround_bus.format, SurroundFormat::Surround51);
    }

    #[test]
    fn toggle_surround_bus_enables() {
        let mut app = fresh();
        assert!(!app.surround_bus.enabled);
        app.apply(Action::ToggleSurroundBus);
        assert!(app.surround_bus.enabled);
    }

    #[test]
    fn toggle_surround_bus_disables() {
        let mut app = fresh();
        app.apply(Action::ToggleSurroundBus);
        app.apply(Action::ToggleSurroundBus);
        assert!(!app.surround_bus.enabled);
    }

    #[test]
    fn toggle_binaural_monitor() {
        let mut app = fresh();
        assert!(!app.surround_bus.binaural_monitor);
        app.apply(Action::ToggleBinauralMonitor);
        assert!(app.surround_bus.binaural_monitor);
        app.apply(Action::ToggleBinauralMonitor);
        assert!(!app.surround_bus.binaural_monitor);
    }

    #[test]
    fn set_track_surround_pan_inserts_new() {
        let mut app = fresh();
        let pan = SurroundPan { x: 0.5, y: -0.5, z: 0.0, size: 1.0 };
        app.apply(Action::SetTrackSurroundPan { track_id: 3, pan: pan.clone() });
        assert_eq!(app.surround_pans.len(), 1);
        assert!((app.surround_pans[0].1.x - 0.5).abs() < 0.001);
    }

    #[test]
    fn set_track_surround_pan_updates_existing() {
        let mut app = fresh();
        let pan1 = SurroundPan { x: 0.1, y: 0.0, z: 0.0, size: 0.0 };
        let pan2 = SurroundPan { x: 0.9, y: 0.0, z: 0.0, size: 0.0 };
        app.apply(Action::SetTrackSurroundPan { track_id: 5, pan: pan1 });
        app.apply(Action::SetTrackSurroundPan { track_id: 5, pan: pan2 });
        assert_eq!(app.surround_pans.len(), 1);
        assert!((app.surround_pans[0].1.x - 0.9).abs() < 0.001);
    }

    #[test]
    fn create_atmos_object() {
        let mut app = fresh();
        app.apply(Action::CreateAtmosObject { track_id: 2 });
        assert_eq!(app.atmos_objects.len(), 1);
        assert_eq!(app.atmos_objects[0].track_id, 2);
        assert!((app.atmos_objects[0].gain - 1.0).abs() < 0.001);
    }

    #[test]
    fn create_multiple_atmos_objects_unique_ids() {
        let mut app = fresh();
        app.apply(Action::CreateAtmosObject { track_id: 1 });
        app.apply(Action::CreateAtmosObject { track_id: 1 });
        let ids: Vec<usize> = app.atmos_objects.iter().map(|o| o.id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn set_atmos_pan() {
        let mut app = fresh();
        app.apply(Action::CreateAtmosObject { track_id: 1 });
        let oid = app.atmos_objects[0].id;
        let pan = SurroundPan { x: 0.3, y: 0.7, z: 0.1, size: 0.5 };
        app.apply(Action::SetAtmosPan { object_id: oid, pan });
        assert!((app.atmos_objects[0].pan.y - 0.7).abs() < 0.001);
    }

    #[test]
    fn remove_atmos_object() {
        let mut app = fresh();
        app.apply(Action::CreateAtmosObject { track_id: 1 });
        app.apply(Action::CreateAtmosObject { track_id: 2 });
        let oid = app.atmos_objects[0].id;
        app.apply(Action::RemoveAtmosObject { object_id: oid });
        assert_eq!(app.atmos_objects.len(), 1);
    }

    #[test]
    fn set_surround_format_atmos() {
        let mut app = fresh();
        app.apply(Action::SetSurroundFormat { format: SurroundFormat::Atmos });
        assert_eq!(app.surround_bus.format, SurroundFormat::Atmos);
    }
}
