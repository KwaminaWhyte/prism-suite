//! Scripting sandbox — embed [`rhai`] (pure-Rust, no system deps) to drive the
//! editor programmatically. A script does not touch `App` directly: it calls a
//! handful of registered command functions that *enqueue* [`ScriptCmd`]s into a
//! shared buffer. After the script runs, every queued command is translated to
//! an [`Action`] and dispatched through [`App::apply`]. This keeps the script
//! sandboxed (it can only emit the commands we expose) and deterministic.
//!
//! Read-only document facts are injected as plain variables before evaluation:
//! `doc_w`, `doc_h`, `layer_count`, `active_layer`. So a script can branch on the
//! canvas size or loop `layer_count` times.
//!
//! ## Registered commands
//! - `select_all()` / `select_none()` / `invert_selection()`
//! - `fill(r, g, b)` — set the brush color then fill the selection/foreground
//! - `set_brush_size(px)` / `set_color(r, g, b)`
//! - `new_layer()` / `flatten()`
//! - `add_adjustment(name)` — `"invert"`, `"posterize"`, `"threshold"`, …
//! - `set_tool(name)` — `"brush"`, `"eraser"`, `"move"`, …
//! - `log(msg)` — append to the script log
//!
//! A tiny **line-DSL** fallback ([`parse_dsl`]) parses the same verbs as
//! whitespace-separated lines (`select_all`, `fill 255 0 0`, `flatten`) and is
//! used when a script does not look like rhai (no parentheses / semicolons) — so
//! both `fill(255,0,0)` and `fill 255 0 0` work.

use std::cell::RefCell;
use std::rc::Rc;

use rhai::{Dynamic, Engine};

use super::{Action, AdjKind, App, Tool};

/// A single editor command emitted by a script, later mapped to an [`Action`].
#[derive(Debug, Clone, PartialEq)]
pub enum ScriptCmd {
    SelectAll,
    SelectNone,
    InvertSelection,
    SetColor([f32; 3]),
    Fill([f32; 3]),
    SetBrushSize(f32),
    NewLayer,
    Flatten,
    AddAdjustment(String),
    SetTool(String),
    Log(String),
}

/// Result of running a script: the actions that were dispatched plus any log
/// lines the script emitted (and an optional error string on failure).
#[derive(Debug, Clone, Default)]
pub struct ScriptOutcome {
    pub actions: Vec<Action>,
    pub log: Vec<String>,
    pub error: Option<String>,
}

/// Map a `ScriptCmd` to zero or more concrete `Action`s.
fn cmd_to_actions(cmd: &ScriptCmd) -> Vec<Action> {
    match cmd {
        ScriptCmd::SelectAll => vec![Action::SelectAll],
        ScriptCmd::SelectNone => vec![Action::ClearSelection],
        ScriptCmd::InvertSelection => vec![Action::InvertSelection],
        ScriptCmd::SetColor([r, g, b]) => vec![Action::SetBrushColor([*r, *g, *b, 1.0])],
        ScriptCmd::Fill([r, g, b]) => {
            // Set the foreground color, then add a solid-color fill layer in it —
            // a deterministic, position-free fill the script can rely on.
            let to_u8 = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
            vec![
                Action::SetBrushColor([*r, *g, *b, 1.0]),
                Action::AddSolidFillLayer([to_u8(*r), to_u8(*g), to_u8(*b), 255]),
            ]
        }
        ScriptCmd::SetBrushSize(px) => vec![Action::SetBrushSize(*px)],
        ScriptCmd::NewLayer => vec![Action::NewLayer],
        ScriptCmd::Flatten => vec![Action::FlattenLayers],
        ScriptCmd::AddAdjustment(name) => match adj_kind_from_name(name) {
            Some(k) => vec![Action::AddAdjustment(k)],
            None => vec![],
        },
        ScriptCmd::SetTool(name) => match tool_from_name(name) {
            Some(t) => vec![Action::SetTool(t)],
            None => vec![],
        },
        ScriptCmd::Log(_) => vec![],
    }
}

