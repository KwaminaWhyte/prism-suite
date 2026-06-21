# Prism UI System

Design system and implementation guide for all six Prism GPUI apps. Every panel, component, and layout decision should reference this document. Inconsistencies across apps are bugs.

---

## Framework

**GPUI** (Zed's GPU UI framework). Renders via the Metal backend on macOS (blade-graphics, NOT wgpu). All six apps share the same framework version pinned in their workspace `Cargo.toml`.

**Current version:** `gpui = "0.2.2"` (crates.io). Hand-written components live in `prism-ui/src/components.rs`.

Key GPUI concepts used across the suite:
- `div()` — the primary layout primitive (flexbox-like)
- `canvas()` — raw 2D paint surface; used for the canvas preview overlay and coordinate hit-testing
- `svg()` — renders an SVG file from an `AssetSource`; used for all icons
- `window.request_animation_frame()` — drives animation loops (playback, marching ants, waveform bars)
- `cx.listener(...)` — attaches event handlers that emit `Action`s to the root view
- `App::apply(Action)` — **the single mutation choke point**; all state changes go through here

---

## Shared crate: `prism-ui`

Lives at `shared/prism-ui/`. Import in any GPUI app:

```toml
prism-ui = { path = "../../shared/prism-ui" }
```

Provides: design tokens, icon enum + SVG assets, shared GPUI components. Never import raw hex colors or hardcode sizes — always use the tokens.

---

## Design philosophy

- **Dark pro-tool**: Affinity/Adobe aesthetic. Dark chrome, not light. No rounded-corner "consumer" style.
- **Flat panels**: No gradients on chrome. Subtle borders only. Color used for state (active, hover, danger), not decoration.
- **Typography over icons only where needed**: icons for tools and common actions, text for labels and values.
- **Density**: panels are compact. 8px vertical rhythm, 12px horizontal padding. Content fits without scrolling where possible.
- **Consistent affordance language**: the same button style means the same thing everywhere.

---

## Design tokens (`prism_ui::tokens`)

### Colors

| Token | Hex | Usage |
|---|---|---|
| `colors::surface_bg()` | `#1a1a1c` | App background, panel body |
| `colors::surface_raised()` | `#222226` | Toolbar, button backgrounds |
| `colors::surface_overlay()` | `#2a2a2f` | Dropdowns, tooltips |
| `colors::surface_border()` | `#333338` | Dividers, panel borders |
| `colors::text_primary()` | `#f0f0f2` | Primary labels, values |
| `colors::text_secondary()` | `#a0a0a8` | Muted labels, section headers |
| `colors::text_disabled()` | `#555560` | Disabled controls |
| `colors::accent()` | `#7c5af5` | Active tool, primary action, selection highlight |
| `colors::accent_hover()` | `#9070ff` | Hover state on accent elements |
| `colors::accent_pressed()` | `#6347d4` | Pressed/active state |
| `colors::success()` | `#3dc97e` | Export complete, positive state |
| `colors::warning()` | `#f5a623` | Warning badges |
| `colors::danger()` | `#f05555` | Delete, destructive actions |
| `colors::tool_active()` | `#7c5af5` | Active tool highlight in strip |
| `colors::tool_hover()` | `#2e2e36` | Hover on tool/icon buttons |

### Spacing

| Token | Value | Usage |
|---|---|---|
| `spacing::XS` | 2px | Tight gaps, icon padding |
| `spacing::SM` | 4px | Button vertical padding, small gaps |
| `spacing::MD` | 8px | Standard gap, panel item padding |
| `spacing::LG` | 12px | Panel section padding |
| `spacing::XL` | 16px | Large gaps, section margins |
| `spacing::XXL` | 24px | Panel-level margins |

### Border radius

| Token | Value | Usage |
|---|---|---|
| `radius::SM` | 3px | Buttons, tool icons |
| `radius::MD` | 5px | Panels, cards |
| `radius::LG` | 8px | Dialogs, overlays |
| `radius::PILL` | 999px | Toggle pills, badge chips |

### Font sizes

| Token | Value | Usage |
|---|---|---|
| `font_size::XS` | 10px | Section headers (uppercase) |
| `font_size::SM` | 11px | Panel labels, values |
| `font_size::MD` | 12px | Default body text |
| `font_size::LG` | 13px | Toolbar text, important labels |
| `font_size::XL` | 15px | Persona tab labels |
| `font_size::TITLE` | 18px | App title, dialog headings |

---

## Icon system (`prism_ui::icons`)

Icons are SVG files embedded in `prism-ui/assets/icons/`. Always use the `Icon` enum — never raw path strings.

```rust
use prism_ui::{Icon, components::icon};

// Render a 16px eye icon in primary text color
icon(Icon::Eye, 16.0, colors::text_primary())
```

SVG files use `viewBox="0 0 24 24"`, `stroke="currentColor"`, `fill="none"`, `stroke-width="1.5"`. The `text_color()` call on the `svg()` element drives `currentColor`.

### Icon reference

| Enum variant | File | When to use |
|---|---|---|
| `Icon::Cursor` | cursor.svg | Select / pointer tool |
| `Icon::Pen` | pen.svg | Pen / bézier tool |
| `Icon::Pencil` | pencil.svg | Pencil / freehand |
| `Icon::Brush` | brush.svg | Brush / paint tool |
| `Icon::Eraser` | eraser.svg | Eraser tool |
| `Icon::Move` | move.svg | Move tool |
| `Icon::ZoomIn` | zoom_in.svg | Zoom in |
| `Icon::ZoomOut` | zoom_out.svg | Zoom out |
| `Icon::Eyedropper` | eyedropper.svg | Color picker / eyedropper tool |
| `Icon::Text` | text.svg | Type / text tool |
| `Icon::Rect` | rect.svg | Rectangle shape tool |
| `Icon::Ellipse` | ellipse.svg | Ellipse / circle tool |
| `Icon::Line` | line.svg | Line tool |
| `Icon::Polygon` | polygon.svg | Polygon / star tool |
| `Icon::Lasso` | lasso.svg | Lasso selection |
| `Icon::Wand` | wand.svg | Magic wand selection |
| `Icon::Crop` | crop.svg | Crop tool |
| `Icon::Heal` | heal.svg | Heal / patch tool |
| `Icon::Clone` | clone.svg | Clone stamp |
| `Icon::Gradient` | gradient.svg | Gradient tool / fill |
| `Icon::Fill` | fill.svg | Paint bucket fill |
| `Icon::Shape` | shape.svg | Generic shape / star tool |
| `Icon::Eye` | eye.svg | Visibility on |
| `Icon::EyeOff` | eye_off.svg | Visibility off |
| `Icon::Lock` | lock.svg | Locked |
| `Icon::Unlock` | unlock.svg | Unlocked |
| `Icon::Layers` | layers.svg | Layers panel |
| `Icon::Add` | add.svg | Add item |
| `Icon::Remove` | remove.svg | Remove item |
| `Icon::Trash` | trash.svg | Delete |
| `Icon::Check` | check.svg | Confirm / done |
| `Icon::Close` | close.svg | Close / cancel |
| `Icon::ChevronDown` | chevron_down.svg | Expand / dropdown |
| `Icon::ChevronRight` | chevron_right.svg | Collapse / navigate |
| `Icon::Play` | play.svg | Play transport |
| `Icon::Pause` | pause.svg | Pause transport |
| `Icon::Stop` | stop.svg | Stop |
| `Icon::Rewind` | rewind.svg | Jump to start / rewind |
| `Icon::FastForward` | fast_forward.svg | Jump to end / skip |
| `Icon::Scissors` | scissors.svg | Cut / razor tool |
| `Icon::Link` | link.svg | Link / chain |
| `Icon::Unlink` | unlink.svg | Unlink |
| `Icon::Grid` | grid.svg | Grid / align |
| `Icon::Settings` | settings.svg | Settings / preferences |
| `Icon::Export` | export.svg | Export / save-as |
| `Icon::Import` | import.svg | Import / open |
| `Icon::Undo` | undo.svg | Undo |
| `Icon::Redo` | redo.svg | Redo |
| `Icon::AlignLeft` | align_left.svg | Align to left edge |
| `Icon::AlignCenter` | align_center.svg | Align centers horizontally |
| `Icon::AlignRight` | align_right.svg | Align to right edge |
| `Icon::AlignTop` | align_top.svg | Align to top edge |
| `Icon::AlignMiddle` | align_middle.svg | Align centers vertically |
| `Icon::AlignBottom` | align_bottom.svg | Align to bottom edge |
| `Icon::DistributeH` | distribute_h.svg | Distribute horizontally |
| `Icon::DistributeV` | distribute_v.svg | Distribute vertically |
| `Icon::Speaker` | speaker.svg | Audio / sound on |
| `Icon::Mute` | mute.svg | Audio muted |
| `Icon::Volume` | volume.svg | Volume control |
| `Icon::Camera` | camera.svg | Camera / video input |
| `Icon::Film` | film.svg | Video / film |
| `Icon::Music` | music.svg | Audio clip / music |
| `Icon::Waveform` | waveform.svg | Audio waveform display |
| `Icon::Cut` | cut.svg | Cut (edit menu) |
| `Icon::Transition` | transition.svg | Transition between clips |
| `Icon::ColorWheel` | color_wheel.svg | Color / hue |
| `Icon::Mask` | mask.svg | Layer mask |
| `Icon::Adjustment` | adjustment.svg | Adjustment layer / filter |
| `Icon::Histogram` | histogram.svg | Histogram / levels |
| `Icon::Star` | star.svg | Star / favorite |
| `Icon::ArrowUp` | arrow_up.svg | Move up |
| `Icon::ArrowDown` | arrow_down.svg | Move down |
| `Icon::ArrowLeft` | arrow_left.svg | Move left |
| `Icon::ArrowRight` | arrow_right.svg | Move right |

**Rule: no text labels for tool buttons, transport controls, or common actions.** Use `Icon` + tooltip. Text is for labels, values, and items in lists.

---

## Component library (`prism_ui::components`)

Import: `use prism_ui::components::*;`

### `icon(i: Icon, size: f32, color: Rgba) -> impl IntoElement`
Renders a single SVG icon at the given size and color.

### `icon_button(i: Icon, active: bool) -> impl IntoElement`
28×28px square icon button. Active state fills with `accent`. Use for tool strip, toolbar icon actions.

### `button(label: &str, variant: ButtonVariant) -> impl IntoElement`
Text button with three variants:
- `ButtonVariant::Primary` — accent background, primary text. Main CTA.
- `ButtonVariant::Ghost` — raised surface background, secondary text. Secondary actions.
- `ButtonVariant::Danger` — red background. Destructive actions.

### `divider() -> impl IntoElement`
1px horizontal rule in `surface_border`. Use between panel sections.

### `panel_header(title: &str) -> impl IntoElement`
Section label: uppercase, `text_secondary`, `font_size::XS`. Use to name panel sections.

### `stepper_row(label: &str, value: impl ToString) -> impl IntoElement`
Label-value pair row, space-between. For numeric properties. Pair with `cx.listener` on −/+ buttons alongside for mutation.

### `visibility_toggle(visible: bool) -> impl IntoElement`
Eye icon, primary when visible, disabled color when hidden. Use in layer/track rows.

### `color_swatch(color: Rgba, size: f32) -> impl IntoElement`
Solid color square with border. Use for fill/stroke color pickers and swatches.

---

## Layout rules

All six apps share the same outer shell structure:

```
┌─────────────────────────────────────────────────┐
│  Toolbar  (height: 48px, surface_raised)         │
├──────┬──────────────────────────────┬────────────┤
│Tools │        Canvas / Viewer        │ Right dock │
│strip │        (fills remainder)      │  (220px)   │
│(40px)│                              │            │
├──────┴──────────────────────────────┴────────────┤
│  Timeline / transport (app-specific, ~180px)      │
└─────────────────────────────────────────────────┘
```

- **Toolbar**: `h(px(48))`, `bg(colors::surface_raised())`, horizontal flex. Contains app-specific menus, persona tabs, transport controls, zoom.
- **Tool strip**: `w(px(40))`, `bg(colors::surface_bg())`, vertical flex, icon buttons only.
- **Right dock**: `w(px(220))`, `bg(colors::surface_bg())`, vertical scroll. Houses all panels.
- **Canvas / viewer**: fills remaining space. No explicit size — flex grows.
- **Timeline** (Pulse/Reel): horizontal strip at bottom.

---

## Panel rules

Each panel follows this structure:

```rust
fn render_my_panel(app: &App, cx: &mut ViewContext<Root>) -> impl IntoElement {
    div()
        .flex_col()
        .child(panel_header("Panel Name"))
        .child(divider())
        .child(
            div()
                .flex_col()
                .p(px(spacing::MD))
                // ... panel content
        )
}
```

- Header via `panel_header()`, then `divider()`
- Content in a `flex_col` div with `p(px(spacing::MD))` padding
- Vertical scroll on the right dock container, not individual panels
- Section separators via `divider()`
- Interactive rows use `cx.listener` for mutations → `Action` → `App::apply`

---

## How to add a new panel

1. Create `crates/<app>-gpui/src/panels/my_panel.rs`
2. Implement: `pub fn render(app: &App, cx: &mut ViewContext<Root>) -> impl IntoElement`
3. Register in `panels/mod.rs`: `pub mod my_panel;`
4. Add toggle field to `App`: `pub show_my_panel: bool`
5. Add `Action::ToggleMyPanel` → `apply()` flips the flag
6. Add toggle button to toolbar
7. In `main.rs` layout: conditionally include `my_panel::render(app, cx)` in the right dock

---

## GPUI implementation patterns

### Canvas bounds recording (coordinate mapping)

```rust
let bounds_cell = Rc::new(Cell::new(None::<Bounds<Pixels>>));
let bounds_writer = bounds_cell.clone();

canvas(
    move |bounds, _cx| { bounds_writer.set(Some(bounds)); },
    |_, _| {}
)
.absolute().inset_0()
.on_mouse_down(MouseButton::Left, cx.listener(move |root, event, cx| {
    if let Some(b) = bounds_cell.get() {
        let doc_pos = window_to_doc(event.position, b, &root.app);
        root.app.apply(Action::BeginDrag(doc_pos));
        cx.notify();
    }
}))
```

### Animation loop (marching ants, playback)

```rust
fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
    cx.request_animation_frame();  // self-sustaining: re-renders next frame
    // ... rest of render
}
```

Only call `request_animation_frame()` when animation is actually running (check `app.is_playing` or `app.marquee_visible` first) to avoid burning CPU when idle.

### Mouse handler pattern

```rust
.on_mouse_down(MouseButton::Left, cx.listener(|root, event, cx| {
    root.app.apply(Action::BeginDrag(event.position));
    cx.notify();
}))
.on_mouse_move(cx.listener(|root, event, cx| {
    if root.app.drag_state.is_some() {
        root.app.apply(Action::ContinueDrag(event.position));
        cx.notify();
    }
}))
.on_mouse_up(MouseButton::Left, cx.listener(|root, event, cx| {
    root.app.apply(Action::EndDrag(event.position));
    cx.notify();
}))
```

### App::apply choke point

```rust
pub fn apply(&mut self, action: Action) {
    match action {
        Action::SetTool(t) => { self.active = t; }
        Action::BeginDrag(pos) => { /* ... */ self.host.mark_dirty(); }
        // every other action
    }
}
```

`mark_dirty()` triggers a recomposite on the next frame. Panels never write to `App` fields directly — always via `Action`.

---

## Engine isolation rule

Do not modify shared engine crates (`prism-canvas`, `prism-core`, `prism-color`, `prism-io`, `prism-media`) for GPUI-specific needs. The shared crates must remain UI-framework-agnostic.

---

## Sources

- [Affinity v2 UI redesign — Affinity Spotlight](https://affinityspotlight.com/article/redesigned-with-you-in-mind-the-all-new-affinity-v2-ui/)
- [Affinity Designer 2 — Interface guide (Envato Tuts+)](https://design.tutsplus.com/tutorials/affinity-designer-2-get-to-know-the-interface--cms-108766)
- [GPUI documentation — gpui.rs](https://www.gpui.rs/)
- [Zed source — reference GPUI usage](https://github.com/zed-industries/zed)
