//! Keyboard-shortcut remapping: a command→keychord map with Photoshop-like
//! defaults, remap support, JSON load/save, and conflict detection.
//!
//! A `KeyChord` is a normalized text form (e.g. `"Cmd+Shift+S"`); commands are
//! identified by a stable string id. The model is pure data — the GPUI host
//! later resolves a chord to a command and emits the matching `Action`.

use super::{App, Action};
use std::collections::HashMap;

/// A normalized key chord: ordered modifier flags + a base key name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// Base key, upper-cased single char or named key ("F1", "Delete", "Space").
    pub key: String,
}

impl KeyChord {
    /// Parse a chord from a `+`-joined string like `"Cmd+Shift+S"`. Modifier
    /// tokens are case-insensitive; the final token is the base key. Returns
    /// `None` if there is no base key.
    pub fn parse(s: &str) -> Option<KeyChord> {
        let mut chord = KeyChord {
            cmd: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: String::new(),
        };
        for part in s.split('+') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            match p.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "meta" | "super" | "win" => chord.cmd = true,
                "ctrl" | "control" => chord.ctrl = true,
                "alt" | "opt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                _ => {
                    // Base key: single letters upper-case; named keys kept as-is.
                    chord.key = if p.chars().count() == 1 {
                        p.to_ascii_uppercase()
                    } else {
                        // Title-case-ish: keep first form (e.g. "F1", "Delete").
                        p.to_string()
                    };
                }
            }
        }
        if chord.key.is_empty() {
            None
        } else {
            Some(chord)
        }
    }

    /// Render to the canonical string form (modifiers in a fixed order).
    pub fn to_text(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if self.cmd {
            parts.push("Cmd");
        }
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.shift {
            parts.push("Shift");
        }
        let mut s = parts.join("+");
        if !s.is_empty() {
            s.push('+');
        }
        s.push_str(&self.key);
        s
    }
}

/// The command→chord binding table.
#[derive(Clone, Debug, Default)]
pub struct ShortcutMap {
    /// command id → chord.
    pub bindings: HashMap<String, KeyChord>,
}

impl ShortcutMap {
    /// Photoshop-like defaults for the core commands.
    pub fn defaults() -> Self {
        let mut bindings = HashMap::new();
        let mut set = |cmd: &str, chord: &str| {
            if let Some(c) = KeyChord::parse(chord) {
                bindings.insert(cmd.to_string(), c);
            }
        };
        set("file.new", "Cmd+N");
        set("file.open", "Cmd+O");
        set("file.save", "Cmd+S");
        set("file.save_as", "Cmd+Shift+S");
        set("file.export", "Cmd+Shift+E");
        set("edit.undo", "Cmd+Z");
        set("edit.redo", "Cmd+Shift+Z");
        set("edit.copy", "Cmd+C");
        set("edit.cut", "Cmd+X");
        set("edit.paste", "Cmd+V");
        set("edit.free_transform", "Cmd+T");
        set("select.all", "Cmd+A");
        set("select.deselect", "Cmd+D");
        set("select.inverse", "Cmd+Shift+I");
        set("layer.new", "Cmd+Shift+N");
        set("layer.group", "Cmd+G");
        set("layer.merge_down", "Cmd+E");
        set("view.zoom_in", "Cmd++");
        set("view.zoom_out", "Cmd+-");
        set("view.fit", "Cmd+0");
        set("tool.brush", "B");
        set("tool.eraser", "E");
        set("tool.move", "V");
        set("tool.marquee", "M");
        set("tool.lasso", "L");
        set("tool.text", "T");
        set("tool.crop", "C");
        Self { bindings }
    }

    /// Rebind a command to a new chord (overwrites any existing binding).
    pub fn remap(&mut self, command: &str, chord: KeyChord) {
        self.bindings.insert(command.to_string(), chord);
    }

    /// Remove a command's binding.
    pub fn unbind(&mut self, command: &str) {
        self.bindings.remove(command);
    }

    /// Find the command currently bound to `chord`, if any.
    pub fn command_for(&self, chord: &KeyChord) -> Option<&str> {
        self.bindings
            .iter()
            .find(|(_, c)| *c == chord)
            .map(|(k, _)| k.as_str())
    }

