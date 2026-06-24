//! **Disk Cache Manager** — the data model behind Pulse's disk cache (After
//! Effects' *Media & Disk Cache*).
//!
//! Tracks a cache directory, a maximum size, and a list of cached entries with
//! their byte size and a logical access clock for **LRU** eviction. `touch`
//! records a use; `purge` empties the cache; `evict_to_fit` removes the
//! least-recently-used entries until the cache is back under its size limit.
//! Pure in-memory accounting — it never touches the filesystem in tests — so the
//! eviction policy is fully deterministic and unit-testable.

use std::path::PathBuf;

use super::{App, Action};

/// One cached artifact (a pre-rendered frame, a peak file, a conformed audio
/// clip, …). `last_access` is a monotonic logical clock, not wall time, so
/// eviction order is reproducible.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheEntry {
    /// Stable key (e.g. a frame hash or relative filename).
    pub key: String,
    /// Size in bytes.
    pub size: u64,
    /// Logical access tick — higher = more recently used.
    pub last_access: u64,
}

/// The disk-cache accounting model.
#[derive(Clone, Debug)]
pub struct DiskCacheManager {
    /// Where cached files live.
    pub cache_dir: PathBuf,
    /// Maximum cache size in bytes (eviction target).
    pub max_size_bytes: u64,
    /// Live entries, keyed by insertion; LRU order is by `last_access`.
    pub entries: Vec<CacheEntry>,
    /// Monotonic logical clock incremented on every insert/touch.
    clock: u64,
}

impl Default for DiskCacheManager {
    fn default() -> Self {
        Self {
            cache_dir: PathBuf::from("~/Library/Caches/PulseDiskCache"),
            // 30 GB default, matching AE's stock disk-cache size.
            max_size_bytes: 30 * 1024 * 1024 * 1024,
            entries: Vec::new(),
            clock: 0,
        }
    }
}

impl DiskCacheManager {
    /// A new manager rooted at `dir` with a `max_gb` gigabyte limit.
    pub fn new(dir: impl Into<PathBuf>, max_gb: f32) -> Self {
        Self {
            cache_dir: dir.into(),
            max_size_bytes: (max_gb.max(0.0) as f64 * 1024.0 * 1024.0 * 1024.0) as u64,
            entries: Vec::new(),
            clock: 0,
        }
    }

    /// Current total bytes used by all cached entries.
    pub fn current_usage(&self) -> u64 {
        self.entries.iter().map(|e| e.size).sum()
    }

    /// Fraction of the cache that is full (0.0–1.0+; can exceed 1 before an
    /// eviction pass runs).
    pub fn usage_fraction(&self) -> f32 {
        if self.max_size_bytes == 0 {
            return 0.0;
        }
        self.current_usage() as f32 / self.max_size_bytes as f32
    }

    /// Insert (or update) a cache entry of `size` bytes under `key`, then evict
    /// LRU entries until the cache fits its limit. Returns the keys evicted.
    pub fn insert(&mut self, key: impl Into<String>, size: u64) -> Vec<String> {
        let key = key.into();
        self.clock += 1;
        if let Some(e) = self.entries.iter_mut().find(|e| e.key == key) {
            e.size = size;
            e.last_access = self.clock;
        } else {
            self.entries.push(CacheEntry {
                key,
                size,
                last_access: self.clock,
            });
        }
        self.evict_to_fit()
    }

    /// Record a use of `key` (bumps its LRU recency). No-op if absent.
    pub fn touch(&mut self, key: &str) -> bool {
        self.clock += 1;
        if let Some(e) = self.entries.iter_mut().find(|e| e.key == key) {
            e.last_access = self.clock;
            true
        } else {
            false
        }
    }

    /// The keys in least-recently-used → most-recently-used order. Drives the
    /// eviction list shown in the UI.
    pub fn lru_order(&self) -> Vec<String> {
        let mut idx: Vec<&CacheEntry> = self.entries.iter().collect();
        idx.sort_by_key(|e| e.last_access);
        idx.into_iter().map(|e| e.key.clone()).collect()
    }

    /// Evict least-recently-used entries until `current_usage <= max_size_bytes`.
    /// Returns the keys removed (oldest first).
    pub fn evict_to_fit(&mut self) -> Vec<String> {
        let mut evicted = Vec::new();
        while self.current_usage() > self.max_size_bytes && !self.entries.is_empty() {
            // Find the LRU entry (smallest last_access).
            let (lru_idx, _) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_access)
                .unwrap();
            evicted.push(self.entries.remove(lru_idx).key);
        }
        evicted
    }

    /// Empty the whole cache, returning the freed byte count.
    pub fn purge(&mut self) -> u64 {
        let freed = self.current_usage();
        self.entries.clear();
        freed
    }

    /// Set a new size limit and immediately evict to fit it.
    pub fn set_max_bytes(&mut self, bytes: u64) -> Vec<String> {
        self.max_size_bytes = bytes;
        self.evict_to_fit()
    }
}

