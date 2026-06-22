use super::{App, Action};

/// Supported scripting languages for frame scripts and button actions.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptLanguage {
    JavaScript,
    TypeScript,
    Lua,
}

/// What a script is attached to.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptTarget {
    Global,
    Layer { layer_id: usize },
    Scene { scene_id: usize },
}

/// When a script is triggered.
#[derive(Clone, Debug, PartialEq)]
pub enum ScriptEvent {
    OnEnterFrame,
    OnLoad,
    OnClick { layer_id: usize },
    OnKeyDown { key: String },
    OnCustomEvent { name: String },
}

/// A single script attached to the document.
#[derive(Clone, Debug)]
pub struct Script {
    pub id: usize,
    pub name: String,
    pub language: ScriptLanguage,
    pub source: String,
    pub attached_to: ScriptTarget,
    pub enabled: bool,
    pub run_on: ScriptEvent,
    pub error: Option<String>,
}

/// Log level for script console output.
#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

/// A single entry in the script console log.
#[derive(Clone, Debug)]
pub struct ScriptLogEntry {
    pub level: LogLevel,
    pub message: String,
    pub script_id: Option<usize>,
    pub frame: usize,
}

/// Script output console — buffers log entries up to `max_entries`.
#[derive(Clone, Debug)]
pub struct ScriptConsole {
    pub log: Vec<ScriptLogEntry>,
    pub max_entries: usize,
}

impl ScriptConsole {
    pub fn new() -> Self {
        Self { log: Vec::new(), max_entries: 500 }
    }

    pub fn append(&mut self, entry: ScriptLogEntry) {
        if self.log.len() >= self.max_entries {
            self.log.remove(0);
        }
        self.log.push(entry);
    }
}

