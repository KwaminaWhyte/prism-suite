//! **Workspaces (panel layouts).**
//!
//! A named panel-layout model like Premiere's workspace switcher (Editing,
//! Color, Audio, Effects, Graphics, …). A [`Workspace`] is a name plus a
//! [`PanelLayout`] — a set of dockable [`Panel`]s positioned in dock [`Region`]s
//! with visibility + size. The [`WorkspaceManager`] owns the list, the active
//! index, and a *pristine* copy of each built-in workspace so it can **reset** a
//! customised layout back to its factory default. Switching makes a workspace
//! active; saving captures the current (possibly edited) layout back into the
//! active workspace.
//!
//! Pure data + deterministic logic (no GPUI windows here), so it's fully
//! unit-testable.

use super::{App, Action};

/// A dock region a panel can live in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Region {
    Left,
    #[default]
    Center,
    Right,
    Bottom,
    Floating,
}

/// A panel that can appear in a workspace layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Panel {
    Project,
    SourceMonitor,
    ProgramMonitor,
    Timeline,
    EffectControls,
    EffectsBrowser,
    LumetriColor,
    LumetriScopes,
    AudioMixer,
    AudioClipMixer,
    EssentialGraphics,
    EssentialSound,
    MediaBrowser,
}

impl Panel {
    pub fn id(self) -> &'static str {
        match self {
            Panel::Project => "project",
            Panel::SourceMonitor => "source_monitor",
            Panel::ProgramMonitor => "program_monitor",
            Panel::Timeline => "timeline",
            Panel::EffectControls => "effect_controls",
            Panel::EffectsBrowser => "effects_browser",
            Panel::LumetriColor => "lumetri_color",
            Panel::LumetriScopes => "lumetri_scopes",
            Panel::AudioMixer => "audio_mixer",
            Panel::AudioClipMixer => "audio_clip_mixer",
            Panel::EssentialGraphics => "essential_graphics",
            Panel::EssentialSound => "essential_sound",
            Panel::MediaBrowser => "media_browser",
        }
    }
}

/// One panel's placement in a layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelSlot {
    pub panel: Panel,
    pub region: Region,
    pub visible: bool,
    /// Fractional size (0..=1) of the panel within its region.
    pub size: f32,
}

impl PanelSlot {
    pub fn new(panel: Panel, region: Region) -> Self {
        Self { panel, region, visible: true, size: 0.25 }
    }
}

/// A complete panel layout: the placement of every panel it includes.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PanelLayout {
    pub slots: Vec<PanelSlot>,
}

impl PanelLayout {
    pub fn slot(&self, panel: Panel) -> Option<&PanelSlot> {
        self.slots.iter().find(|s| s.panel == panel)
    }

    pub fn slot_mut(&mut self, panel: Panel) -> Option<&mut PanelSlot> {
        self.slots.iter_mut().find(|s| s.panel == panel)
    }

    /// Add or replace a panel slot.
    pub fn place(&mut self, slot: PanelSlot) {
        if let Some(s) = self.slot_mut(slot.panel) {
            *s = slot;
        } else {
            self.slots.push(slot);
        }
    }

    /// Panels currently visible, in region-then-insertion order.
    pub fn visible_panels(&self) -> Vec<Panel> {
        self.slots.iter().filter(|s| s.visible).map(|s| s.panel).collect()
    }
}

/// A named workspace with its layout.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    pub name: String,
    pub layout: PanelLayout,
    /// `true` for the built-in workspaces (which have a factory default to reset
    /// to); user-created ones are `false`.
    pub builtin: bool,
}

impl Workspace {
    fn from_slots(name: &str, slots: Vec<PanelSlot>) -> Self {
        Self { name: name.to_string(), layout: PanelLayout { slots }, builtin: true }
    }

    pub fn editing() -> Self {
        Self::from_slots(
            "Editing",
            vec![
                PanelSlot::new(Panel::Project, Region::Left),
                PanelSlot::new(Panel::SourceMonitor, Region::Center),
                PanelSlot::new(Panel::ProgramMonitor, Region::Center),
                PanelSlot::new(Panel::Timeline, Region::Bottom),
                PanelSlot::new(Panel::EffectControls, Region::Left),
            ],
        )
    }

    pub fn color() -> Self {
        Self::from_slots(
            "Color",
            vec![
                PanelSlot::new(Panel::ProgramMonitor, Region::Center),
                PanelSlot::new(Panel::LumetriColor, Region::Right),
                PanelSlot::new(Panel::LumetriScopes, Region::Left),
                PanelSlot::new(Panel::Timeline, Region::Bottom),
            ],
        )
    }