impl App {
    pub(super) fn apply_cache_manager(&mut self, action: Action) {
        match action {
            Action::SetCacheDir(dir) => {
                self.disk_cache.cache_dir = dir;
            }
            Action::SetCacheMaxGb(gb) => {
                let bytes = (gb.max(0.0) as f64 * 1024.0 * 1024.0 * 1024.0) as u64;
                self.disk_cache.set_max_bytes(bytes);
            }
            Action::CacheInsert { key, size } => {
                self.disk_cache.insert(key, size);
            }
            Action::CacheTouch(key) => {
                self.disk_cache.touch(&key);
            }
            Action::PurgeDiskCache => {
                self.disk_cache.purge();
            }
            Action::EvictDiskCache => {
                self.disk_cache.evict_to_fit();
            }
            _ => unreachable!("apply_cache_manager called with wrong action"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_sets_limit_from_gb() {
        let c = DiskCacheManager::new("/tmp/cache", 2.0);
        assert_eq!(c.max_size_bytes, 2 * 1024 * 1024 * 1024);
        assert_eq!(c.current_usage(), 0);
    }

    #[test]
    fn test_insert_and_usage() {
        let mut c = DiskCacheManager::new("/tmp", 1.0);
        c.insert("a", 100);
        c.insert("b", 200);
        assert_eq!(c.current_usage(), 300);
    }

    #[test]
    fn test_insert_same_key_updates_not_duplicates() {
        let mut c = DiskCacheManager::new("/tmp", 1.0);
        c.insert("a", 100);
        c.insert("a", 250);
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.current_usage(), 250);
    }

    #[test]
    fn test_lru_eviction_removes_oldest() {
        // Limit of 250 bytes; inserting three 100-byte entries must evict the LRU.
        let mut c = DiskCacheManager {
            max_size_bytes: 250,
            ..Default::default()
        };
        c.insert("a", 100);
        c.insert("b", 100);
        // Touch "a" so "b" becomes the LRU before the third insert.
        c.touch("a");
        let evicted = c.insert("c", 100);
        assert_eq!(evicted, vec!["b".to_string()]);
        assert_eq!(c.current_usage(), 200);
        assert!(c.entries.iter().any(|e| e.key == "a"));
        assert!(c.entries.iter().any(|e| e.key == "c"));
        assert!(!c.entries.iter().any(|e| e.key == "b"));
    }

    #[test]
    fn test_lru_order() {
        let mut c = DiskCacheManager::new("/tmp", 1.0);
        c.insert("a", 1);
        c.insert("b", 1);
        c.insert("c", 1);
        c.touch("a"); // a now most recent
        let order = c.lru_order();
        assert_eq!(order, vec!["b".to_string(), "c".to_string(), "a".to_string()]);
    }

    #[test]
    fn test_purge_empties_and_reports_freed() {
        let mut c = DiskCacheManager::new("/tmp", 1.0);
        c.insert("a", 500);
        c.insert("b", 700);
        let freed = c.purge();
        assert_eq!(freed, 1200);
        assert_eq!(c.current_usage(), 0);
        assert!(c.entries.is_empty());
    }

    #[test]
    fn test_set_max_bytes_evicts_immediately() {
        let mut c = DiskCacheManager::new("/tmp", 1.0);
        c.insert("a", 100);
        c.insert("b", 100);
        c.insert("c", 100);
        let evicted = c.set_max_bytes(150);
        // Must evict down to <= 150 bytes (so two of three 100B entries removed).
        assert_eq!(evicted.len(), 2);
        assert_eq!(c.current_usage(), 100);
    }

    #[test]
    fn test_usage_fraction() {
        let mut c = DiskCacheManager {
            max_size_bytes: 1000,
            ..Default::default()
        };
        c.insert("a", 250);
        assert!((c.usage_fraction() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_app_actions() {
        let mut app = App::new();
        app.apply(Action::SetCacheMaxGb(0.0)); // 0 bytes
        app.apply(Action::CacheInsert { key: "frame0".to_string(), size: 100 });
        // With a 0-byte limit the insert is immediately evicted.
        assert_eq!(app.disk_cache.current_usage(), 0);

        app.apply(Action::SetCacheDir(PathBuf::from("/tmp/pulse_cache")));
        assert_eq!(app.disk_cache.cache_dir, PathBuf::from("/tmp/pulse_cache"));

        app.apply(Action::SetCacheMaxGb(1.0));
        app.apply(Action::CacheInsert { key: "f1".to_string(), size: 10 });
        app.apply(Action::CacheInsert { key: "f2".to_string(), size: 20 });
        assert_eq!(app.disk_cache.current_usage(), 30);
        app.apply(Action::CacheTouch("f1".to_string()));
        app.apply(Action::PurgeDiskCache);
        assert_eq!(app.disk_cache.current_usage(), 0);
    }
}