/// Resolve a textual adjustment name to an [`AdjKind`].
fn adj_kind_from_name(name: &str) -> Option<AdjKind> {
    Some(match name.to_ascii_lowercase().as_str() {
        "brightnesscontrast" | "brightness_contrast" | "brightness" => AdjKind::BrightnessContrast,
        "levels" => AdjKind::Levels,
        "curves" => AdjKind::Curves,
        "huesaturation" | "hue_saturation" | "hue" => AdjKind::HueSaturation,
        "exposure" => AdjKind::Exposure,
        "vibrance" => AdjKind::Vibrance,
        "photofilter" | "photo_filter" => AdjKind::PhotoFilter,
        "posterize" => AdjKind::Posterize,
        "gradientmap" | "gradient_map" => AdjKind::GradientMap,
        "colorbalance" | "color_balance" => AdjKind::ColorBalance,
        "channelmixer" | "channel_mixer" => AdjKind::ChannelMixer,
        "blackwhite" | "black_white" | "bw" => AdjKind::BlackWhite,
        "threshold" => AdjKind::Threshold,
        "invert" => AdjKind::Invert,
        _ => return None,
    })
}

/// Resolve a textual tool name to a [`Tool`].
fn tool_from_name(name: &str) -> Option<Tool> {
    Some(match name.to_ascii_lowercase().as_str() {
        "move" => Tool::Move,
        "movelayer" | "move_layer" => Tool::MoveLayer,
        "brush" => Tool::Brush,
        "eraser" => Tool::Eraser,
        "clone" => Tool::Clone,
        "heal" => Tool::Heal,
        "dodge" => Tool::Dodge,
        "burn" => Tool::Burn,
        "smudge" => Tool::Smudge,
        "fill" | "bucket" => Tool::Fill,
        "eyedropper" => Tool::Eyedropper,
        "selectrect" | "marquee" => Tool::SelectRect,
        "selectellipse" => Tool::SelectEllipse,
        "lasso" => Tool::Lasso,
        "magicwand" | "wand" => Tool::MagicWand,
        "transform" => Tool::Transform,
        "crop" => Tool::Crop,
        "text" | "type" => Tool::Text,
        "pen" => Tool::Pen,
        "gradient" => Tool::Gradient,
        _ => return None,
    })
}

/// Normalize a color triple. Values > 1 are treated as 0..255 and scaled to
/// 0..1; otherwise taken as already-normalized 0..1. Always clamped to [0,1].
fn norm_color(r: f64, g: f64, b: f64) -> [f32; 3] {
    let scale = if r > 1.0 || g > 1.0 || b > 1.0 { 1.0 / 255.0 } else { 1.0 };
    [
        ((r * scale) as f32).clamp(0.0, 1.0),
        ((g * scale) as f32).clamp(0.0, 1.0),
        ((b * scale) as f32).clamp(0.0, 1.0),
    ]
}

fn as_f64(v: &Dynamic) -> f64 {
    if let Some(f) = v.clone().try_cast::<f64>() {
        f
    } else if let Some(i) = v.clone().try_cast::<i64>() {
        i as f64
    } else {
        0.0
    }
}

/// Heuristic: does this source look like rhai (has parens or semicolons), or is
/// it the plain line-DSL?
fn looks_like_rhai(src: &str) -> bool {
    src.contains('(') || src.contains(';') || src.contains('{')
}

/// Parse the line-DSL into `ScriptCmd`s. Each non-empty, non-`#`-comment line is
/// `verb [args...]`. Unknown verbs are reported as an error on that line.
pub fn parse_dsl(src: &str) -> Result<Vec<ScriptCmd>, String> {
    let mut out = Vec::new();
    for (lineno, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let mut parts = line.split_whitespace();
        let verb = parts.next().unwrap();
        let args: Vec<&str> = parts.collect();
        let num = |i: usize| -> f64 { args.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) };
        let cmd = match verb {
            "select_all" => ScriptCmd::SelectAll,
            "select_none" => ScriptCmd::SelectNone,
            "invert_selection" => ScriptCmd::InvertSelection,
            "new_layer" => ScriptCmd::NewLayer,
            "flatten" => ScriptCmd::Flatten,
            "fill" => ScriptCmd::Fill(norm_color(num(0), num(1), num(2))),
            "set_color" => ScriptCmd::SetColor(norm_color(num(0), num(1), num(2))),
            "set_brush_size" => ScriptCmd::SetBrushSize(num(0) as f32),
            "add_adjustment" => ScriptCmd::AddAdjustment(args.first().unwrap_or(&"").to_string()),
            "set_tool" => ScriptCmd::SetTool(args.first().unwrap_or(&"").to_string()),
            "log" => ScriptCmd::Log(args.join(" ")),
            other => return Err(format!("line {}: unknown command '{other}'", lineno + 1)),
        };
        out.push(cmd);
    }
    Ok(out)
}

