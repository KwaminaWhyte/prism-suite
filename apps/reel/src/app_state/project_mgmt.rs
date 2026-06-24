//! **Project management: relink / consolidate / offline-online.**
//!
//! Models the media-management operations Premiere groups under "Project
//! Manager" and the offline/online workflow, operating on the project's
//! [`MediaItem`](super::reel_project::MediaItem) list:
//!
//! - **Offline / online**: a per-item flag plus bulk mark-offline / mark-online.
//! - **Relink**: remap one item's path to a new location and bring it back online.
//! - **Consolidate**: collect the project's *referenced* media into a destination
//!   folder, producing a [`ConsolidateManifest`] (a pure plan: source → dest path
//!   for every item, with duplicates de-duped). No files are moved here — the
//!   manifest is what an executor would act on, which keeps it unit-testable.

use std::path::{Path, PathBuf};

use super::{App, Action};
use super::reel_project::MediaItem;

/// One planned copy in a consolidate operation: where the media lives now and
/// where it should be collected to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsolidateEntry {
    pub item_id: usize,
    pub source: String,
    pub dest: PathBuf,
}

/// The plan produced by [`App::consolidate_manifest`]: the per-item copy plan
/// plus the offline items that were skipped (their media can't be collected).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConsolidateManifest {
    pub destination: PathBuf,
    pub entries: Vec<ConsolidateEntry>,
    /// Item ids that were offline (skipped — nothing to collect).
    pub skipped_offline: Vec<usize>,
}

impl ConsolidateManifest {
    /// The number of distinct files the manifest would copy.
    pub fn file_count(&self) -> usize {
        self.entries.len()
    }
}

/// The destination file name (stem + extension) for a media `source` path,
/// falling back to `media` when the path has no file name.
fn dest_file_name(source: &str) -> String {
    Path::new(source)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "media".to_string())
}

impl App {
    /// Mark media item `item_id` offline (its source can't be found / was
    /// unmounted). Returns `true` if the item existed.
    pub fn set_media_item_offline(&mut self, item_id: usize, offline: bool) -> bool {
        if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
            m.offline = offline;
            true
        } else {
            false
        }
    }

    /// Mark **every** media item offline / online in bulk. Returns the count
    /// changed.
    pub fn set_all_media_offline(&mut self, offline: bool) -> usize {
        let mut n = 0;
        for m in &mut self.media_items {
            if m.offline != offline {
                m.offline = offline;
                n += 1;
            }
        }
        n
    }

    /// Relink media item `item_id` to `new_path`: remap its path and bring it
    /// back online. Returns `true` if the item existed.
    pub fn relink_media_item(&mut self, item_id: usize, new_path: impl Into<String>) -> bool {
        if let Some(m) = self.media_items.iter_mut().find(|m| m.id == item_id) {
            m.path = new_path.into();
            m.offline = false;
            true
        } else {
            false
        }
    }

    /// Build a consolidate manifest collecting every **online** media item into
    /// `destination`. Duplicate source paths collapse to one entry; clashing
    /// destination file names are disambiguated with an `_N` suffix. Offline
    /// items are recorded in `skipped_offline`. Pure (no filesystem I/O).
    pub fn consolidate_manifest(&self, destination: impl Into<PathBuf>) -> ConsolidateManifest {
        let destination = destination.into();
        let mut entries: Vec<ConsolidateEntry> = Vec::new();
        let mut skipped_offline: Vec<usize> = Vec::new();
        let mut seen_sources: Vec<String> = Vec::new();
        let mut used_names: Vec<String> = Vec::new();

        for item in &self.media_items {
            if item.offline {
                skipped_offline.push(item.id);
                continue;
            }
            if item.path.is_empty() {
                continue;
            }
            // De-dup by source path (the same media used twice is collected once).
            if seen_sources.iter().any(|s| s == &item.path) {
                continue;
            }
            seen_sources.push(item.path.clone());

            // Disambiguate destination file names that collide.
            let mut name = dest_file_name(&item.path);
            if used_names.contains(&name) {
                let (stem, ext) = split_name(&name);
                let mut i = 1;
                loop {
                    let candidate = if ext.is_empty() {
                        format!("{stem}_{i}")
                    } else {
                        format!("{stem}_{i}.{ext}")
                    };
                    if !used_names.contains(&candidate) {
                        name = candidate;
                        break;
                    }
                    i += 1;
                }
            }
            used_names.push(name.clone());

            entries.push(ConsolidateEntry {
                item_id: item.id,
                source: item.path.clone(),
                dest: destination.join(name),
            });
        }

        ConsolidateManifest { destination, entries, skipped_offline }
    }

    /// Add a media item to the project's media list, returning its id.
    pub fn add_media_item(&mut self, mut item: MediaItem) -> usize {
        let id = self.next_media_item_id;
        item.id = id;
        self.next_media_item_id += 1;
        self.media_items.push(item);
        id
    }

    /// Apply the project-management actions (relink / offline-online /
    /// consolidate). Routed from the Batch-5 dispatcher.
    pub(crate) fn apply_project_mgmt(&mut self, action: Action) {
        match action {
            Action::SetAllMediaOffline(offline) => {
                self.set_all_media_offline(offline);
            }
            Action::ConsolidateProject { destination } => {
                let manifest = self.consolidate_manifest(&destination);
                self.last_consolidate_manifest = Some(manifest);
            }
            _ => {}
        }
    }
}