    /// Detect all conflicts: chords bound to more than one command. Returns a
    /// list of `(chord_text, [commands…])` for each duplicated chord, with the
    /// command lists sorted for determinism.
    pub fn conflicts(&self) -> Vec<(String, Vec<String>)> {
        let mut by_chord: HashMap<String, Vec<String>> = HashMap::new();
        for (cmd, chord) in &self.bindings {
            by_chord
                .entry(chord.to_text())
                .or_default()
                .push(cmd.clone());
        }
        let mut out: Vec<(String, Vec<String>)> = by_chord
            .into_iter()
            .filter(|(_, cmds)| cmds.len() > 1)
            .map(|(chord, mut cmds)| {
                cmds.sort();
                (chord, cmds)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Whether assigning `chord` to `command` would collide with a different
    /// command's existing binding.
    pub fn would_conflict(&self, command: &str, chord: &KeyChord) -> bool {
        self.bindings
            .iter()
            .any(|(cmd, c)| cmd != command && c == chord)
    }

    /// Serialize to a pretty JSON object: `{ "command": "Chord", … }`.
    pub fn to_json(&self) -> String {
        let map: std::collections::BTreeMap<String, String> = self
            .bindings
            .iter()
            .map(|(k, v)| (k.clone(), v.to_text()))
            .collect();
        serde_json::to_string_pretty(&map).unwrap_or_else(|_| "{}".to_string())
    }

    /// Parse from the JSON object form. Invalid chords are skipped.
    pub fn from_json(json: &str) -> Option<ShortcutMap> {
        let map: std::collections::BTreeMap<String, String> = serde_json::from_str(json).ok()?;
        let mut bindings = HashMap::new();
        for (cmd, chord_text) in map {
            if let Some(chord) = KeyChord::parse(&chord_text) {
                bindings.insert(cmd, chord);
            }
        }
        Some(ShortcutMap { bindings })
    }
}

impl App {
    /// Dispatch for the keyboard-shortcuts domain actions.
    pub(super) fn apply_shortcuts(&mut self, action: Action) {
        match action {
            Action::RemapShortcut { command, chord } => {
                if let Some(c) = KeyChord::parse(&chord) {
                    if self.shortcut_map.would_conflict(&command, &c) {
                        self.status_message =
                            Some(format!("Shortcut conflict: {chord} already in use"));
                    } else {
                        self.shortcut_map.remap(&command, c);
                        self.status_message = Some(format!("Bound {command} → {chord}"));
                    }
                }
            }
            Action::UnbindShortcut(command) => {
                self.shortcut_map.unbind(&command);
            }
            Action::ResetShortcuts => {
                self.shortcut_map = ShortcutMap::defaults();
            }
            Action::LoadShortcutsJson(json) => {
                if let Some(map) = ShortcutMap::from_json(&json) {
                    self.shortcut_map = map;
                    self.status_message = Some("Shortcuts loaded".to_string());
                } else {
                    self.status_message = Some("Shortcut load failed".to_string());
                }
            }
            Action::SaveShortcutsJson(path) => {
                let json = self.shortcut_map.to_json();
                match std::fs::write(&path, json) {
                    Ok(()) => {
                        self.status_message =
                            Some(format!("Shortcuts saved: {}", path.display()))
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Shortcut save failed: {e}"))
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_roundtrip_chord() {
        let c = KeyChord::parse("Cmd+Shift+S").unwrap();
        assert!(c.cmd && c.shift && !c.alt && !c.ctrl);
        assert_eq!(c.key, "S");
        assert_eq!(c.to_text(), "Cmd+Shift+S");
    }

    #[test]
    fn parse_is_case_insensitive_on_mods() {
        let a = KeyChord::parse("cmd+z").unwrap();
        let b = KeyChord::parse("CMD+Z").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn parse_named_key() {
        let c = KeyChord::parse("Alt+Delete").unwrap();
        assert!(c.alt);
        assert_eq!(c.key, "Delete");
        // No base key → None.
        assert!(KeyChord::parse("Cmd+Shift").is_none());
    }

    #[test]
    fn defaults_present() {
        let m = ShortcutMap::defaults();
        assert_eq!(m.bindings["file.save"].to_text(), "Cmd+S");
        assert_eq!(m.bindings["edit.undo"].to_text(), "Cmd+Z");
        assert_eq!(m.bindings["tool.brush"].to_text(), "B");
    }

    #[test]
    fn remap_and_lookup() {
        let mut m = ShortcutMap::defaults();
        let chord = KeyChord::parse("Cmd+J").unwrap();
        m.remap("layer.duplicate", chord.clone());
        assert_eq!(m.command_for(&chord), Some("layer.duplicate"));
    }

    #[test]
    fn conflict_detected() {
        let mut m = ShortcutMap::defaults();
        // Bind two commands to the same chord.
        let chord = KeyChord::parse("Cmd+K").unwrap();
        m.remap("custom.a", chord.clone());
        m.remap("custom.b", chord.clone());
        let conflicts = m.conflicts();
        let hit = conflicts
            .iter()
            .find(|(c, _)| c == "Cmd+K")
            .expect("Cmd+K conflict");
        assert_eq!(hit.1, vec!["custom.a".to_string(), "custom.b".to_string()]);
        // would_conflict catches it pre-emptively.
        assert!(m.would_conflict("custom.c", &chord));
        assert!(!m.would_conflict("custom.a", &chord)); // same command = no conflict
    }

    #[test]
    fn no_conflicts_in_defaults() {
        // The default set is collision-free.
        let m = ShortcutMap::defaults();
        assert!(m.conflicts().is_empty(), "defaults must not conflict");
    }

    #[test]
    fn json_roundtrip() {
        let m = ShortcutMap::defaults();
        let json = m.to_json();
        let back = ShortcutMap::from_json(&json).unwrap();
        assert_eq!(back.bindings["file.save"], m.bindings["file.save"]);
        assert_eq!(back.bindings.len(), m.bindings.len());
    }

    #[test]
    fn from_json_skips_invalid() {
        let json = r#"{ "a.cmd": "Cmd+G", "b.cmd": "Cmd+Shift" }"#;
        let m = ShortcutMap::from_json(json).unwrap();
        assert!(m.bindings.contains_key("a.cmd"));
        // "Cmd+Shift" has no base key → skipped.
        assert!(!m.bindings.contains_key("b.cmd"));
    }

    #[test]
    fn apply_remap_rejects_conflict() {
        let mut app = App::new();
        // file.save is Cmd+S by default; remapping a new command to Cmd+S should
        // be rejected with a conflict message.
        app.apply(Action::RemapShortcut {
            command: "custom.thing".to_string(),
            chord: "Cmd+S".to_string(),
        });
        assert!(!app.shortcut_map.bindings.contains_key("custom.thing"));
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or("")
            .contains("conflict"));
    }

    #[test]
    fn apply_remap_succeeds_when_free() {
        let mut app = App::new();
        app.apply(Action::RemapShortcut {
            command: "custom.free".to_string(),
            chord: "Cmd+Alt+9".to_string(),
        });
        assert_eq!(
            app.shortcut_map.bindings["custom.free"].to_text(),
            "Cmd+Alt+9"
        );
    }
}