/// Compile + run a rhai script against `(doc_w, doc_h, layer_count, active_layer)`,
/// returning the queued commands. A parse/eval error is returned as `Err`.
pub fn run_rhai(
    src: &str,
    doc_w: u32,
    doc_h: u32,
    layer_count: usize,
    active_layer: i64,
) -> Result<Vec<ScriptCmd>, String> {
    let queue: Rc<RefCell<Vec<ScriptCmd>>> = Rc::new(RefCell::new(Vec::new()));
    let mut engine = Engine::new();
    // Keep scripts cheap and bounded so a bad loop can't hang the editor.
    engine.set_max_operations(500_000);
    engine.set_max_call_levels(32);
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(16 * 1024);

    macro_rules! reg0 {
        ($name:literal, $cmd:expr) => {{
            let q = queue.clone();
            engine.register_fn($name, move || q.borrow_mut().push($cmd));
        }};
    }
    reg0!("select_all", ScriptCmd::SelectAll);
    reg0!("select_none", ScriptCmd::SelectNone);
    reg0!("invert_selection", ScriptCmd::InvertSelection);
    reg0!("new_layer", ScriptCmd::NewLayer);
    reg0!("flatten", ScriptCmd::Flatten);

    {
        let q = queue.clone();
        engine.register_fn("set_brush_size", move |px: Dynamic| {
            q.borrow_mut().push(ScriptCmd::SetBrushSize(as_f64(&px) as f32));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_color", move |r: Dynamic, g: Dynamic, b: Dynamic| {
            q.borrow_mut()
                .push(ScriptCmd::SetColor(norm_color(as_f64(&r), as_f64(&g), as_f64(&b))));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("fill", move |r: Dynamic, g: Dynamic, b: Dynamic| {
            q.borrow_mut()
                .push(ScriptCmd::Fill(norm_color(as_f64(&r), as_f64(&g), as_f64(&b))));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("add_adjustment", move |name: &str| {
            q.borrow_mut().push(ScriptCmd::AddAdjustment(name.to_string()));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("set_tool", move |name: &str| {
            q.borrow_mut().push(ScriptCmd::SetTool(name.to_string()));
        });
    }
    {
        let q = queue.clone();
        engine.register_fn("log", move |msg: &str| {
            q.borrow_mut().push(ScriptCmd::Log(msg.to_string()));
        });
    }

    let mut scope = rhai::Scope::new();
    scope.push_constant("doc_w", doc_w as i64);
    scope.push_constant("doc_h", doc_h as i64);
    scope.push_constant("layer_count", layer_count as i64);
    scope.push_constant("active_layer", active_layer);

    engine
        .run_with_scope(&mut scope, src)
        .map_err(|e| e.to_string())?;

    let cmds = queue.borrow().clone();
    Ok(cmds)
}

impl App {
    /// Compile `src` (rhai or line-DSL, auto-detected) into [`ScriptCmd`]s using
    /// the current document facts. Does NOT mutate the app.
    pub fn compile_script(&self, src: &str) -> Result<Vec<ScriptCmd>, String> {
        let active = self.doc.active_layer.map(|l| l.0 as i64).unwrap_or(-1);
        if looks_like_rhai(src) {
            run_rhai(
                src,
                self.doc.size.width,
                self.doc.size.height,
                self.doc.layers.layers.len(),
                active,
            )
        } else {
            parse_dsl(src)
        }
    }

    /// Run `src`: compile to commands, translate to actions, dispatch each, and
    /// return the [`ScriptOutcome`]. A compile error short-circuits with the
    /// actions left empty and the error recorded.
    pub fn run_script(&mut self, src: &str) -> ScriptOutcome {
        let mut outcome = ScriptOutcome::default();
        let cmds = match self.compile_script(src) {
            Ok(c) => c,
            Err(e) => {
                outcome.error = Some(e.clone());
                self.status_message = Some(format!("Script error: {e}"));
                self.script_log.push(format!("error: {e}"));
                return outcome;
            }
        };
        for cmd in &cmds {
            if let ScriptCmd::Log(msg) = cmd {
                outcome.log.push(msg.clone());
                self.script_log.push(msg.clone());
            }
            for action in cmd_to_actions(cmd) {
                outcome.actions.push(action.clone());
                self.apply(action);
            }
        }
        self.status_message = Some(format!("Script ran {} command(s)", cmds.len()));
        outcome
    }

    pub(super) fn apply_scripting(&mut self, action: Action) {
        match action {
            Action::RunScript { source } => {
                self.run_script(&source);
            }
            Action::SetScriptSource(source) => {
                self.script_source = source;
            }
            Action::RunCurrentScript => {
                let src = self.script_source.clone();
                self.run_script(&src);
            }
            Action::ClearScriptLog => {
                self.script_log.clear();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsl_parses_basic_verbs() {
        let cmds = parse_dsl("select_all\nfill 255 0 0\nflatten").unwrap();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[0], ScriptCmd::SelectAll);
        assert_eq!(cmds[1], ScriptCmd::Fill([1.0, 0.0, 0.0]));
        assert_eq!(cmds[2], ScriptCmd::Flatten);
    }

    #[test]
    fn dsl_skips_comments_and_blanks() {
        let src = "# a comment\n\nselect_all\n// another\nflatten\n";
        let cmds = parse_dsl(src).unwrap();
        assert_eq!(cmds.len(), 2);
    }

    #[test]
    fn dsl_unknown_command_errors() {
        let err = parse_dsl("frobnicate 1 2").unwrap_err();
        assert!(err.contains("unknown command"));
    }

    #[test]
    fn dsl_normalized_color() {
        // <=1 values stay as 0..1.
        let cmds = parse_dsl("set_color 0.5 0.25 1").unwrap();
        assert_eq!(cmds[0], ScriptCmd::SetColor([0.5, 0.25, 1.0]));
    }

    #[test]
    fn rhai_queues_commands() {
        let cmds = run_rhai("select_all(); fill(0, 128, 255);", 100, 50, 1, 0).unwrap();
        assert_eq!(cmds[0], ScriptCmd::SelectAll);
        // 128/255 ~ 0.50196.
        if let ScriptCmd::Fill([r, g, b]) = cmds[1] {
            assert_eq!(r, 0.0);
            assert!((g - 0.50196).abs() < 1e-3);
            assert_eq!(b, 1.0);
        } else {
            panic!("expected fill cmd");
        }
    }

    #[test]
    fn rhai_can_branch_on_doc_size() {
        let src = "if doc_w > 200 { new_layer(); } else { flatten(); }";
        let cmds = run_rhai(src, 100, 100, 1, 0).unwrap();
        assert_eq!(cmds, vec![ScriptCmd::Flatten]);
        let cmds2 = run_rhai(src, 400, 100, 1, 0).unwrap();
        assert_eq!(cmds2, vec![ScriptCmd::NewLayer]);
    }

    #[test]
    fn rhai_loop_uses_layer_count() {
        let src = "for i in 0..layer_count { new_layer(); }";
        let cmds = run_rhai(src, 10, 10, 3, 0).unwrap();
        assert_eq!(cmds.len(), 3);
        assert!(cmds.iter().all(|c| *c == ScriptCmd::NewLayer));
    }

    #[test]
    fn rhai_parse_error_is_err() {
        let res = run_rhai("this is not valid (((", 10, 10, 1, 0);
        assert!(res.is_err());
    }

    #[test]
    fn cmd_to_actions_fill_sets_color_then_fills() {
        let actions = cmd_to_actions(&ScriptCmd::Fill([1.0, 0.0, 0.0]));
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], Action::SetBrushColor(_)));
        assert!(matches!(actions[1], Action::AddSolidFillLayer([255, 0, 0, 255])));
    }

    #[test]
    fn run_script_dsl_mutates_app() {
        let mut app = App::new();
        let out = app.run_script("set_brush_size 64\nset_color 255 0 0");
        assert!(out.error.is_none());
        assert_eq!(app.brush.size, 64.0);
        assert_eq!(app.brush.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn run_script_rhai_adds_adjustment() {
        let mut app = App::new();
        let before = app.doc.layers.layers.len();
        let out = app.run_script(r#"add_adjustment("invert");"#);
        assert!(out.error.is_none());
        assert_eq!(app.doc.layers.layers.len(), before + 1);
    }

    #[test]
    fn run_script_log_captured() {
        let mut app = App::new();
        let out = app.run_script(r#"log("hello"); log("world");"#);
        assert_eq!(out.log, vec!["hello", "world"]);
        assert!(app.script_log.iter().any(|l| l == "hello"));
    }

    #[test]
    fn run_script_error_recorded() {
        let mut app = App::new();
        let out = app.run_script("set_tool brush\nbogus_verb");
        assert!(out.error.is_some());
    }

    #[test]
    fn action_run_current_script() {
        let mut app = App::new();
        app.apply(Action::SetScriptSource("set_brush_size 5".into()));
        app.apply(Action::RunCurrentScript);
        assert_eq!(app.brush.size, 5.0);
    }
}
