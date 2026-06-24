//! **Preferences + remappable keybindings.**
//!
//! Two related models:
//!
//! 1. [`Preferences`] — the editor's global settings (autosave, scratch disks,
//!    playback resolution, default transition duration), with clamping so values
//!    stay sane.
//! 2. [`Keymap`] — a command → key-chord map seeded with Premiere-like defaults.
//!    Chords are remappable; [`Keymap::conflicts`] detects two commands bound to
//!    the same chord, and the map serialises to / from a small JSON document
//!    (hand-rolled so this module needs no extra crate dependency).
//!
//! All pure + deterministic, hence unit-testable end to end.

use std::path::PathBuf;

use super::{App, Action};

// ============================================================================
// Preferences
// ============================================================================

/// Playback preview resolution (fraction of full).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum PlaybackResolution {
    Full,
    #[default]
    Half,
    Quarter,
}

impl PlaybackResolution {
    pub fn scale(self) -> f32 {
        match self {
            PlaybackResolution::Full => 1.0,
            PlaybackResolution::Half => 0.5,
            PlaybackResolution::Quarter => 0.25,
        }
    }
}

/// Editor-wide preferences.
#[derive(Clone, Debug)]
pub struct Preferences {
    pub autosave_enabled: bool,
    pub autosave_interval_secs: u32,
    pub autosave_max_versions: u32,
    /// Scratch-disk roots for captured/rendered/proxy media.
    pub scratch_capture: PathBuf,
    pub scratch_rendered: PathBuf,
    pub scratch_proxies: PathBuf,
    pub playback_resolution: PlaybackResolution,
    pub paused_resolution: PlaybackResolution,
    /// Default transition duration (seconds) for newly-added transitions.
    pub default_transition_secs: f32,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            autosave_enabled: true,
            autosave_interval_secs: 300,
            autosave_max_versions: 20,
            scratch_capture: PathBuf::from("Scratch/Captured"),
            scratch_rendered: PathBuf::from("Scratch/Rendered"),
            scratch_proxies: PathBuf::from("Scratch/Proxies"),
            playback_resolution: PlaybackResolution::Half,
            paused_resolution: PlaybackResolution::Full,
            default_transition_secs: 1.0,
        }
    }
}

impl Preferences {
    /// Clamp interval to `[10, 3600]` seconds.
    pub fn set_autosave_interval(&mut self, secs: u32) {
        self.autosave_interval_secs = secs.clamp(10, 3600);
    }

    /// Clamp version retention to `[1, 100]`.
    pub fn set_max_versions(&mut self, n: u32) {
        self.autosave_max_versions = n.clamp(1, 100);
    }

    /// Clamp default transition duration to `[0.1, 30.0]` seconds.
    pub fn set_default_transition(&mut self, secs: f32) {
        self.default_transition_secs = secs.clamp(0.1, 30.0);
    }
}

// ============================================================================
// Keybindings
// ============================================================================

/// A remappable editor command (the set Premiere assigns default chords to).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditorCommand {
    PlayPause,
    Stop,
    RazorAtPlayhead,
    RippleDelete,
    LiftDelete,
    InsertEdit,
    OverwriteEdit,
    MarkIn,
    MarkOut,
    NudgeLeft,
    NudgeRight,
    ZoomIn,
    ZoomOut,
    Undo,
    Redo,
    Save,
    Export,
}

impl EditorCommand {
    /// A stable string id used as the JSON key.
    pub fn id(self) -> &'static str {
        match self {
            EditorCommand::PlayPause => "play_pause",
            EditorCommand::Stop => "stop",
            EditorCommand::RazorAtPlayhead => "razor_at_playhead",
            EditorCommand::RippleDelete => "ripple_delete",
            EditorCommand::LiftDelete => "lift_delete",
            EditorCommand::InsertEdit => "insert_edit",
            EditorCommand::OverwriteEdit => "overwrite_edit",
            EditorCommand::MarkIn => "mark_in",
            EditorCommand::MarkOut => "mark_out",
            EditorCommand::NudgeLeft => "nudge_left",
            EditorCommand::NudgeRight => "nudge_right",
            EditorCommand::ZoomIn => "zoom_in",
            EditorCommand::ZoomOut => "zoom_out",
            EditorCommand::Undo => "undo",
            EditorCommand::Redo => "redo",
            EditorCommand::Save => "save",
            EditorCommand::Export => "export",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    pub const ALL: [EditorCommand; 17] = [
        EditorCommand::PlayPause,
        EditorCommand::Stop,
        EditorCommand::RazorAtPlayhead,
        EditorCommand::RippleDelete,
        EditorCommand::LiftDelete,
        EditorCommand::InsertEdit,
        EditorCommand::OverwriteEdit,
        EditorCommand::MarkIn,
        EditorCommand::MarkOut,
        EditorCommand::NudgeLeft,
        EditorCommand::NudgeRight,
        EditorCommand::ZoomIn,
        EditorCommand::ZoomOut,
        EditorCommand::Undo,
        EditorCommand::Redo,
        EditorCommand::Save,
        EditorCommand::Export,
    ];
}

/// A normalised key chord: modifiers + a key name. Comparison is order- and
/// case-insensitive on the key name; modifiers are explicit flags.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Key name, normalised to lowercase, e.g. `"k"`, `"space"`, `"left"`.
    pub key: String,
}

