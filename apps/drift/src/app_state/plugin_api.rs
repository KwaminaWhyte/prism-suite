//! Plugin / extension API domain for Drift.

use super::{App, Action};

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum PluginKind {
    Effect,
    Generator,
    Tool,
    Export,
    Import,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PluginStatus {
    Unloaded,
    Loading,
    Active,
    Error,
    Disabled,
}

#[derive(Clone, Debug)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: PluginKind,
}

#[derive(Clone, Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub status: PluginStatus,
}

#[derive(Clone, Debug)]
pub struct ExtensionPanel {
    pub id: usize,
    pub plugin_id: String,
    pub title: String,
    pub visible: bool,
}

// ── impl App ──────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_plugin_api(&mut self, action: &Action) {
        match action {
            Action::RegisterPlugin { manifest } => {
                self.loaded_plugins.push(LoadedPlugin {
                    manifest: manifest.clone(),
                    status: PluginStatus::Unloaded,
                });
            }
            Action::EnablePlugin { plugin_id } => {
                if let Some(p) = self.loaded_plugins.iter_mut().find(|p| p.manifest.id == *plugin_id) {
                    p.status = PluginStatus::Active;
                }
            }
            Action::DisablePlugin { plugin_id } => {
                if let Some(p) = self.loaded_plugins.iter_mut().find(|p| p.manifest.id == *plugin_id) {
                    p.status = PluginStatus::Disabled;
                }
            }
            Action::UnloadPlugin { plugin_id } => {
                if let Some(p) = self.loaded_plugins.iter_mut().find(|p| p.manifest.id == *plugin_id) {
                    p.status = PluginStatus::Unloaded;
                }
            }
            Action::OpenExtensionPanel { plugin_id, title } => {
                let id = self.next_ext_panel_id;
                self.next_ext_panel_id += 1;
                self.extension_panels.push(ExtensionPanel {
                    id,
                    plugin_id: plugin_id.clone(),
                    title: title.clone(),
                    visible: true,
                });
            }
            Action::CloseExtensionPanel { panel_id } => {
                if let Some(p) = self.extension_panels.iter_mut().find(|p| p.id == *panel_id) {
                    p.visible = false;
                }
            }
            Action::ToggleExtensionPanel { panel_id } => {
                if let Some(p) = self.extension_panels.iter_mut().find(|p| p.id == *panel_id) {
                    p.visible = !p.visible;
                }
            }
            _ => {}
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{PluginKind, PluginManifest, PluginStatus};

    fn app() -> App {
        App::new()
    }

    fn manifest(id: &str) -> PluginManifest {
        PluginManifest {
            id: id.to_string(),
            name: format!("Plugin {}", id),
            version: "1.0.0".to_string(),
            kind: PluginKind::Effect,
        }
    }

    #[test]
    fn test_register_plugin() {
        let mut a = app();
        a.apply(Action::RegisterPlugin { manifest: manifest("fx.glow") });
        assert_eq!(a.loaded_plugins.len(), 1);
        assert_eq!(a.loaded_plugins[0].status, PluginStatus::Unloaded);
        assert_eq!(a.loaded_plugins[0].manifest.id, "fx.glow");
    }

    #[test]
    fn test_enable_plugin() {
        let mut a = app();
        a.apply(Action::RegisterPlugin { manifest: manifest("fx.blur") });
        a.apply(Action::EnablePlugin { plugin_id: "fx.blur".to_string() });
        assert_eq!(a.loaded_plugins[0].status, PluginStatus::Active);
    }

    #[test]
    fn test_disable_plugin() {
        let mut a = app();
        a.apply(Action::RegisterPlugin { manifest: manifest("fx.blur") });
        a.apply(Action::EnablePlugin { plugin_id: "fx.blur".to_string() });
        a.apply(Action::DisablePlugin { plugin_id: "fx.blur".to_string() });
        assert_eq!(a.loaded_plugins[0].status, PluginStatus::Disabled);
    }

    #[test]
    fn test_unload_plugin() {
        let mut a = app();
        a.apply(Action::RegisterPlugin { manifest: manifest("gen.noise") });
        a.apply(Action::EnablePlugin { plugin_id: "gen.noise".to_string() });
        a.apply(Action::UnloadPlugin { plugin_id: "gen.noise".to_string() });
        assert_eq!(a.loaded_plugins[0].status, PluginStatus::Unloaded);
    }

    #[test]
    fn test_open_extension_panel() {
        let mut a = app();
        a.apply(Action::OpenExtensionPanel {
            plugin_id: "fx.glow".to_string(),
            title: "Glow Panel".to_string(),
        });
        assert_eq!(a.extension_panels.len(), 1);
        assert!(a.extension_panels[0].visible);
        assert_eq!(a.extension_panels[0].title, "Glow Panel");
    }

    #[test]
    fn test_open_multiple_panels_have_unique_ids() {
        let mut a = app();
        a.apply(Action::OpenExtensionPanel { plugin_id: "p1".to_string(), title: "T1".to_string() });
        a.apply(Action::OpenExtensionPanel { plugin_id: "p2".to_string(), title: "T2".to_string() });
        assert_ne!(a.extension_panels[0].id, a.extension_panels[1].id);
    }

    #[test]
    fn test_close_extension_panel() {
        let mut a = app();
        a.apply(Action::OpenExtensionPanel { plugin_id: "p".to_string(), title: "T".to_string() });
        let panel_id = a.extension_panels[0].id;
        a.apply(Action::CloseExtensionPanel { panel_id });
        assert!(!a.extension_panels[0].visible);
    }

    #[test]
    fn test_toggle_extension_panel_on_off() {
        let mut a = app();
        a.apply(Action::OpenExtensionPanel { plugin_id: "p".to_string(), title: "T".to_string() });
        let panel_id = a.extension_panels[0].id;
        // starts visible, toggle → hidden
        a.apply(Action::ToggleExtensionPanel { panel_id });
        assert!(!a.extension_panels[0].visible);
        // toggle again → visible
        a.apply(Action::ToggleExtensionPanel { panel_id });
        assert!(a.extension_panels[0].visible);
    }

    #[test]
    fn test_enable_unknown_plugin_is_noop() {
        let mut a = app();
        // Should not panic
        a.apply(Action::EnablePlugin { plugin_id: "nonexistent".to_string() });
        assert!(a.loaded_plugins.is_empty());
    }

    #[test]
    fn test_close_unknown_panel_is_noop() {
        let mut a = app();
        // Should not panic
        a.apply(Action::CloseExtensionPanel { panel_id: 999 });
    }
}
