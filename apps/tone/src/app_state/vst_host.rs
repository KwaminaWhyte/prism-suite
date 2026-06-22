//! VST host domain — plugin scanning, loading, bypassing, presets, blacklist.

use super::{App, Action};

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub enum VstFormat {
    Vst2,
    Vst3,
    AudioUnit,
    Clap,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VstHostStatus {
    Unscanned,
    Scanning,
    Active,
    Bypassed,
    Error,
    Blacklisted,
}

#[derive(Clone, Debug)]
pub struct VstPluginInfo {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub format: VstFormat,
    pub input_channels: u8,
    pub output_channels: u8,
    pub has_gui: bool,
}

#[derive(Clone, Debug)]
pub struct LoadedVst {
    pub instance_id: usize,
    pub info: VstPluginInfo,
    pub track_id: Option<usize>,
    pub slot_index: usize,
    pub status: VstHostStatus,
    pub bypass: bool,
    pub preset_name: String,
}

// ─── impl App ─────────────────────────────────────────────────────────────────

impl App {
    pub(super) fn apply_vst_host(&mut self, action: Action) {
        match action {
            Action::ScanVstDirectory { path } => {
                self.vst_scan_path = path;
                self.vst_scan_running = true;
            }
            Action::CompletVstScan { found_plugins } => {
                for plugin in found_plugins {
                    self.vst_scan_results.push(plugin);
                }
                self.vst_scan_running = false;
            }
            Action::LoadVst { plugin_id, track_id, slot_index } => {
                if let Some(info) = self
                    .vst_scan_results
                    .iter()
                    .find(|p| p.id == plugin_id)
                    .cloned()
                {
                    let id = self.next_vst_instance_id;
                    self.next_vst_instance_id += 1;
                    self.loaded_vsts.push(LoadedVst {
                        instance_id: id,
                        info,
                        track_id,
                        slot_index,
                        status: VstHostStatus::Active,
                        bypass: false,
                        preset_name: String::new(),
                    });
                }
            }
            Action::UnloadVst { vst_instance_id } => {
                self.loaded_vsts.retain(|v| v.instance_id != vst_instance_id);
            }
            Action::BypassVst { vst_instance_id, bypass } => {
                if let Some(vst) = self.loaded_vsts.iter_mut().find(|v| v.instance_id == vst_instance_id) {
                    vst.bypass = bypass;
                    vst.status = if bypass { VstHostStatus::Bypassed } else { VstHostStatus::Active };
                }
            }
            Action::SetVstPreset { vst_instance_id, preset_name } => {
                if let Some(vst) = self.loaded_vsts.iter_mut().find(|v| v.instance_id == vst_instance_id) {
                    vst.preset_name = preset_name;
                }
            }
            Action::BlacklistVst { plugin_id } => {
                if !self.vst_blacklist.contains(&plugin_id) {
                    self.vst_blacklist.push(plugin_id.clone());
                }
                for vst in self.loaded_vsts.iter_mut() {
                    if vst.info.id == plugin_id {
                        vst.status = VstHostStatus::Blacklisted;
                    }
                }
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

    fn make_plugin(id: &str) -> VstPluginInfo {
        VstPluginInfo {
            id: id.to_string(),
            name: format!("Plugin {id}"),
            vendor: "Vendor".to_string(),
            version: "1.0.0".to_string(),
            format: VstFormat::Vst3,
            input_channels: 2,
            output_channels: 2,
            has_gui: true,
        }
    }

    #[test]
    fn scan_sets_path_and_running() {
        let mut app = fresh();
        app.apply(Action::ScanVstDirectory { path: "/usr/lib/vst".to_string() });
        assert_eq!(app.vst_scan_path, "/usr/lib/vst");
        assert!(app.vst_scan_running);
    }

    #[test]
    fn complete_scan_populates_results_and_clears_running() {
        let mut app = fresh();
        app.apply(Action::ScanVstDirectory { path: "/lib".to_string() });
        let plugins = vec![make_plugin("abc"), make_plugin("def")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        assert!(!app.vst_scan_running);
        assert_eq!(app.vst_scan_results.len(), 2);
    }

    #[test]
    fn load_vst_creates_loaded_instance() {
        let mut app = fresh();
        let plugins = vec![make_plugin("myplugin")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "myplugin".to_string(), track_id: Some(1), slot_index: 0 });
        assert_eq!(app.loaded_vsts.len(), 1);
        assert_eq!(app.loaded_vsts[0].status, VstHostStatus::Active);
        assert_eq!(app.loaded_vsts[0].track_id, Some(1));
    }

    #[test]
    fn load_vst_unknown_plugin_does_nothing() {
        let mut app = fresh();
        app.apply(Action::LoadVst { plugin_id: "nonexistent".to_string(), track_id: None, slot_index: 0 });
        assert!(app.loaded_vsts.is_empty());
    }

    #[test]
    fn unload_vst_removes_instance() {
        let mut app = fresh();
        let plugins = vec![make_plugin("p1")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "p1".to_string(), track_id: None, slot_index: 0 });
        let iid = app.loaded_vsts[0].instance_id;
        app.apply(Action::UnloadVst { vst_instance_id: iid });
        assert!(app.loaded_vsts.is_empty());
    }

    #[test]
    fn bypass_vst_sets_bypass_and_status() {
        let mut app = fresh();
        let plugins = vec![make_plugin("p2")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "p2".to_string(), track_id: None, slot_index: 0 });
        let iid = app.loaded_vsts[0].instance_id;
        app.apply(Action::BypassVst { vst_instance_id: iid, bypass: true });
        assert!(app.loaded_vsts[0].bypass);
        assert_eq!(app.loaded_vsts[0].status, VstHostStatus::Bypassed);
    }

    #[test]
    fn unbypass_vst_restores_active() {
        let mut app = fresh();
        let plugins = vec![make_plugin("p3")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "p3".to_string(), track_id: None, slot_index: 0 });
        let iid = app.loaded_vsts[0].instance_id;
        app.apply(Action::BypassVst { vst_instance_id: iid, bypass: true });
        app.apply(Action::BypassVst { vst_instance_id: iid, bypass: false });
        assert!(!app.loaded_vsts[0].bypass);
        assert_eq!(app.loaded_vsts[0].status, VstHostStatus::Active);
    }

    #[test]
    fn set_vst_preset_updates_name() {
        let mut app = fresh();
        let plugins = vec![make_plugin("p4")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "p4".to_string(), track_id: None, slot_index: 0 });
        let iid = app.loaded_vsts[0].instance_id;
        app.apply(Action::SetVstPreset { vst_instance_id: iid, preset_name: "Warm Strings".to_string() });
        assert_eq!(app.loaded_vsts[0].preset_name, "Warm Strings");
    }

    #[test]
    fn blacklist_vst_adds_to_blacklist() {
        let mut app = fresh();
        app.apply(Action::BlacklistVst { plugin_id: "bad_plugin".to_string() });
        assert!(app.vst_blacklist.contains(&"bad_plugin".to_string()));
    }

    #[test]
    fn blacklist_vst_no_duplicates() {
        let mut app = fresh();
        app.apply(Action::BlacklistVst { plugin_id: "dup".to_string() });
        app.apply(Action::BlacklistVst { plugin_id: "dup".to_string() });
        assert_eq!(app.vst_blacklist.iter().filter(|x| *x == "dup").count(), 1);
    }

    #[test]
    fn blacklist_vst_sets_loaded_status() {
        let mut app = fresh();
        let plugins = vec![make_plugin("badvst")];
        app.apply(Action::CompletVstScan { found_plugins: plugins });
        app.apply(Action::LoadVst { plugin_id: "badvst".to_string(), track_id: None, slot_index: 0 });
        app.apply(Action::BlacklistVst { plugin_id: "badvst".to_string() });
        assert_eq!(app.loaded_vsts[0].status, VstHostStatus::Blacklisted);
    }
}