/// Split `name` into `(stem, ext)`; `ext` is empty when there's no dot.
fn split_name(name: &str) -> (String, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), ext.to_string()),
        _ => (name.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::Action;
    use super::super::reel_project::{BinColor, MediaItem, MediaKind};

    fn item(name: &str, path: &str) -> MediaItem {
        MediaItem {
            id: 0,
            name: name.to_string(),
            path: path.to_string(),
            duration_s: 1.0,
            frame_rate: 30.0,
            width: 1920,
            height: 1080,
            has_audio: true,
            has_video: true,
            media_kind: MediaKind::Video,
            proxy_path: None,
            label: BinColor::None,
            offline: false,
            log_note: String::new(),
        }
    }

    #[test]
    fn relink_updates_path_and_clears_offline() {
        let mut app = App::new();
        let id = app.add_media_item(item("shot", "/old/shot.mov"));
        app.set_media_item_offline(id, true);
        assert!(app.media_items[0].offline);
        // Relink → new path + back online.
        assert!(app.relink_media_item(id, "/new/shot.mov"));
        assert_eq!(app.media_items[0].path, "/new/shot.mov");
        assert!(!app.media_items[0].offline);
        // Relinking a missing id is a no-op false.
        assert!(!app.relink_media_item(9999, "/x"));
    }

    #[test]
    fn offline_online_single_and_bulk() {
        let mut app = App::new();
        let a = app.add_media_item(item("a", "/m/a.mov"));
        let b = app.add_media_item(item("b", "/m/b.mov"));
        assert!(app.set_media_item_offline(a, true));
        assert!(app.media_items.iter().find(|m| m.id == a).unwrap().offline);
        // Bulk offline marks the remaining online item (a is already offline).
        let changed = app.set_all_media_offline(true);
        assert_eq!(changed, 1, "only b changed");
        assert!(app.media_items.iter().all(|m| m.offline));
        // Bulk online brings both back.
        let changed = app.set_all_media_offline(false);
        assert_eq!(changed, 2);
        assert!(app.media_items.iter().all(|m| !m.offline));
        let _ = b;
    }

    #[test]
    fn consolidate_collects_online_media_into_folder() {
        let mut app = App::new();
        app.add_media_item(item("a", "/footage/a.mov"));
        app.add_media_item(item("b", "/audio/b.wav"));
        let off = app.add_media_item(item("c", "/footage/c.mov"));
        app.set_media_item_offline(off, true);

        let manifest = app.consolidate_manifest("/collect");
        assert_eq!(manifest.destination, PathBuf::from("/collect"));
        assert_eq!(manifest.file_count(), 2, "online media only");
        assert_eq!(manifest.skipped_offline, vec![off]);
        // Destinations land under the collect folder, keeping file names.
        let dests: Vec<_> = manifest.entries.iter().map(|e| e.dest.clone()).collect();
        assert!(dests.contains(&PathBuf::from("/collect/a.mov")));
        assert!(dests.contains(&PathBuf::from("/collect/b.wav")));
    }

    #[test]
    fn consolidate_dedups_sources_and_disambiguates_names() {
        let mut app = App::new();
        // Two items pointing at the same source → one entry.
        app.add_media_item(item("a1", "/footage/a.mov"));
        app.add_media_item(item("a2", "/footage/a.mov"));
        // A different source that shares a file name with the first.
        app.add_media_item(item("a3", "/other/a.mov"));

        let manifest = app.consolidate_manifest("/c");
        assert_eq!(manifest.file_count(), 2, "duplicate source collapsed");
        let dests: Vec<_> = manifest.entries.iter().map(|e| e.dest.clone()).collect();
        // First keeps a.mov, the name clash gets _1.
        assert!(dests.contains(&PathBuf::from("/c/a.mov")));
        assert!(dests.contains(&PathBuf::from("/c/a_1.mov")));
    }

    #[test]
    fn consolidate_action_stores_manifest() {
        let mut app = App::new();
        app.add_media_item(item("a", "/m/a.mov"));
        app.apply(Action::ConsolidateProject { destination: "/dest".to_string() });
        let m = app.last_consolidate_manifest.as_ref().expect("manifest stored");
        assert_eq!(m.file_count(), 1);
        assert_eq!(m.entries[0].dest, PathBuf::from("/dest/a.mov"));
    }

    #[test]
    fn set_all_media_offline_action() {
        let mut app = App::new();
        app.add_media_item(item("a", "/m/a.mov"));
        app.apply(Action::SetAllMediaOffline(true));
        assert!(app.media_items.iter().all(|m| m.offline));
    }
}