    pub fn audio() -> Self {
        Self::from_slots(
            "Audio",
            vec![
                PanelSlot::new(Panel::ProgramMonitor, Region::Center),
                PanelSlot::new(Panel::AudioMixer, Region::Right),
                PanelSlot::new(Panel::EssentialSound, Region::Left),
                PanelSlot::new(Panel::Timeline, Region::Bottom),
            ],
        )
    }

    pub fn effects() -> Self {
        Self::from_slots(
            "Effects",
            vec![
                PanelSlot::new(Panel::ProgramMonitor, Region::Center),
                PanelSlot::new(Panel::EffectsBrowser, Region::Right),
                PanelSlot::new(Panel::EffectControls, Region::Left),
                PanelSlot::new(Panel::Timeline, Region::Bottom),
            ],
        )
    }

    pub fn graphics() -> Self {
        Self::from_slots(
            "Graphics",
            vec![
                PanelSlot::new(Panel::ProgramMonitor, Region::Center),
                PanelSlot::new(Panel::EssentialGraphics, Region::Right),
                PanelSlot::new(Panel::Project, Region::Left),
                PanelSlot::new(Panel::Timeline, Region::Bottom),
            ],
        )
    }
}

/// Owns the workspace list, the active index, and the factory defaults.
#[derive(Clone, Debug)]
pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub active: usize,
    /// Pristine factory copies of the built-in workspaces, keyed by name, used by
    /// [`WorkspaceManager::reset_active`].
    defaults: Vec<Workspace>,
}

impl Default for WorkspaceManager {
    fn default() -> Self {
        let builtins = vec![
            Workspace::editing(),
            Workspace::color(),
            Workspace::audio(),
            Workspace::effects(),
            Workspace::graphics(),
        ];
        Self { workspaces: builtins.clone(), active: 0, defaults: builtins }
    }
}

impl WorkspaceManager {
    /// The active workspace.
    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    /// The active workspace's layout (what the UI renders).
    pub fn active_layout(&self) -> &PanelLayout {
        &self.workspaces[self.active].layout
    }

    /// Switch to the workspace at `idx` (clamped; no-op if out of range).
    pub fn switch_to(&mut self, idx: usize) -> bool {
        if idx < self.workspaces.len() {
            self.active = idx;
            true
        } else {
            false
        }
    }

    /// Switch to the first workspace named `name` (case-insensitive). Returns the
    /// index if found.
    pub fn switch_by_name(&mut self, name: &str) -> Option<usize> {
        let idx = self
            .workspaces
            .iter()
            .position(|w| w.name.eq_ignore_ascii_case(name))?;
        self.active = idx;
        Some(idx)
    }

    /// Save `layout` into the active workspace (capturing the user's edits).
    pub fn save_active(&mut self, layout: PanelLayout) {
        self.workspaces[self.active].layout = layout;
    }

    /// Reset the active workspace to its factory layout. Returns `true` when a
    /// factory default existed (built-in workspaces only).
    pub fn reset_active(&mut self) -> bool {
        let name = self.workspaces[self.active].name.clone();
        if let Some(def) = self.defaults.iter().find(|d| d.name == name) {
            self.workspaces[self.active].layout = def.layout.clone();
            true
        } else {
            false
        }
    }

    /// Add a new user workspace cloning the active layout. Returns its index.
    pub fn add_user_workspace(&mut self, name: impl Into<String>) -> usize {
        let layout = self.active_layout().clone();
        self.workspaces.push(Workspace { name: name.into(), layout, builtin: false });
        self.workspaces.len() - 1
    }

    /// Remove the workspace at `idx` (built-ins can't be removed). Keeps `active`
    /// in range. Returns `true` when removed.
    pub fn remove(&mut self, idx: usize) -> bool {
        if idx >= self.workspaces.len() || self.workspaces[idx].builtin {
            return false;
        }
        self.workspaces.remove(idx);
        if self.active >= self.workspaces.len() {
            self.active = self.workspaces.len().saturating_sub(1);
        }
        true
    }
}

