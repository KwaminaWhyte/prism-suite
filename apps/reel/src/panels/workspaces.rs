//! Workspaces switcher — a top-bar control that swaps the visible panel
//! layout, mirroring Premiere's workspace tabs (Editing / Color / Audio /
//! Effects / Graphics).
//!
//! Reel has no dedicated `SwitchWorkspace` action; a workspace here is simply a
//! known combination of which dock panels are open. The switcher reads the
//! existing panel-visibility booleans on [`App`] to decide which workspace is
//! "active" and emits the existing per-panel toggle [`Action`]s to bring the
//! requested layout into being. This keeps the panel a pure consumer of
//! existing state + actions, per the panel convention in [`super`].

use gpui::{
    div, px, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};
use prism_ui::colors;

use crate::app_state::{Action, App};
use crate::Reel;

/// The named workspaces the switcher offers. Each maps to a target set of
/// open panels expressed purely as existing `App` booleans.
#[derive(Clone, Copy, PartialEq)]
pub enum Workspace {
    Editing,
    Color,
    Audio,
    Effects,
    Graphics,
}

impl Workspace {
    fn label(self) -> &'static str {
        match self {
            Workspace::Editing => "Editing",
            Workspace::Color => "Color",
            Workspace::Audio => "Audio",
            Workspace::Effects => "Effects",
            Workspace::Graphics => "Graphics",
        }
    }

    /// The target panel layout: `(mixer, scopes, lumetri, bins, mogr)` open
    /// flags. `dual_viewer` is left untouched so source/program preference
    /// survives a workspace switch.
    fn layout(self) -> WorkspaceLayout {
        match self {
            // Editing: clean — everything closed, just timeline + preview.
            Workspace::Editing => WorkspaceLayout { mixer: false, scopes: false, lumetri: false, bins: true, mogr: false },
            // Color: scopes + Lumetri grading.
            Workspace::Color => WorkspaceLayout { mixer: false, scopes: true, lumetri: true, bins: false, mogr: false },
            // Audio: mixer up.
            Workspace::Audio => WorkspaceLayout { mixer: true, scopes: false, lumetri: false, bins: false, mogr: false },
            // Effects: Lumetri + bins (effect browser analog).
            Workspace::Effects => WorkspaceLayout { mixer: false, scopes: false, lumetri: true, bins: true, mogr: false },
            // Graphics: Essential Graphics library.
            Workspace::Graphics => WorkspaceLayout { mixer: false, scopes: false, lumetri: false, bins: false, mogr: true },
        }
    }
}

struct WorkspaceLayout {
    mixer: bool,
    scopes: bool,
    lumetri: bool,
    bins: bool,
    mogr: bool,
}

const ALL: [Workspace; 5] = [
    Workspace::Editing,
    Workspace::Color,
    Workspace::Audio,
    Workspace::Effects,
    Workspace::Graphics,
];

/// Read the current open-panel booleans into a layout snapshot so we can
/// compare against each workspace's target layout to highlight the active tab.
fn current_layout(app: &App) -> WorkspaceLayout {
    WorkspaceLayout {
        mixer: app.show_mixer,
        scopes: app.scopes_open,
        lumetri: app.lumetri_panel_open,
        bins: app.bins_open,
        mogr: app.mogr_library_open,
    }
}

fn layouts_match(a: &WorkspaceLayout, b: &WorkspaceLayout) -> bool {
    a.mixer == b.mixer
        && a.scopes == b.scopes
        && a.lumetri == b.lumetri
        && a.bins == b.bins
        && a.mogr == b.mogr
}

/// Emit the toggle actions needed to move from the current layout to `target`.
/// Each panel has exactly one boolean and one toggle action; we toggle only the
/// panels whose desired state differs from the current one.
fn apply_workspace(root: &mut Reel, target: &WorkspaceLayout) {
    let cur = current_layout(&root.app);
    if cur.mixer != target.mixer {
        root.app.apply(Action::ToggleMixer);
    }
    if cur.scopes != target.scopes {
        root.app.apply(Action::ToggleScopes);
    }
    if cur.lumetri != target.lumetri {
        root.app.apply(Action::ToggleLumetriPanel);
    }
    if cur.bins != target.bins {
        root.app.apply(Action::ToggleBins);
    }
    if cur.mogr != target.mogr {
        root.app.apply(Action::ToggleMogrLibrary);
    }
}

pub fn render(app: &App, cx: &mut Context<Reel>) -> impl IntoElement {
    let cur = current_layout(app);

    let tabs: Vec<gpui::AnyElement> = ALL
        .iter()
        .enumerate()
        .map(|(i, &ws)| {
            let active = layouts_match(&cur, &ws.layout());
            div()
                .id(("workspace-tab", i))
                .px(px(8.0))
                .py(px(3.0))
                .rounded_sm()
                .cursor_pointer()
                .bg(if active { colors::accent() } else { colors::surface_overlay() })
                .text_color(if active { colors::text_primary() } else { colors::text_secondary() })
                .text_size(px(10.0))
                .hover(|s| s.bg(if active { colors::accent_hover() } else { colors::tool_hover() }))
                .on_click(cx.listener(move |root, _ev, _win, cx| {
                    apply_workspace(root, &ws.layout());
                    cx.notify();
                }))
                .child(ws.label())
                .into_any_element()
        })
        .collect();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(3.0))
        .child(
            div()
                .text_color(colors::text_disabled())
                .text_size(px(9.0))
                .mr(px(2.0))
                .child("WS"),
        )
        .children(tabs)
}