impl KeyChord {
    pub fn new(key: impl Into<String>) -> Self {
        Self { ctrl: false, shift: false, alt: false, key: key.into().to_lowercase() }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Canonical string form, e.g. `"Ctrl+Shift+K"` or `"Space"`.
    pub fn to_canonical(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        // Capitalise the key name for display.
        let mut chars = self.key.chars();
        if let Some(first) = chars.next() {
            s.extend(first.to_uppercase());
            s.push_str(chars.as_str());
        }
        s
    }

    /// Parse a canonical chord string (`"Ctrl+Shift+K"`) back into a [`KeyChord`].
    pub fn parse(s: &str) -> Option<KeyChord> {
        let mut ctrl = false;
        let mut shift = false;
        let mut alt = false;
        let mut key: Option<String> = None;
        for part in s.split('+') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            match p.to_lowercase().as_str() {
                "ctrl" | "cmd" | "control" => ctrl = true,
                "shift" => shift = true,
                "alt" | "opt" | "option" => alt = true,
                other => key = Some(other.to_string()),
            }
        }
        key.map(|k| KeyChord { ctrl, shift, alt, key: k })
    }
}

/// The command → chord map.
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    /// `(command, chord)` pairs. One command may appear once.
    pub bindings: Vec<(EditorCommand, KeyChord)>,
}

impl Keymap {
    /// The Premiere-like default keymap.
    pub fn premiere_defaults() -> Self {
        let mut m = Keymap::default();
        m.set(EditorCommand::PlayPause, KeyChord::new("space"));
        m.set(EditorCommand::Stop, KeyChord::new("k"));
        m.set(EditorCommand::RazorAtPlayhead, KeyChord::new("c"));
        m.set(EditorCommand::RippleDelete, KeyChord::new("delete").shift());
        m.set(EditorCommand::LiftDelete, KeyChord::new("delete"));
        m.set(EditorCommand::InsertEdit, KeyChord::new(","));
        m.set(EditorCommand::OverwriteEdit, KeyChord::new("."));
        m.set(EditorCommand::MarkIn, KeyChord::new("i"));
        m.set(EditorCommand::MarkOut, KeyChord::new("o"));
        m.set(EditorCommand::NudgeLeft, KeyChord::new("left").alt());
        m.set(EditorCommand::NudgeRight, KeyChord::new("right").alt());
        m.set(EditorCommand::ZoomIn, KeyChord::new("=").ctrl());
        m.set(EditorCommand::ZoomOut, KeyChord::new("-").ctrl());
        m.set(EditorCommand::Undo, KeyChord::new("z").ctrl());
        m.set(EditorCommand::Redo, KeyChord::new("z").ctrl().shift());
        m.set(EditorCommand::Save, KeyChord::new("s").ctrl());
        m.set(EditorCommand::Export, KeyChord::new("m").ctrl());
        m
    }

    /// Bind `command` to `chord`, replacing any existing binding for it.
    pub fn set(&mut self, command: EditorCommand, chord: KeyChord) {
        if let Some(b) = self.bindings.iter_mut().find(|(c, _)| *c == command) {
            b.1 = chord;
        } else {
            self.bindings.push((command, chord));
        }
    }

    /// The chord currently bound to `command`, if any.
    pub fn chord_for(&self, command: EditorCommand) -> Option<&KeyChord> {
        self.bindings.iter().find(|(c, _)| *c == command).map(|(_, k)| k)
    }

    /// The command currently triggered by `chord`, if any.
    pub fn command_for(&self, chord: &KeyChord) -> Option<EditorCommand> {
        self.bindings.iter().find(|(_, k)| k == chord).map(|(c, _)| *c)
    }

    /// Remap `command` to `chord`. Returns the set of *other* commands that now
    /// conflict (share `chord`) — empty when the remap is conflict-free.
    pub fn remap(&mut self, command: EditorCommand, chord: KeyChord) -> Vec<EditorCommand> {
        self.set(command, chord.clone());
        self.bindings
            .iter()
            .filter(|(c, k)| *c != command && *k == chord)
            .map(|(c, _)| *c)
            .collect()
    }