impl App {
    pub fn apply_scripting(&mut self, action: Action) {
        match action {
            Action::AddScript { name, language } => {
                let id = self.next_script_id;
                self.next_script_id += 1;
                self.scripts.push(Script {
                    id,
                    name,
                    language,
                    source: String::new(),
                    attached_to: ScriptTarget::Global,
                    enabled: true,
                    run_on: ScriptEvent::OnEnterFrame,
                    error: None,
                });
            }
            Action::RemoveScript { script_id } => {
                self.scripts.retain(|s| s.id != script_id);
            }
            Action::RenameScript { script_id, name } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.name = name;
                }
            }
            Action::SetScriptSource { script_id, source } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.source = source;
                    // Editing source clears any previous error.
                    s.error = None;
                }
            }
            Action::SetScriptTarget { script_id, target } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.attached_to = target;
                }
            }
            Action::SetScriptEvent { script_id, event } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.run_on = event;
                }
            }
            Action::ToggleScript { script_id } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.enabled = !s.enabled;
                }
            }
            Action::SetScriptError { script_id, error } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.error = error;
                }
            }
            Action::ClearScriptError { script_id } => {
                if let Some(s) = self.scripts.iter_mut().find(|s| s.id == script_id) {
                    s.error = None;
                }
            }
            Action::AppendScriptLog { level, message, script_id } => {
                let frame = self.current_frame;
                let entry = ScriptLogEntry { level, message, script_id, frame };
                self.script_console.append(entry);
            }
            Action::ClearScriptConsole => {
                self.script_console.log.clear();
            }
            Action::SetScriptExecutionEnabled(enabled) => {
                self.script_execution_enabled = enabled;
            }
            _ => {}
        }
    }

    /// Returns all enabled scripts.
    pub fn enabled_scripts(&self) -> Vec<&Script> {
        self.scripts.iter().filter(|s| s.enabled).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Action};
    use super::{ScriptLanguage, ScriptTarget, ScriptEvent, LogLevel};

    fn app() -> App {
        App::new()
    }

    #[test]
    fn test_add_script() {
        let mut a = app();
        a.apply(Action::AddScript { name: "frame_script".to_string(), language: ScriptLanguage::JavaScript });
        assert_eq!(a.scripts.len(), 1);
        assert_eq!(a.scripts[0].name, "frame_script");
        assert!(a.scripts[0].enabled);
    }

    #[test]
    fn test_add_multiple_scripts_unique_ids() {
        let mut a = app();
        a.apply(Action::AddScript { name: "A".to_string(), language: ScriptLanguage::JavaScript });
        a.apply(Action::AddScript { name: "B".to_string(), language: ScriptLanguage::Lua });
        assert_ne!(a.scripts[0].id, a.scripts[1].id);
    }

    #[test]
    fn test_remove_script() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::RemoveScript { script_id: id });
        assert!(a.scripts.is_empty());
    }

    #[test]
    fn test_rename_script() {
        let mut a = app();
        a.apply(Action::AddScript { name: "Old".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::RenameScript { script_id: id, name: "New".to_string() });
        assert_eq!(a.scripts[0].name, "New");
    }

    #[test]
    fn test_set_script_source_clears_error() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::SetScriptError { script_id: id, error: Some("SyntaxError".to_string()) });
        assert!(a.scripts[0].error.is_some());
        a.apply(Action::SetScriptSource { script_id: id, source: "trace('hello')".to_string() });
        assert!(a.scripts[0].error.is_none());
    }

    #[test]
    fn test_set_script_target() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::SetScriptTarget { script_id: id, target: ScriptTarget::Layer { layer_id: 3 } });
        assert_eq!(a.scripts[0].attached_to, ScriptTarget::Layer { layer_id: 3 });
    }

    #[test]
    fn test_set_script_event() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::TypeScript });
        let id = a.scripts[0].id;
        a.apply(Action::SetScriptEvent {
            script_id: id,
            event: ScriptEvent::OnKeyDown { key: "Space".to_string() },
        });
        assert_eq!(a.scripts[0].run_on, ScriptEvent::OnKeyDown { key: "Space".to_string() });
    }

    #[test]
    fn test_toggle_script() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::ToggleScript { script_id: id });
        assert!(!a.scripts[0].enabled);
        a.apply(Action::ToggleScript { script_id: id });
        assert!(a.scripts[0].enabled);
    }

    #[test]
    fn test_set_and_clear_script_error() {
        let mut a = app();
        a.apply(Action::AddScript { name: "S".to_string(), language: ScriptLanguage::JavaScript });
        let id = a.scripts[0].id;
        a.apply(Action::SetScriptError { script_id: id, error: Some("ReferenceError".to_string()) });
        assert_eq!(a.scripts[0].error, Some("ReferenceError".to_string()));
        a.apply(Action::ClearScriptError { script_id: id });
        assert!(a.scripts[0].error.is_none());
    }

    #[test]
    fn test_append_script_log() {
        let mut a = app();
        a.apply(Action::AppendScriptLog { level: LogLevel::Info, message: "hello".to_string(), script_id: None });
        assert_eq!(a.script_console.log.len(), 1);
        assert_eq!(a.script_console.log[0].message, "hello");
    }

    #[test]
    fn test_clear_script_console() {
        let mut a = app();
        a.apply(Action::AppendScriptLog { level: LogLevel::Warn, message: "w".to_string(), script_id: None });
        a.apply(Action::AppendScriptLog { level: LogLevel::Error, message: "e".to_string(), script_id: Some(0) });
        assert_eq!(a.script_console.log.len(), 2);
        a.apply(Action::ClearScriptConsole);
        assert!(a.script_console.log.is_empty());
    }

    #[test]
    fn test_set_script_execution_enabled() {
        let mut a = app();
        assert!(a.script_execution_enabled);
        a.apply(Action::SetScriptExecutionEnabled(false));
        assert!(!a.script_execution_enabled);
        a.apply(Action::SetScriptExecutionEnabled(true));
        assert!(a.script_execution_enabled);
    }

    #[test]
    fn test_console_max_entries() {
        let mut a = app();
        a.script_console.max_entries = 3;
        for i in 0..5usize {
            a.apply(Action::AppendScriptLog {
                level: LogLevel::Debug,
                message: format!("msg{i}"),
                script_id: None,
            });
        }
        assert_eq!(a.script_console.log.len(), 3);
        assert_eq!(a.script_console.log[0].message, "msg2");
    }

    #[test]
    fn test_enabled_scripts_filter() {
        let mut a = app();
        a.apply(Action::AddScript { name: "A".to_string(), language: ScriptLanguage::JavaScript });
        a.apply(Action::AddScript { name: "B".to_string(), language: ScriptLanguage::Lua });
        let id_a = a.scripts[0].id;
        a.apply(Action::ToggleScript { script_id: id_a });
        let enabled = a.enabled_scripts();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].name, "B");
    }

    #[test]
    fn test_lua_script_language() {
        let mut a = app();
        a.apply(Action::AddScript { name: "L".to_string(), language: ScriptLanguage::Lua });
        assert_eq!(a.scripts[0].language, ScriptLanguage::Lua);
    }
}
