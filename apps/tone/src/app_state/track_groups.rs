//! Track groups domain — `TrackGroup` type + group apply methods + tests.

// ─── Types ────────────────────────────────────────────────────────────────────

/// A named group of tracks that can be collapsed/expanded and color-coded.
#[derive(Clone, Debug)]
pub struct TrackGroup {
    pub id: usize,
    pub name: String,
    pub track_ids: Vec<usize>,
    pub collapsed: bool,
    pub color: String,
}

// ─── Apply methods ────────────────────────────────────────────────────────────

use super::{Action, App};
use super::tracks::ToneTrack;

impl App {
    /// Return all tracks that belong to the given group.
    pub fn tracks_in_group(&self, group_id: usize) -> Vec<&ToneTrack> {
        let Some(group) = self.track_groups.iter().find(|g| g.id == group_id) else {
            return Vec::new();
        };
        self.tracks.iter()
            .filter(|t| group.track_ids.contains(&t.id))
            .collect()
    }

    pub(super) fn apply_track_groups(&mut self, action: Action) {
        match action {
            Action::CreateTrackGroup { name, track_ids } => {
                let id = self.next_group_id;
                self.next_group_id += 1;
                self.track_groups.push(TrackGroup {
                    id,
                    name,
                    track_ids,
                    collapsed: false,
                    color: "#6366F1".to_string(),
                });
            }
            Action::AddTrackToGroup { group_id, track_id } => {
                if let Some(g) = self.track_groups.iter_mut().find(|g| g.id == group_id) {
                    if !g.track_ids.contains(&track_id) {
                        g.track_ids.push(track_id);
                    }
                }
            }
            Action::RemoveTrackFromGroup { group_id, track_id } => {
                if let Some(g) = self.track_groups.iter_mut().find(|g| g.id == group_id) {
                    g.track_ids.retain(|&id| id != track_id);
                }
            }
            Action::CollapseGroup { group_id, collapsed } => {
                if let Some(g) = self.track_groups.iter_mut().find(|g| g.id == group_id) {
                    g.collapsed = collapsed;
                }
            }
            Action::DeleteTrackGroup { group_id } => {
                self.track_groups.retain(|g| g.id != group_id);
            }
            Action::SetGroupColor { group_id, color } => {
                if let Some(g) = self.track_groups.iter_mut().find(|g| g.id == group_id) {
                    g.color = color;
                }
            }
            Action::RenameTrackGroup { group_id, name } => {
                if let Some(g) = self.track_groups.iter_mut().find(|g| g.id == group_id) {
                    g.name = name;
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
    use super::super::tracks::TrackKind;

    fn fresh() -> App {
        App::new()
    }

    fn add_audio_track(app: &mut App) -> usize {
        app.apply(Action::AddTrack(TrackKind::Audio));
        app.tracks.last().unwrap().id
    }

    #[test]
    fn create_track_group() {
        let mut app = fresh();
        let t1 = add_audio_track(&mut app);
        let t2 = add_audio_track(&mut app);
        app.apply(Action::CreateTrackGroup {
            name: "Drums".to_string(),
            track_ids: vec![t1, t2],
        });
        assert_eq!(app.track_groups.len(), 1);
        assert_eq!(app.track_groups[0].name, "Drums");
        assert_eq!(app.track_groups[0].track_ids, vec![t1, t2]);
        assert!(!app.track_groups[0].collapsed);
    }

    #[test]
    fn create_group_increments_id() {
        let mut app = fresh();
        app.apply(Action::CreateTrackGroup { name: "G1".to_string(), track_ids: vec![] });
        app.apply(Action::CreateTrackGroup { name: "G2".to_string(), track_ids: vec![] });
        assert_eq!(app.track_groups[0].id, 0);
        assert_eq!(app.track_groups[1].id, 1);
    }

    #[test]
    fn add_track_to_group() {
        let mut app = fresh();
        let t1 = add_audio_track(&mut app);
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![] });
        let gid = app.track_groups[0].id;
        app.apply(Action::AddTrackToGroup { group_id: gid, track_id: t1 });
        assert!(app.track_groups[0].track_ids.contains(&t1));
    }

    #[test]
    fn add_track_to_group_no_duplicates() {
        let mut app = fresh();
        let t1 = add_audio_track(&mut app);
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![t1] });
        let gid = app.track_groups[0].id;
        app.apply(Action::AddTrackToGroup { group_id: gid, track_id: t1 });
        assert_eq!(app.track_groups[0].track_ids.len(), 1);
    }

    #[test]
    fn remove_track_from_group() {
        let mut app = fresh();
        let t1 = add_audio_track(&mut app);
        let t2 = add_audio_track(&mut app);
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![t1, t2] });
        let gid = app.track_groups[0].id;
        app.apply(Action::RemoveTrackFromGroup { group_id: gid, track_id: t1 });
        assert!(!app.track_groups[0].track_ids.contains(&t1));
        assert!(app.track_groups[0].track_ids.contains(&t2));
    }

    #[test]
    fn collapse_group() {
        let mut app = fresh();
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![] });
        let gid = app.track_groups[0].id;
        assert!(!app.track_groups[0].collapsed);
        app.apply(Action::CollapseGroup { group_id: gid, collapsed: true });
        assert!(app.track_groups[0].collapsed);
        app.apply(Action::CollapseGroup { group_id: gid, collapsed: false });
        assert!(!app.track_groups[0].collapsed);
    }

    #[test]
    fn delete_track_group() {
        let mut app = fresh();
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![] });
        assert_eq!(app.track_groups.len(), 1);
        let gid = app.track_groups[0].id;
        app.apply(Action::DeleteTrackGroup { group_id: gid });
        assert!(app.track_groups.is_empty());
    }

    #[test]
    fn set_group_color() {
        let mut app = fresh();
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![] });
        let gid = app.track_groups[0].id;
        app.apply(Action::SetGroupColor { group_id: gid, color: "#FF0000".to_string() });
        assert_eq!(app.track_groups[0].color, "#FF0000");
    }

    #[test]
    fn rename_track_group() {
        let mut app = fresh();
        app.apply(Action::CreateTrackGroup { name: "Old".to_string(), track_ids: vec![] });
        let gid = app.track_groups[0].id;
        app.apply(Action::RenameTrackGroup { group_id: gid, name: "New Name".to_string() });
        assert_eq!(app.track_groups[0].name, "New Name");
    }

    #[test]
    fn tracks_in_group_helper() {
        let mut app = fresh();
        let t1 = add_audio_track(&mut app);
        let t2 = add_audio_track(&mut app);
        app.apply(Action::AddTrack(TrackKind::Midi));
        let _t3 = app.tracks.last().unwrap().id;
        app.apply(Action::CreateTrackGroup { name: "G".to_string(), track_ids: vec![t1, t2] });
        let gid = app.track_groups[0].id;
        let in_group = app.tracks_in_group(gid);
        assert_eq!(in_group.len(), 2);
        assert!(in_group.iter().any(|t| t.id == t1));
        assert!(in_group.iter().any(|t| t.id == t2));
    }

    #[test]
    fn tracks_in_group_nonexistent_returns_empty() {
        let app = fresh();
        let result = app.tracks_in_group(9999);
        assert!(result.is_empty());
    }

    #[test]
    fn delete_nonexistent_group_is_noop() {
        let mut app = fresh();
        // Should not panic
        app.apply(Action::DeleteTrackGroup { group_id: 9999 });
        assert!(app.track_groups.is_empty());
    }
}