    /// Every pair of commands bound to the same chord, as `(a, b, chord)`.
    pub fn conflicts(&self) -> Vec<(EditorCommand, EditorCommand, KeyChord)> {
        let mut out = Vec::new();
        for i in 0..self.bindings.len() {
            for j in (i + 1)..self.bindings.len() {
                if self.bindings[i].1 == self.bindings[j].1 {
                    out.push((self.bindings[i].0, self.bindings[j].0, self.bindings[i].1.clone()));
                }
            }
        }
        out
    }

    /// Serialise to a small JSON object `{ "command_id": "Chord", ... }`.
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\n");
        for (i, (cmd, chord)) in self.bindings.iter().enumerate() {
            s.push_str("  \"");
            s.push_str(cmd.id());
            s.push_str("\": \"");
            s.push_str(&chord.to_canonical());
            s.push('"');
            if i + 1 < self.bindings.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push('}');
        s
    }

    /// Parse the JSON produced by [`Keymap::to_json`]. Unknown command ids and
    /// malformed chords are skipped. Always succeeds (returns whatever parsed).
    pub fn from_json(json: &str) -> Keymap {
        let mut m = Keymap::default();
        // Strip the outer braces and split on commas at the top level (the values
        // never contain commas after the chord canonicalisation, but be safe by
        // only splitting between `"` … `"` pairs).
        let body = json.trim().trim_start_matches('{').trim_end_matches('}');
        for entry in split_json_entries(body) {
            let Some((k, v)) = parse_json_pair(&entry) else { continue };
            let Some(cmd) = EditorCommand::from_id(&k) else { continue };
            let Some(chord) = KeyChord::parse(&v) else { continue };
            m.set(cmd, chord);
        }
        m
    }
}