impl App {
    /// Apply the workspace actions. Routed from `mod.rs`.
    pub(crate) fn apply_workspaces(&mut self, action: Action) {
        match action {
            Action::SwitchWorkspace(idx) => {
                self.workspaces.switch_to(idx);
                self.host.mark_dirty();
            }
            Action::SwitchWorkspaceByName(name) => {
                self.workspaces.switch_by_name(&name);
                self.host.mark_dirty();
            }
            Action::SaveWorkspaceLayout(layout) => {
                self.workspaces.save_active(layout);
            }
            Action::ResetWorkspace => {
                self.workspaces.reset_active();
                self.host.mark_dirty();
            }
            Action::AddWorkspace(name) => {
                self.workspaces.add_user_workspace(name);
            }
            Action::RemoveWorkspace(idx) => {
                self.workspaces.remove(idx);
            }
            Action::TogglePanelVisible(panel) => {
                let active = self.workspaces.active;
                if let Some(slot) = self.workspaces.workspaces[active].layout.slot_mut(panel) {
                    slot.visible = !slot.visible;
                    self.host.mark_dirty();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::{App, Action};

    #[test]
    fn manager_seeds_five_builtins() {
        let m = WorkspaceManager::default();
        assert_eq!(m.workspaces.len(), 5);
        let names: Vec<&str> = m.workspaces.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["Editing", "Color", "Audio", "Effects", "Graphics"]);
        assert_eq!(m.active().name, "Editing");
        assert!(m.workspaces.iter().all(|w| w.builtin));
    }

    #[test]
    fn switch_updates_active_layout() {
        let mut m = WorkspaceManager::default();
        assert!(m.active_layout().slot(Panel::EffectControls).is_some());
        // Switch to Color → its layout has the Lumetri panels.
        assert!(m.switch_to(1));
        assert_eq!(m.active().name, "Color");
        assert!(m.active_layout().slot(Panel::LumetriColor).is_some());
        assert!(m.active_layout().slot(Panel::LumetriScopes).is_some());
        // Out-of-range switch is a no-op false.
        assert!(!m.switch_to(99));
        assert_eq!(m.active().name, "Color");
    }

    #[test]
    fn switch_by_name_case_insensitive() {
        let mut m = WorkspaceManager::default();
        assert_eq!(m.switch_by_name("graphics"), Some(4));
        assert_eq!(m.active().name, "Graphics");
        assert!(m.switch_by_name("nope").is_none());
    }

    #[test]
    fn save_then_reset_restores_factory() {
        let mut m = WorkspaceManager::default();
        // Edit the Editing layout: hide the Timeline + add a floating panel.
        let mut layout = m.active_layout().clone();
        layout.slot_mut(Panel::Timeline).unwrap().visible = false;
        layout.place(PanelSlot::new(Panel::MediaBrowser, Region::Floating));
        m.save_active(layout);
        assert!(!m.active_layout().slot(Panel::Timeline).unwrap().visible);
        assert!(m.active_layout().slot(Panel::MediaBrowser).is_some());
        // Reset → back to the factory Editing layout.
        assert!(m.reset_active());
        assert!(m.active_layout().slot(Panel::Timeline).unwrap().visible);
        assert!(m.active_layout().slot(Panel::MediaBrowser).is_none());
    }

    #[test]
    fn user_workspace_add_remove() {
        let mut m = WorkspaceManager::default();
        let idx = m.add_user_workspace("My Layout");
        assert_eq!(idx, 5);
        assert!(!m.workspaces[idx].builtin);
        // A user workspace has no factory default → reset returns false.
        m.switch_to(idx);
        assert!(!m.reset_active());
        // Built-ins can't be removed.
        assert!(!m.remove(0));
        // User workspace can be removed; active stays in range.
        assert!(m.remove(idx));
        assert_eq!(m.workspaces.len(), 5);
        assert!(m.active < m.workspaces.len());
    }

    #[test]
    fn action_switch_and_reset() {
        let mut app = App::new();
        assert_eq!(app.workspaces.active().name, "Editing");
        app.apply(Action::SwitchWorkspace(2));
        assert_eq!(app.workspaces.active().name, "Audio");
        assert!(app.workspaces.active_layout().slot(Panel::AudioMixer).is_some());
        // Switch by name.
        app.apply(Action::SwitchWorkspaceByName("Effects".to_string()));
        assert_eq!(app.workspaces.active().name, "Effects");
        // Edit + save + reset via actions.
        let mut layout = app.workspaces.active_layout().clone();
        layout.slot_mut(Panel::Timeline).unwrap().visible = false;
        app.apply(Action::SaveWorkspaceLayout(layout));
        assert!(!app.workspaces.active_layout().slot(Panel::Timeline).unwrap().visible);
        app.apply(Action::ResetWorkspace);
        assert!(app.workspaces.active_layout().slot(Panel::Timeline).unwrap().visible);
    }

    #[test]
    fn action_toggle_panel_visibility() {
        let mut app = App::new();
        assert!(app.workspaces.active_layout().slot(Panel::Project).unwrap().visible);
        app.apply(Action::TogglePanelVisible(Panel::Project));
        assert!(!app.workspaces.active_layout().slot(Panel::Project).unwrap().visible);
        app.apply(Action::TogglePanelVisible(Panel::Project));
        assert!(app.workspaces.active_layout().slot(Panel::Project).unwrap().visible);
    }

    #[test]
    fn action_add_workspace() {
        let mut app = App::new();
        app.apply(Action::AddWorkspace("Custom".to_string()));
        assert_eq!(app.workspaces.workspaces.len(), 6);
        assert_eq!(app.workspaces.workspaces[5].name, "Custom");
        app.apply(Action::RemoveWorkspace(5));
        assert_eq!(app.workspaces.workspaces.len(), 5);
    }
}
