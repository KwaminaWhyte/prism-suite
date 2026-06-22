//! Bus / Return track routing domain — bus tracks, master bus, send routing.

use super::{Action, App};
use super::tracks::{InsertEffect, InsertEffectKind};

// ─── Types ────────────────────────────────────────────────────────────────────

pub struct BusTrack {
    pub id: usize,
    pub name: String,
    pub color: String,
    pub volume: f32,
    pub pan: f32,
    pub muted: bool,
    pub insert_effects: Vec<InsertEffect>,
    pub receive_levels: Vec<(usize, f32)>,
}

impl BusTrack {
    pub fn new(id: usize, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            color: "#888888".to_string(),
            volume: 1.0,
            pan: 0.0,
            muted: false,
            insert_effects: Vec::new(),
            receive_levels: Vec::new(),
        }
    }
}

pub struct MasterBus {
    pub volume: f32,
    pub limiter_threshold: f32,
    pub limiter_release: f32,
    pub dither: bool,
    pub dither_bits: u8,
    pub insert_effects: Vec<InsertEffect>,
    pub auto_normalize: bool,
}

impl MasterBus {
    pub fn new() -> Self {
        Self {
            volume: 1.0,
            limiter_threshold: -1.0,
            limiter_release: 100.0,
            dither: false,
            dither_bits: 24,
            insert_effects: Vec::new(),
            auto_normalize: false,
        }
    }
}