/// Split a JSON object body into `"key": "value"` entries, respecting quoted
/// strings so a comma inside a value never splits an entry.
fn split_json_entries(body: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in body.chars() {
        match ch {
            '"' => {
                in_str = !in_str;
                cur.push(ch);
            }
            ',' if !in_str => {
                entries.push(std::mem::take(&mut cur));
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        entries.push(cur);
    }
    entries
}

/// Parse a single `"key": "value"` entry into `(key, value)` unquoted strings.
fn parse_json_pair(entry: &str) -> Option<(String, String)> {
    let (k, v) = entry.split_once(':')?;
    let key = k.trim().trim_matches('"').to_string();
    let val = v.trim().trim_matches('"').to_string();
    if key.is_empty() {
        return None;
    }
    Some((key, val))
}

impl App {
    /// Apply the preferences / keybinding actions. Routed from `mod.rs`.
    pub(crate) fn apply_prefs_keys(&mut self, action: Action) {
        match action {
            Action::SetPrefAutosaveEnabled(b) => {
                self.preferences.autosave_enabled = b;
            }
            Action::SetPrefAutosaveInterval(s) => {
                self.preferences.set_autosave_interval(s);
            }
            Action::SetPrefMaxVersions(n) => {
                self.preferences.set_max_versions(n);
            }
            Action::SetPrefScratchDisk { kind, path } => match kind {
                ScratchKind::Capture => self.preferences.scratch_capture = path,
                ScratchKind::Rendered => self.preferences.scratch_rendered = path,
                ScratchKind::Proxies => self.preferences.scratch_proxies = path,
            },
            Action::SetPrefPlaybackResolution(r) => {
                self.preferences.playback_resolution = r;
            }
            Action::SetPrefPausedResolution(r) => {
                self.preferences.paused_resolution = r;
            }
            Action::SetPrefDefaultTransition(secs) => {
                self.preferences.set_default_transition(secs);
            }
            Action::RemapKeybinding { command, chord } => {
                self.last_keybind_conflicts = self.keymap.remap(command, chord);
            }
            Action::ResetKeymap => {
                self.keymap = Keymap::premiere_defaults();
                self.last_keybind_conflicts.clear();
            }
            Action::LoadKeymapJson(json) => {
                self.keymap = Keymap::from_json(&json);
            }
            _ => {}
        }
    }
}

/// Which scratch-disk root an action targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScratchKind {
    Capture,
    Rendered,
    Proxies,
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn prefs_clamp_ranges() {
        let mut p = Preferences::default();
        p.set_autosave_interval(2);
        assert_eq!(p.autosave_interval_secs, 10);
        p.set_autosave_interval(99999);
        assert_eq!(p.autosave_interval_secs, 3600);
        p.set_max_versions(0);
        assert_eq!(p.autosave_max_versions, 1);
        p.set_default_transition(0.0);
        assert!((p.default_transition_secs - 0.1).abs() < 1e-5);
        p.set_default_transition(100.0);
        assert!((p.default_transition_secs - 30.0).abs() < 1e-5);
    }

    #[test]
    fn defaults_cover_all_commands_without_conflict() {
        let km = Keymap::premiere_defaults();
        assert_eq!(km.bindings.len(), EditorCommand::ALL.len());
        // Every command is bound.
        for cmd in EditorCommand::ALL {
            assert!(km.chord_for(cmd).is_some(), "{:?} unbound", cmd);
        }
        // Defaults are conflict-free.
        assert!(km.conflicts().is_empty(), "{:?}", km.conflicts());
    }

    #[test]
    fn chord_canonical_roundtrip() {
        let c = KeyChord::new("z").ctrl().shift();
        assert_eq!(c.to_canonical(), "Ctrl+Shift+Z");
        let parsed = KeyChord::parse("Ctrl+Shift+Z").unwrap();
        assert_eq!(parsed, c);
        // Order-insensitive parse.
        assert_eq!(KeyChord::parse("Shift+Ctrl+Z").unwrap(), c);
        // Plain key.
        assert_eq!(KeyChord::parse("Space").unwrap(), KeyChord::new("space"));
    }

    #[test]
    fn remap_detects_conflict() {
        let mut km = Keymap::premiere_defaults();
        // Bind ZoomIn to the same chord as PlayPause (Space) → conflict reported.
        let conflicts = km.remap(EditorCommand::ZoomIn, KeyChord::new("space"));
        assert!(conflicts.contains(&EditorCommand::PlayPause));
        // The global conflict scan also finds it.
        assert!(km.conflicts().iter().any(|(a, b, _)| {
            *a == EditorCommand::PlayPause && *b == EditorCommand::ZoomIn
                || *a == EditorCommand::ZoomIn && *b == EditorCommand::PlayPause
        }));
        // Remapping to a free chord is conflict-free.
        let none = km.remap(EditorCommand::ZoomIn, KeyChord::new("f9"));
        assert!(none.is_empty());
    }

    #[test]
    fn json_roundtrip() {
        let km = Keymap::premiere_defaults();
        let json = km.to_json();
        let back = Keymap::from_json(&json);
        // Every binding survives the round-trip.
        for cmd in EditorCommand::ALL {
            assert_eq!(
                back.chord_for(cmd),
                km.chord_for(cmd),
                "binding for {:?} differs",
                cmd
            );
        }
    }

    #[test]
    fn from_json_skips_unknown_and_malformed() {
        let json = r#"{
  "play_pause": "Space",
  "not_a_command": "X",
  "save": ""
}"#;
        let km = Keymap::from_json(json);
        assert_eq!(km.chord_for(EditorCommand::PlayPause), Some(&KeyChord::new("space")));
        // Unknown command id dropped; empty chord (no key) dropped.
        assert!(km.command_for(&KeyChord::new("space")).is_some());
        assert!(km.chord_for(EditorCommand::Save).is_none());
    }

    #[test]
    fn action_remap_records_conflicts() {
        let mut app = App::new();
        // Remap Save onto Space → conflicts with PlayPause.
        app.apply(Action::RemapKeybinding {
            command: EditorCommand::Save,
            chord: KeyChord::new("space"),
        });
        assert!(app.last_keybind_conflicts.contains(&EditorCommand::PlayPause));
        assert_eq!(app.keymap.chord_for(EditorCommand::Save), Some(&KeyChord::new("space")));
        // Reset restores defaults and clears conflicts.
        app.apply(Action::ResetKeymap);
        assert!(app.last_keybind_conflicts.is_empty());
        assert_eq!(app.keymap.chord_for(EditorCommand::Save), Some(&KeyChord::new("s").ctrl()));
    }

    #[test]
    fn action_set_prefs_and_load_keymap() {
        let mut app = App::new();
        app.apply(Action::SetPrefDefaultTransition(2.5));
        assert!((app.preferences.default_transition_secs - 2.5).abs() < 1e-5);
        app.apply(Action::SetPrefPlaybackResolution(PlaybackResolution::Quarter));
        assert_eq!(app.preferences.playback_resolution, PlaybackResolution::Quarter);
        app.apply(Action::SetPrefScratchDisk {
            kind: ScratchKind::Proxies,
            path: PathBuf::from("/fast/ssd/proxies"),
        });
        assert_eq!(app.preferences.scratch_proxies, PathBuf::from("/fast/ssd/proxies"));
        // Load a minimal keymap JSON.
        app.apply(Action::LoadKeymapJson("{ \"undo\": \"Ctrl+Z\" }".to_string()));
        assert_eq!(app.keymap.chord_for(EditorCommand::Undo), Some(&KeyChord::new("z").ctrl()));
    }
}
