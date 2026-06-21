# Prism — UI/UX guidelines

Research-backed conventions every Prism app should converge on. Sourced from Affinity v2 / unified-Affinity (the closest free, pro, single-window reference) and the Adobe suite.

> **All six apps run on GPUI.** All new UI work uses the shared `prism-ui` design system. See [UI_SYSTEM.md](./UI_SYSTEM.md) for the full implementation guide.

---

## What Affinity/Adobe do that we should match

1. **Studio = dockable, tabbed, collapsible panels** on the right (Affinity calls it the *Studio*). Panels group into tabbed stacks; each section collapses; the whole column scrolls. Users **show/hide** any panel from a **Window menu** (checkmark = visible) and **save/switch/share workspaces** ("Studio presets").

2. **Compact tool palette** on the far left — a thin icon column (never text labels). Related tools nest behind a **flyout** (long-press / corner triangle). Never a tall ungrouped list that overflows.

3. **Contextual toolbar (tool options) across the top**, directly under the main toolbar — its controls change with the active tool. Per-tool settings (brush size, shape corner, stroke) live here, NOT in the right panel.

4. **Personas / modes** (Affinity: Vector / Pixel / Layout): one document, multiple tool+panel sets. Our suite-level analog is the four apps + Dynamic Link.

5. **Every tall region scrolls.** No control is ever unreachable on a short window.

6. **Reset workspace** to defaults from the Window menu.

---

## Adopted Prism standard — GPUI implementation

All new UI work uses the **`prism-ui` shared crate** (`shared/prism-ui/`). Never hardcode colors, spacing, or sizes — use the design tokens.

### Design tokens in use

```rust
use prism_ui::{colors, spacing, radius, font_size, Icon};
use prism_ui::components::*;
```

- **Surfaces**: `colors::surface_bg()` → app bg; `colors::surface_raised()` → toolbar/buttons
- **Accent**: `colors::accent()` (#7c5af5 purple-indigo) → active tools, primary actions, selection
- **Text**: `colors::text_primary()` → values; `colors::text_secondary()` → labels/headers
- **Spacing**: `spacing::MD` (8px) standard gap; `spacing::LG` (12px) section padding
- **Radius**: `radius::SM` (3px) buttons; `radius::MD` (5px) panels

### Panel conventions

- Panel body: `bg(colors::surface_bg())`, `flex_col()`
- Section header: `panel_header("Section Name")` component → uppercase, muted, 10px
- Dividers: `divider()` component → 1px `surface_border`
- Rows: `stepper_row(label, value)` for numeric properties
- All panels scroll vertically in the right dock container

### Icons instead of text

Every tool button, transport control, and common action uses `icon_button(Icon::Xxx, active)`. No text labels on buttons in the tool strip or toolbar. Text is for labels, values, and list items only.

### No egui in GPUI crates

The GPUI binaries have zero egui dependency. Do not import `egui::*` in any `*-gpui` crate. The egui binaries run unchanged from their `*-app` crates.

---

## GPUI implementation patterns

### Canvas bounds recording

A transparent `canvas()` overlay element records its painted `Bounds<Pixels>` into an `Rc<Cell<Option<Bounds<Pixels>>>>` each frame. Mouse handlers read these bounds to map pointer coordinates to document space:

```rust
let b = bounds_cell.get().unwrap_or_default();
let doc_x = (event.position.x - b.origin.x) / b.size.width * doc_width;
```

### Animation loop

`window.request_animation_frame()` called in `render()` self-sustains the loop. Only activate when animation is running (playback, marching ants, waveform bars) — idle frames don't need it.

### App::apply choke point

All state mutation routes through `App::apply(Action)`. Panels are pure render functions (`fn render(app: &App, cx: ...) -> impl IntoElement`) that emit `Action`s via `cx.listener`. This keeps panels testable and lets them be ported independently.

### Dirty caching

`canvas_host.mark_dirty()` triggers a GPU recomposite on the next frame. Only composite when dirty — idle frames return the cached `RenderImage` directly.

---

## Per-app UI gaps (see each app's PLAN.md)

- **Pigment** — Dodge/Burn/Smudge/Crop; layer styles; clipping masks; color picker wheel; PSD I/O; dockable workspaces. See [PLAN.md](apps/pigment/PLAN.md).
- **Contour** — Gradient fill rendering; shape builder geometry; character/paragraph panels; PDF export; graphic styles. See [PLAN.md](apps/contour/PLAN.md).
- **Pulse** — RAM preview; parenting; 3D camera; more AE effects; full expression language. See [PLAN.md](apps/pulse/PLAN.md).
- **Reel** — Source/program dual viewer; audio playback output; snap; Lumetri Scopes; ProRes export. See [PLAN.md](apps/reel/PLAN.md).
- **Drift** — GPUI timeline panel; layers panel; canvas with checkerboard + rulers; vector drawing tools (pen/shapes); tweening UI; onion skinning. See [PLAN.md](apps/drift/PLAN.md).
- **Tone** — GPUI piano roll canvas; mixer channel strips with faders/knobs/meters; waveform thumbnail clips on timeline; beat-grid ruler; quantize panel. See [PLAN.md](apps/tone/PLAN.md).

---

## Sources

- [Affinity v2 UI redesign — Affinity Spotlight](https://affinityspotlight.com/article/redesigned-with-you-in-mind-the-all-new-affinity-v2-ui/)
- [Affinity Designer 2 — Get To Know The Interface (Envato Tuts+)](https://design.tutsplus.com/tutorials/affinity-designer-2-get-to-know-the-interface--cms-108766)
- [Affinity (Canva) — unified Pixel/Vector/Layout Studios](https://www.affinity.studio/)
- [UI_SYSTEM.md](./UI_SYSTEM.md) — full GPUI implementation guide and component reference