impl Default for MasterBus {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Apply methods ────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_bus_routing(&mut self, action: Action) {
        match action {
            Action::AddBusTrack { name } => {
                let id = self.next_bus_track_id;
                self.next_bus_track_id += 1;
                self.bus_tracks.push(BusTrack::new(id, name));
            }
            Action::DeleteBusTrack { bus_id } => {
                self.bus_tracks.retain(|b| b.id != bus_id);
            }
            Action::RenameBusTrack { bus_id, name } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    b.name = name;
                }
            }
            Action::SetBusVolume { bus_id, volume } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    b.volume = volume.clamp(0.0, 2.0);
                }
            }
            Action::SetBusPan { bus_id, pan } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    b.pan = pan.clamp(-1.0, 1.0);
                }
            }
            Action::MuteBusTrack { bus_id, muted } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    b.muted = muted;
                }
            }
            Action::AddBusInsertEffect { bus_id, kind } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    let new_id = b.insert_effects.iter().map(|e| e.id).max().map(|m| m + 1).unwrap_or(0);
                    b.insert_effects.push(InsertEffect {
                        id: new_id,
                        effect_type: kind,
                        enabled: true,
                        gain_in: 1.0,
                        gain_out: 1.0,
                    });
                }
            }
            Action::RemoveBusInsertEffect { bus_id, effect_id } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    b.insert_effects.retain(|e| e.id != effect_id);
                }
            }
            Action::SetSendToBus { from_track_id, bus_id, level } => {
                if let Some(b) = self.bus_tracks.iter_mut().find(|b| b.id == bus_id) {
                    let clamped = level.clamp(0.0, 1.0);
                    if let Some(entry) = b.receive_levels.iter_mut().find(|(tid, _)| *tid == from_track_id) {
                        entry.1 = clamped;
                    } else {
                        b.receive_levels.push((from_track_id, clamped));
                    }
                }
            }
            Action::SetMasterBusVolume(volume) => {
                self.master_bus.volume = volume.clamp(0.0, 2.0);
            }
            Action::SetMasterLimiterThreshold(threshold) => {
                self.master_bus.limiter_threshold = threshold;
            }
            Action::SetMasterLimiterRelease(release) => {
                self.master_bus.limiter_release = release;
            }
            Action::SetMasterDither { enabled, bits } => {
                if bits == 16 || bits == 24 {
                    self.master_bus.dither = enabled;
                    self.master_bus.dither_bits = bits;
                }
            }
            Action::SetMasterAutoNormalize(enabled) => {
                self.master_bus.auto_normalize = enabled;
            }
            Action::AddMasterInsertEffect(kind) => {
                let new_id = self.master_bus.insert_effects.iter().map(|e| e.id).max().map(|m| m + 1).unwrap_or(0);
                self.master_bus.insert_effects.push(InsertEffect {
                    id: new_id,
                    effect_type: kind,
                    enabled: true,
                    gain_in: 1.0,
                    gain_out: 1.0,
                });
            }
            Action::RemoveMasterInsertEffect { index } => {
                if index < self.master_bus.insert_effects.len() {
                    self.master_bus.insert_effects.remove(index);
                }
            }
            _ => {}
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{Action, App};
    use super::super::tracks::InsertEffectKind;

    fn fresh() -> App {
        App::new()
    }

    #[test]
    fn bus_routing_defaults() {
        let app = fresh();
        assert!(app.bus_tracks.is_empty());
        assert_eq!(app.next_bus_track_id, 0);
        assert!((app.master_bus.volume - 1.0).abs() < 0.001);
        assert!((app.master_bus.limiter_threshold - -1.0).abs() < 0.001);
        assert!(!app.master_bus.dither);
        assert_eq!(app.master_bus.dither_bits, 24);
        assert!(!app.master_bus.auto_normalize);
    }

    #[test]
    fn add_bus_track() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Reverb Bus".to_string() });
        assert_eq!(app.bus_tracks.len(), 1);
        assert_eq!(app.bus_tracks[0].name, "Reverb Bus");
        assert_eq!(app.bus_tracks[0].id, 0);
        assert!((app.bus_tracks[0].volume - 1.0).abs() < 0.001);
    }

    #[test]
    fn delete_bus_track() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus 1".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::DeleteBusTrack { bus_id: bid });
        assert!(app.bus_tracks.is_empty());
    }

    #[test]
    fn rename_bus_track() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Old".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::RenameBusTrack { bus_id: bid, name: "New".to_string() });
        assert_eq!(app.bus_tracks[0].name, "New");
    }

    #[test]
    fn set_bus_volume_clamped() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::SetBusVolume { bus_id: bid, volume: 5.0 });
        assert!((app.bus_tracks[0].volume - 2.0).abs() < 0.001);
        app.apply(Action::SetBusVolume { bus_id: bid, volume: -1.0 });
        assert_eq!(app.bus_tracks[0].volume, 0.0);
    }

    #[test]
    fn set_bus_pan_clamped() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::SetBusPan { bus_id: bid, pan: 2.0 });
        assert!((app.bus_tracks[0].pan - 1.0).abs() < 0.001);
        app.apply(Action::SetBusPan { bus_id: bid, pan: -2.0 });
        assert!((app.bus_tracks[0].pan - -1.0).abs() < 0.001);
    }

    #[test]
    fn mute_bus_track() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::MuteBusTrack { bus_id: bid, muted: true });
        assert!(app.bus_tracks[0].muted);
    }

    #[test]
    fn add_bus_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::AddBusInsertEffect { bus_id: bid, kind: InsertEffectKind::Reverb { room_size: 0.5, wet: 0.3 } });
        assert_eq!(app.bus_tracks[0].insert_effects.len(), 1);
        assert_eq!(app.bus_tracks[0].insert_effects[0].id, 0);
        assert!(app.bus_tracks[0].insert_effects[0].enabled);
    }

    #[test]
    fn remove_bus_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::AddBusInsertEffect { bus_id: bid, kind: InsertEffectKind::Compressor });
        let eid = app.bus_tracks[0].insert_effects[0].id;
        app.apply(Action::RemoveBusInsertEffect { bus_id: bid, effect_id: eid });
        assert!(app.bus_tracks[0].insert_effects.is_empty());
    }

    #[test]
    fn set_send_to_bus() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::SetSendToBus { from_track_id: 1, bus_id: bid, level: 0.7 });
        assert_eq!(app.bus_tracks[0].receive_levels.len(), 1);
        assert!((app.bus_tracks[0].receive_levels[0].1 - 0.7).abs() < 0.001);
        // Update existing
        app.apply(Action::SetSendToBus { from_track_id: 1, bus_id: bid, level: 0.5 });
        assert_eq!(app.bus_tracks[0].receive_levels.len(), 1);
        assert!((app.bus_tracks[0].receive_levels[0].1 - 0.5).abs() < 0.001);
    }

    #[test]
    fn set_send_to_bus_level_clamped() {
        let mut app = fresh();
        app.apply(Action::AddBusTrack { name: "Bus".to_string() });
        let bid = app.bus_tracks[0].id;
        app.apply(Action::SetSendToBus { from_track_id: 1, bus_id: bid, level: 5.0 });
        assert!((app.bus_tracks[0].receive_levels[0].1 - 1.0).abs() < 0.001);
    }

    #[test]
    fn set_master_bus_volume() {
        let mut app = fresh();
        app.apply(Action::SetMasterBusVolume(0.8));
        assert!((app.master_bus.volume - 0.8).abs() < 0.001);
        app.apply(Action::SetMasterBusVolume(5.0));
        assert!((app.master_bus.volume - 2.0).abs() < 0.001);
    }

    #[test]
    fn set_master_limiter() {
        let mut app = fresh();
        app.apply(Action::SetMasterLimiterThreshold(-3.0));
        app.apply(Action::SetMasterLimiterRelease(50.0));
        assert!((app.master_bus.limiter_threshold - -3.0).abs() < 0.001);
        assert!((app.master_bus.limiter_release - 50.0).abs() < 0.001);
    }

    #[test]
    fn set_master_dither_valid() {
        let mut app = fresh();
        app.apply(Action::SetMasterDither { enabled: true, bits: 16 });
        assert!(app.master_bus.dither);
        assert_eq!(app.master_bus.dither_bits, 16);
    }

    #[test]
    fn set_master_dither_invalid_noop() {
        let mut app = fresh();
        app.apply(Action::SetMasterDither { enabled: true, bits: 32 });
        assert!(!app.master_bus.dither); // unchanged
        assert_eq!(app.master_bus.dither_bits, 24); // unchanged
    }

    #[test]
    fn add_master_insert_effect() {
        let mut app = fresh();
        app.apply(Action::AddMasterInsertEffect(InsertEffectKind::Limiter { ceiling_db: -0.3 }));
        assert_eq!(app.master_bus.insert_effects.len(), 1);
        app.apply(Action::AddMasterInsertEffect(InsertEffectKind::Compressor));
        assert_eq!(app.master_bus.insert_effects.len(), 2);
        assert_eq!(app.master_bus.insert_effects[1].id, 1);
    }

    #[test]
    fn remove_master_insert_effect_by_index() {
        let mut app = fresh();
        app.apply(Action::AddMasterInsertEffect(InsertEffectKind::Compressor));
        app.apply(Action::AddMasterInsertEffect(InsertEffectKind::Eq3Band));
        app.apply(Action::RemoveMasterInsertEffect { index: 0 });
        assert_eq!(app.master_bus.insert_effects.len(), 1);
    }

    #[test]
    fn remove_master_insert_effect_out_of_bounds_noop() {
        let mut app = fresh();
        app.apply(Action::RemoveMasterInsertEffect { index: 0 }); // no panic
        assert!(app.master_bus.insert_effects.is_empty());
    }
}
