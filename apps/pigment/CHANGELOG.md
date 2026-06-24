# Changelog

All notable changes to **Pigment** (the Prism suite's raster editor) are documented
here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project is pre-1.0, so versions are `0.x` milestones.

## [Unreleased]

## [0.12.0] - 2026-06-24

### Added — Multi-line + comprehensive text input
- **Runnable script editor** — floating multi-line `prism_ui::TextArea` for the rhai/line-DSL sandbox; Cmd+Enter / Run → `RunCurrentScript`, live script log.
- **New-document / image-size dialog** — typeable width/height/resolution → `NewDocument`/`SetImageSize`/`SetCanvasSize`/`SetPrintResolution`.
- **Typeable numerics** — brush size/hardness/opacity + free-transform rotation/skew (steppers kept as live display).
- +7 tests (452 → 459).

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Layer rename** — double-click a layer name → inline editable field → `RenameLayer`.
- **Hex color entry** — `#RGB`/`#RRGGBB` field (pure `parse_hex_color`) → `SetBrushColor`.
- **PSD export path** — typeable output-path field + native Browse dialog (`rfd`) → `SetPsdExportPath`/`ExportAsPsd`.
- **Editable text-tool content** — type real text onto the canvas via `SetTextContent` (re-rasterizes the active text layer live).
- +13 tests (439 → 452).

## [0.10.0] - 2026-06-24

### Added — UI (GPUI)
- **Preferences** floating window (Performance/Color/Interface/File-Handling) →
  `SetPref*` / `SavePreferences` / `LoadPreferences` / `ResetPreferences`.
- **Keyboard-Shortcuts editor** floating window — click-to-capture remap with
  conflict highlight → `RemapShortcut` / `UnbindShortcut` / `ResetShortcuts`.
- **Navigator panel + multi-doc tab bar** → `OpenDocTab` / `ActivateDocTab` /
  `CloseDocTab` / `NavigatorZoom` / `NavigatorPan`.
- **Guides + rulers** — guides painted over the canvas from guide state; View menu
  to add/clear/toggle guides, snapping, smart guides, ruler units.
- **Color menu** → `SetWorkingColorMode` / `AssignWorkingSpace` /
  `ConvertWorkingSpace` / `SetEmbedColorProfile`.
- All child windows use `WindowKind::Floating`; UI-only (emits existing 0.9.0
  actions, no new state).

## [0.9.0] - 2026-06-24

### Added — Feature waves 1–3 (parity push)

- **Smudge / Surface Blur / Path Blur / Smart Sharpen** — read-modify-write smudge
  brush, edge-preserving (bilateral) surface blur, directional path blur, and
  Smart-Sharpen wiring (`filters_extra.rs`, `app_state/filters_advanced.rs`).
- **Layer styles — Satin + Pattern Overlay bake** — destructive rasterization for
  the two remaining styles (`app_state/layer_styles_extra.rs`).
- **Free Transform — rotation + skew** — extends translate/scale with a composed
  scale·rotate·skew·translate matrix (`app_state/transform_extra.rs`).
- **Red-eye removal** — desaturate-toward-luma + darken on red-dominant pixels in a
  click radius (`app_state/redeye.rs`).
- **Smart Objects (non-destructive)** — embed a layer's pixels + a re-applicable
  transform and filter stack; update re-renders source→transform→filters into the
  layer; bake commits and drops the embed (`app_state/smart_objects_rich.rs`).
- **Actions / batch automation** — record actions into named sets and replay them,
  with a re-entrancy guard (`app_state/automation.rs`).
- **Scripting sandbox** — `rhai` engine exposing document/layer/selection query +
  mutate ops, plus a line-DSL fallback (`app_state/scripting.rs`).
- **Rich preferences** — Performance/Color/Interface/File-Handling panes with serde
  JSON load/save (`app_state/prefs.rs`).
- **Per-artboard export** — per-artboard format/scale/naming/quality metadata and a
  computed export plan (`app_state/artboards.rs`).
- **Native PSD serializer** — hand-rolled `.psd` writer (header, image resources,
  per-layer bounds/blend/opacity/name, RLE channel data, merged composite) with an
  RLE/raw compression toggle (`app_state/psd_export.rs`).
- **Guides / rulers / smart guides** — add/move/lock/clear guides, snap-to-guide,
  ruler units, and edge/center smart-guide alignment (`app_state/guides.rs`).
- **Color management** — RGB/Grayscale/CMYK/Lab working modes with RGB↔CMYK and
  RGB↔Lab conversion, assign/convert working space (`app_state/color_management.rs`).
- **Keyboard-shortcut remap** — command→chord map with Photoshop-like defaults,
  conflict detection, JSON load/save (`app_state/shortcuts.rs`).
- **Navigator + multi-doc tabs** — open-document tab list with active tab + dirty
  flag, navigator viewport pan/zoom (`app_state/navigator.rs`).

### Fixed

- **GPU sprite-atlas RAM leak.** RAM grew unbounded across a session and never released. gpui's sprite atlas only frees an image's GPU tile via an explicit `Window::drop_image`, and the canvas host produces a brand-new `RenderImage` (new image id) on every dirty composite — and with `request_animation_frame` the root can redraw continuously — so each redraw leaked one atlas tile. Fixed by holding the previously-painted image (`Pigment::last_image`) and calling `window.drop_image(prev)` whenever a new-id frame replaces it, bounding the atlas to ~one composite frame. An idle/cached frame returns the same image id, so its tile is kept.
- **Work-area overflow → dock truncation & top-bar overlap.** The center work-area column had no `overflow_hidden` and its `#canvas` child was a normal in-flow flex child sized to doc-pixels × zoom. With a large image / high zoom the oversized canvas (a) expanded the workspace row and shoved the right dock partly past the window's right edge (dock truncation), and (b) painted vertically over the top toolbar/menu bar (z-index). Fixed to match reel/contour: `.overflow_hidden()` on the center column + `.flex_shrink_0()` on `#canvas`, so the canvas is clipped to its own column and can neither push the dock off-screen nor paint over the top bar.
- **Color panel overflow.** Right-dock Color panel content (hue bars, hex/rgb readouts, R/G/B steppers, swatch & preset rows) overflowed the dock's right edge and got clipped. Root cause: flex rows with no width constraint sized to max-content, and `flex_1` text columns refused to shrink below content (`min-width: auto`). Fixed: `.w_full()` on `dockable_wrap` and all color-panel flex_row containers, `.min_w_0()` on the hex/rgb readout columns, `.flex_shrink_0()` on fixed-size swatches/labels/steppers, and `DOCK_W` 260→280.

### Batch 5 — Motion Blur, Twirl, Pinch/Spherize, Solarize, Glowing Edges (2026-06-19)

#### Added
- **Five new CPU raster filters** (`src/filters.rs`) — pure functions over the
  linear-premultiplied RGBA `f32` layer buffer, slotted into the existing
  `Action::ApplyFilter` dispatch via the read-layer → pure-fn → upload-layer
  pattern (identical to `lens_correction` / `content_aware`; no `prism-canvas`
  shader, no `prism-core`/`prism-color`/`prism-io` change). Each is region-undoable
  through the engine's normal layer-snapshot path and gated to the active paint
  target. New `Filter` variants: `MotionBlur { angle, distance }`,
  `Twirl { angle, radius }`, `Pinch { amount, radius }`,
  `Solarize { threshold }`, `GlowingEdges { width, intensity }`.
  - **Motion Blur** — directional smear: averages up to `distance` bilinear taps
    along a line at `angle` degrees, centred on each pixel (a directional box
    average; premultiplied averaging is correct so alpha edges stay clean).
  - **Twirl** — rotational swirl about the canvas centre with a smooth `(1−r)²`
    falloff to zero at `radius` (fraction of the half-diagonal); inverse-mapped
    bilinear resample.
  - **Pinch / Spherize** — signed radial remap about the centre
    (`amount > 0` pinches toward centre, `< 0` bulges); falloff to `radius`.
    Wired into the Filter Gallery as both **Pinch** (`amount 0.5`) and
    **Spherize** (`amount −0.5`).
  - **Solarize** — Sabattier tonal flip: inverts each RGB channel above
    `threshold`; operates on straight (unpremultiplied) color, repremultiplied
    on write.
  - **Glowing Edges** — 3×3 Sobel edge detection on straight luma rendered as a
    bright neon glow over black (`width` = sampling step, `intensity` = glow gain).
- **Filter Gallery wiring** — promoted the matching stub rows to live entries:
  Blur ▸ **Motion Blur**; Distort ▸ **Pinch**, **Spherize**, **Twirl**;
  Stylize ▸ **Glowing Edges**, **Solarize**.
- **Filter menu** — Motion Blur / Twirl / Pinch / Glowing Edges / Solarize added
  to the top-bar **Filter** dropdown (`panels/toolbar.rs`) for one-click access.
- **Tests** — 9 unit tests of the filter math in `src/filters.rs`
  (zero-distance / zero-angle / zero-amount identities, flat-field preservation,
  outside-radius pass-through, solarize threshold behaviour, glowing-edges
  black-on-flat + edge-detection).

### Batch 4 — Autosave, Healing Brush improvements, Print dialog, Plugin system, History panel (2026-06-19)

#### Added
- **Autosave** — `autosave_interval_secs` (default 300 s) and `last_autosave` on `App`.
  `maybe_autosave()` called each render frame; writes a minimal JSON stub to
  `~/.local/share/prism/pigment_autosave.json` via `dirs::data_local_dir()`.
  `Action::SetAutosaveInterval`, `TriggerAutosave`, `RestoreAutosave`, `DismissAutosave`.
  On startup, if the autosave file exists, a dismissable banner appears above the canvas
  (`autosave_restore_pending` flag).
- **Healing brush improvements** — `HealMode` enum (`Normal`, `Replace`, `Content`).
  `heal_radius: u32` (default 20) replaces brush size for the Heal tool.
  `heal_dab_at()` helper uses `heal_radius` and dispatches on `HealMode`:
  Normal → `heal_at` (Gaussian feather, hardness=0); Replace → `paint_clone_dabs`
  (clone-stamp, no blend); Content → delegates to `ContentAwareFill`.
  Tool-options bar updated: Heal arm shows radius stepper + mode buttons.
  `Action::SetHealRadius`, `Action::SetHealMode`.
- **Print dialog** (`panels/print.rs`) — Cmd+P opens a floating modal with paper size
  (A4/Letter/Legal/A3/Custom), orientation (Portrait/Landscape), scale
  (Fit to Page/100%/Custom %), and print color space (sRGB/AdobeRGB/Display P3).
  "Print" button composites the canvas to JPEG, writes a minimal hand-crafted PDF
  (single page, embedded JPEG XObject, valid `xref` table) to `/tmp/pigment_print.pdf`,
  and opens it with the OS viewer (`open` on macOS). No external PDF crate required.
  `Action::TogglePrintDialog`, `SetPrintPaperSize`, `SetPrintLandscape`,
  `SetPrintScaleMode`, `SetPrintColorSpace`, `DoPrint`.
- **Plugin system stub** (`plugin.rs`) — `Plugin` trait (`name()` + `apply(&mut LayerPixels, &Value)`),
  `PluginRegistry` with `register()` / `run()` / `names()`, built-in `InvertPlugin`
  (inverts RGB, preserves alpha). `panels/plugins.rs` lists plugins with per-row Run
  buttons and a params JSON display. `Action::RunPlugin { name, params }`,
  `Action::SetPluginParams`. Foundation for future WASM/dylib loading.
- **History panel upgrade** (`panels/history.rs`) — each row now emits `Action::UndoTo(idx)`
  (pops back `current − idx` engine undo steps + truncates `history_labels`).
  "+ Snapshot" button creates a named snapshot (`Action::CreateSnapshot`) stored as
  `snapshots: Vec<(String, String)>` (name + JSON layer summary). Snapshot rows shown
  below the undo list. Previous `JumpHistory` behaviour superseded by `UndoTo`.

### Batch 3 — Content-Aware Fill, Lens Correction, Perspective Warp, Camera Raw dialog (2026-06-19)

#### Added
- **Content-Aware Fill** (`Action::ContentAwareFill`, Shift+F5 intent) — CPU PatchMatch
  algorithm fills the active selection by finding and blending similar 8×8 patches from
  outside the masked region. 5 propagation+random-search rounds; top-3 patches blended
  by inverse distance. Regions >512×512 are downsampled, filled, then upsampled. Custom
  LCG replaces `rand` crate.
- **Lens Correction filter** (`Filter::LensCorrection { barrel, pincushion, vignette }`,
  `Action::ApplyLensCorrection`) — radial polynomial distortion
  `r' = r × (1 + barrel×r² + pincushion×r⁴)` with bilinear sampling; vignette darkens
  edges via `pixel × (1 − vignette×r²)`. Added as live entry in Filter Gallery → Distort.
- **Perspective Warp** (`Action::PerspectiveWarp { src_pts, dst_pts }`) — DLT homography
  (8 equations, 8 unknowns solved via partial-pivot Gaussian elimination); inverse warp
  with bilinear sampling.
- **Camera Raw Filter dialog** (`Action::ToggleCameraRawDialog`, Cmd+Shift+A) — expanded
  floating panel replacing the previous inline overlay. Three collapsible sections:
  Basic (Exposure, Contrast, Highlights, Shadows, Whites, Blacks), Detail (Sharpening
  Amount/Radius, Noise Reduction Luminance), HSL/Color (Hue/Sat/Luma × 8 colors = 24
  sliders, collapsed by default). Rendered from `panels/camera_raw.rs`.
- `ToggleCameraRawSection`, `SetLensParam`, `ToggleLensCorrection` actions for dialog
  sub-state.
- `camera_raw_section_basic/detail/hsl`, `lens_barrel/pincushion/vignette/correction_open`
  fields on `App`.

### Batch 2 — Filter pixel ops, CMYK soft proof, export presets, smart filter UX, histogram channels (2026-06-18)

#### Added
- **UnsharpMask filter** — `Filter::UnsharpMask { radius, amount, threshold }` variant;
  two-pass proxy (Gaussian blur pass then sharpen pass) applied via `apply_filter`.
  Wired as a live entry in the Filter Gallery's Blur category.
- **RadialBlur filter** — `Filter::RadialBlur { amount }` variant; Gaussian proxy
  (amount / 30.0 radius). Wired as live in Filter Gallery (was stub).
- **CMYK soft proof** — `SoftProofMode` enum (Off / Cmyk / PrinterProfile); stored on
  `App` and on `CanvasHost`. When active, `composite_and_bridge` applies a
  sRGB → CMYK → sRGB gamut-compression round-trip to the display output. Toggle via
  View → "Soft Proof: Off / CMYK". `SetSoftProof` action; `CanvasHost::set_soft_proof`.
- **Export presets** — `ExportFormat` (Jpeg/Png/Tiff/Webp), `ExportPreset` struct;
  `App::export_presets` defaults to JPEG 90% and PNG lossless. `ExportWithPreset(idx)`
  opens a native save-file dialog and encodes via `export_with_preset`. Two preset
  shortcuts added to File menu.
- **Smart filter sub-rows in Layers panel** — each layer with smart filters shows
  indented `↳ [filter name] ×` rows; clicking × dispatches `RemoveSmartFilter`.
  New "Add SF" footer button adds a default `Blur(2.0)` smart filter to the active layer.
- **Histogram channel mode** — `HistogramChannel` enum (Luminosity/Rgb/Red/Green/Blue);
  clicking the mode label in the Histogram panel cycles through modes via
  `SetHistogramChannel`. Bars render from `h.r/g/b/luma` per mode and are colored
  red/green/blue for individual channel modes, gray for Luminosity/RGB.

### Batch 1 — Multi-layer PSD write, RAW import, Filter Gallery panel, Saveable Preferences (2026-06-18)

#### Added
- **Multi-layer PSD write** — `CanvasHost::export_psd` upgraded to a full PSD v1
  layer-records section. Each non-adjustment layer gets its own record with name,
  blend mode key, opacity, per-channel raw data, and a correct extra-data Pascal
  string. Adjustment layers are skipped (no independent pixel data in PSD v1).
  The merged composite is still written as §2.5 so older readers see the flat image.
  `export_psd` now takes `&Document` in addition to the path.

- **RAW import stub** — `Action::ImportRaw(PathBuf)` + `Action::OpenImportRawDialog`
  added. Opens a file picker filtered to DNG/CR2/CR3/NEF/ARW/ORF/RW2/PEF/SRW/RAF.
  Fallback: `image::open` (handles DNG via TIFF decoder; proprietary RAW requires
  libraw/dcraw, which is not yet a dep — see blocker comment in handler). On success
  adds a new raster layer on top of the current document, resized to the canvas
  dimensions via Lanczos3 if the imported image differs in size.
  "Import RAW…" menu item added to File menu.

- **Filter Gallery panel** — `panels/filter_gallery.rs` replaces the inline filter
  gallery in `main.rs`. Shows all six Photoshop-standard categories (Artistic, Blur,
  Distort, Sketch, Stylize, Texture) with individual named effects. Effects backed by
  a live `Action::ApplyFilter` apply on click; effects without backing pixel ops show
  a "stub" badge. `ToggleFilterGallery` action added; Cmd+Shift+F keyboard shortcut
  wired in `main.rs`.

- **Saveable preferences** — `AppPrefs` struct (`window_width`, `window_height`,
  `last_document`, `recent_files`) serialized to
  `~/.config/prism/pigment_prefs.json` via `serde_json`. Loaded on startup from
  `App::new`; saved automatically when a document is opened via `Action::OpenImage`.
  `Action::SavePrefs` / `Action::LoadPrefs` for explicit control.
  `dirs` crate added to resolve the XDG config directory portably.

### UI cleanup (2026-06-18)
- Toolbar split into left (logo + personas + menus) and right (contextual tool options + undo/redo + readout + zoom). Removed open-PSD/save-as/export-PSD/reset-workspace toolbar buttons — all in File menu.

### Wave 16 — PSD export (2026-06-18)

#### Added
- **PSD export** — `CanvasHost::export_psd` writes a minimal valid PSD v1 (single merged layer, raw channel-planar, big-endian). No external dep. `Action::ExportPsd` now calls the real implementation; replaces the stub.

### Wave 15 — Full parity + egui removal (2026-06-18)

#### Added
- **Liquify warp mesh tool** — `Tool::Liquify`; bilinear displacement within a radius
  with smooth falloff; `CanvasHost::liquify_warp` applies the warp per drag frame.
- **Smart filters** — non-destructive `Vec<SmartFilter>` per layer (`Blur`, `Sharpen`,
  `Brightness`, `HueSat`); stored in `App::smart_filters`; `AddSmartFilter`,
  `RemoveSmartFilter`, `EditSmartFilter` actions.
- **Filter gallery** — modal overlay (deferred, priority 300) with 8 filter buttons:
  Gaussian Blur, Box Blur, Sharpen, Emboss, Find Edges, Posterize, Threshold, Add Noise.
- **Camera Raw** — modal overlay (deferred, priority 400) with Exposure/Highlights/
  Shadows/Whites/Blacks/Clarity/Vibrance/Saturation steppers; Apply maps Clarity to
  Sharpen/GaussianBlur via existing Filter path.
- **Color profiles / soft proofing** — `ColorProfile` enum (sRGB/AdobeRGB/P3/ProPhoto);
  `Action::SetColorProfile`; status badge shown for non-sRGB profiles; View menu entries.
- **Dockable/floating panels** — each dock panel gets a "⤢" detach button; `DetachPanel`/
  `AttachPanel`/`MovePanel` actions; detached panels render as floating shadows.
- **Filter and Camera Raw menu entries** — "Filter" menu now has Gallery/Camera Raw/
  Gaussian Blur/Sharpen/Add Noise; View menu has Color Profile submenu.

#### Removed
- **`pigment-app` (egui/eframe)** removed from workspace — `pigment-gpui` (GPUI) is now
  the sole binary.

#### Bug fixes (Wave 13–14 session)
- Menu bar dropdowns rendered behind canvas (fixed via `deferred().with_priority(100)`).
- Text cursor blink placement was offset (fixed `top(px(vy))` vs. `vy - 14.0`).
- Font dropdown scroll bubbled to parent inspector panel (fixed `on_scroll_wheel` consumer).
- Pigment text color change on brush color didn't repaint (fixed `update_text_layer` call).
- Pulse preview zoomed in on play (removed dual PREVIEW_CAP — single 640px cap).



### Changed
- **Wave 10: prism-ui token enforcement.** All text-label toolbar buttons
  replaced with `Icon` equivalents from `prism-ui`. Remaining raw `rgb()` panel colors
  replaced with `prism_ui::colors::*` tokens.
- **gpui upgraded to 0.2.2.** `prism_ui::init(cx)` called at startup.
  `Application::new()` used at startup. `window.focus` gains required `cx` arg.
  Rust 1.88+ required.

### Added
- **Layer masks and non-destructive adjustment layers.** Pigment's signature non-destructive workflow, all reusing
  the existing `prism-canvas` engine + `prism-core` layer/adjustment model (**no
  `prism-canvas`/`prism-core` change**):
  - **Layer masks.** A layer can now carry a raster mask. The layers-panel footer
    adds *Add Mask* (a white reveal-all mask) / *Delete Mask* and an *Edit Mask*
    toggle; masked rows show an "M" chip. In mask-edit mode, brush strokes paint
    the active layer's mask (white = reveal) and the eraser hides — routed through
    the engine's `paint_dabs(..., into_mask = true)`, routed through
    `paint_mask`. `CanvasHost` gained `set_mask` / `delete_mask` / `has_mask`
    forwarding the engine's per-layer mask API; the host tracks masked layers in
    `App::masked_layers` and the mode in `App::edit_mask`. New actions: `AddMask`,
    `DeleteMask`, `ToggleEditMask`.
  - **Adjustment layers.** The Adjustments browser rows now add a non-destructive
    adjustment *layer* (`doc.layers.add_adjustment`) affecting the layers below —
    Brightness/Contrast, Levels, Curves, Hue/Saturation, Color Balance, Vibrance,
    Black & White, Photo Filter, Channel Mixer, Posterize, Threshold, Invert,
    Gradient Map. `CanvasHost::sync_order` now encodes adjustment layers into the
    compositor `LayerDraw` (`Adjustment::encode` + the ChannelMixer matrix) and
    `sync_adjustment_luts` uploads the per-layer LUTs for the LUT-based kinds
    (Curves / Gradient Map / Color Balance), mirroring the egui app's
    `layer_order` + `sync_curve_luts`. The layers panel labels adjustment rows by
    kind and, under the active adjustment row, shows stepper editors for its
    scalar params (`SetAdjustment` re-uploads LUTs + re-composites; the engine
    owns all adjustment math). New actions: `AddAdjustment(AdjKind)`,
    `SetAdjustment`.
- **Selection overlay, more selection tools, and a tool-options bar.** Three parity items, all reusing the existing
  `prism-canvas` engine + shared `prism-core` selection ops (**no
  `prism-canvas`/`prism-core` change**):
  - **Marching-ants selection overlay.** The active selection's boundary now draws
    on the canvas as an animated dashed outline. `CanvasHost::selection_boundary`
    traces the engine's selection mask into unit pixel-edge segments (doc px); the
    root view caches them and re-traces only when the selection changes (a
    `selection_generation` counter, so the GPU mask isn't read back every frame).
    The canvas `canvas()` paint callback draws crawling black dashes along those
    edges, advancing the dash phase from a monotonic clock and calling
    `request_animation_frame` so the ants keep moving while a selection is active.
  - **Ellipse-marquee, Lasso, and Magic-Wand selection tools.** Joining the rect
    marquee, all route through the existing `begin_drag`/`continue_drag`/`end_drag`
    dispatch: Ellipse rasterizes via the engine `SelectionOp::Marquee { ellipse }`
    (or a CPU `shape_mask` when combining); Lasso collects a freehand polygon and
    fills it with `prism_core::raster::polygon_mask`; Magic-Wand composites and
    flood-selects with `prism_core::fill::flood_fill_mask` (the egui app's
    `do_magic_wand` path). Each result combines with the prior selection through
    `prism_core::raster::combine`, so **Shift adds, Alt subtracts, Shift+Alt
    intersects** (the modifiers flow in via `App::begin_drag(doc, alt, shift)`).
    `CanvasHost` gained `upload_selection_mask`, `read_selection_or_empty`, and
    `read_composite_f32`.
  - **Tool-options bar.** A new full-width strip under the toolbar
    (`panels/tool_options.rs`) shows the active tool's parameters as compact
    stepper rows — brush size/hardness/opacity (Brush/Eraser/Clone/Heal),
    tolerance + contiguous (Fill/Magic-Wand), gradient dither, text size.
    New actions: `SetFillTolerance`, `ToggleFillContiguous`, `ToggleGradientDither`,
    `SetTextSize`.
- **Undo/redo, Text, and Clone stamp.**
  Three parity-critical interactions, all reusing the
  existing `prism-canvas` engine (**no `prism-canvas`/`prism-core` change**):
  - **Undo / redo.** Every canvas mutation opens an undo step through the engine's
    region-COW command stack: brush/eraser strokes snapshot once per stroke
    (`paint_dabs(..., snapshot)`), and fill / gradient / shape / transform-bake /
    clone / text-raster each take a whole-layer `begin_command_now` snapshot before
    mutating; the engine's `apply_*` filters already snapshot internally. **Cmd+Z**
    / **Cmd+Shift+Z** (and **Cmd+Y**) undo/redo from the focused canvas via a root
    `on_key_down` handler, and the top bar gained **Undo (↶) / Redo (↷)** buttons
    that enable/disable from `history_labels()`. `CanvasHost` gained `undo`,
    `redo`, `history_labels`, and `snapshot_layer`; `App` gained `Action::Undo`/
    `Action::Redo`.
  - **Text tool.** Clicking with the Text tool places a raster text layer at the
    click point and opens an in-place edit; typing re-rasterizes the layer through
    `prism_io::text::render_text`, positioned at
    the click via a new host-local `shift_rgba_f32` (3 unit tests). Enter/Escape or
    a tool switch commits the run. (Bakes to raster.)
  - **Clone stamp.** Alt-click sets the source anchor; a drag locks the offset at
    the first dab and copies pixels from a frozen source snapshot along the stroke,
    via the engine's `snapshot_clone_source` + `paint_clone_dabs`. The Alt modifier
    flows in through `App::begin_drag(doc, alt)` from the mouse event.

  All three route through the existing `begin_drag`/`continue_drag`/`end_drag`
  dispatch seam (clone/text added as tool arms).
- **Fill / Gradient / Shape tools draw through the engine.** Three more core canvas
  tools now act on the document via the shared engine, routed through
  `App::begin_drag`/`continue_drag`/`end_drag`: **Fill (paint-bucket)** floods the
  active layer within tolerance (`prism_core::fill::flood_fill_mask`); **Gradient**
  lays a linear gradient source-over the active layer; **Shape (Rectangle / Ellipse)**
  rasterizes a filled, anti-aliased shape. **No raster math is reimplemented** —
  `prism_core::{fill,gradient,shape}` were already host-agnostic (**no
  `prism-canvas`/`prism-core` change needed**). `CanvasHost` gained `fill_at`,
  `apply_gradient`, `draw_shape`.
- **Selection / Move / Transform operate on the canvas.** All canvas pointer events
  route through `App::begin_drag`/`continue_drag`/`end_drag`: **Selection (rect
  marquee)** sets the engine selection via `SelectionOp::Marquee`; **Move**
  translates the active layer; **Transform** scales it. Move/Transform preview live
  via `set_layer_transform` and bake on release via `bake_transform`. `CanvasHost`
  gained `set_marquee`, `clear_selection`, `has_selection`, `set_layer_xform`,
  `bake_layer_xform`.
- **Brush paints on the canvas.** Mouse-driven **brush + eraser** strokes paint
  into the active layer through the `prism-canvas` engine and recomposite live.
  `CanvasHost` gained `paint_dabs` + `select_all`.
- **Full panel chrome.** `pigment-gpui` renders the complete pro-editor layout
  around the live composited canvas: top toolbar, left tools strip, interactive
  layers panel, color panel, adjustments/filters browser, and histogram panel. Each
  panel reads `&App` and mutates only through `App::apply(Action)`.

### Changed
- **Engine extracted to `prism-canvas`.** The host-agnostic GPU engine (`CanvasGpu`,
  compositor, `filter_math`, layer/brush/selection/clone/command/channels models +
  the 6 wgsl shaders) moved into the shared **`prism-canvas`** crate (in
  `prism-suite-prism`). The engine has **zero egui/eframe dependency**.
  `ViewTransform.pan` and `zoom_to` anchor migrated `egui::Vec2 → glam::Vec2`.
  Engine targets **wgpu 29**.

### Added
- **First real vertical slice.** `pigment-gpui` drives the real `prism-canvas` wgpu
  compositor through the readback bridge: uploads layers as RGBA16F, runs
  `composite_now`, reads back the Rgba16Float composite, converts
  linear-premultiplied → BGRA8 sRGB, and displays it as a GPUI `RenderImage`
  (dirty-cached). Includes a read-only **layers panel** and a **histogram panel**
  from live engine state. Engine gained `LayerDraw::basic(...)`.
- **GPUI host foundation.** `pigment-gpui` binary established: GPUI paints chrome +
  canvas background; `CanvasHost` owns a wgpu device and bridges a placeholder
  document. Bridge measured at **~2.1ms at 1360×1920, ~3.5ms at 2480×3508** —
  under 16ms budget. Requires the Metal Toolchain on macOS.
- **Filter · Blur · Field Blur** (Blur Gallery — multi-pin variable blur). A new
  destructive, undoable entry in the **Filter ▸ Blur** submenu that drives the
  per-pixel blur radius from a set of **pins**, each with a position and a blur
  amount (px). The radius at every pixel is **interpolated between the pins by
  inverse-distance-squared (IDW / Shepard, power 2) weighting**, then a local 2D
  Gaussian of that radius is run at the pixel — the same variable-radius kernel as
  Tilt-Shift / Iris Blur, but the radius field comes from pin interpolation rather
  than a band/ellipse. A **single pin is a uniform blur** everywhere at that pin's
  amount; each pin's amount is reproduced exactly at its own location and the field
  varies smoothly and monotonically in between; an empty pin list or all-zero
  amounts is an exact identity. Implemented as one GPU pass (filter shader **kind
  33**); up to three pins ride in the existing `cr0`/`cr1`/`cr2` uniform overflow
  slots (reused additively from Camera Raw: `xy` = uv position, `z` = amount px,
  `cr0.w` = pin count), so no uniform-layout or `prism-io` change was needed. The
  radius-field interpolation has a pure CPU reference
  (`canvas::filter_math::field_blur_radius_at` / `field_blur`) that the shader
  mirrors bit-for-bit (incl. the `1e-3` squared-distance floor for exact pin hits).
  The Blur menu exposes a pin count (1–3) and per-pin x / y / blur-px sliders.
  Tested: single pin → uniform radius; two pins → the field equals each pin's
  amount at its location and interpolates monotonically (midpoint = mean) between
  them; zero-amount / no-pins → identity; a nonzero pin softens the edge — 5 CPU
  unit tests + 2 GPU pixel tests (center-sharp/corner-blurred two-pin field, and
  single-pin uniform vs zero-amount identity; skip-on-no-adapter). App-local only
  (destructive one-shot, like Iris Blur); no prism crate change.

## [0.6.0] - 2026-06-17

### Added
- **Camera Raw filter** (Smart Filters — non-destructive develop). A new
  **re-editable smart-filter kind** ("Camera Raw") that applies the RAW develop
  controls to any raster layer as a single non-destructive, re-orderable entry in
  the layer's smart-filter stack: **white balance** (temp / tint), **exposure**,
  **contrast**, the four **tonal regions** (highlights / shadows / whites /
  blacks), **vibrance**, **saturation**, and a positional **vignette** — every
  control defaults to 0, which is an exact no-op. Implemented as one GPU pass
  (filter shader **kind 32**): white balance and exposure run in **linear light**,
  then contrast, the tonal regions (luma-weighted so hue is preserved), and
  vibrance/saturation run in **display (sRGB)** space so the pivots and tones land
  where the user sees them, then back to linear; the vignette is applied last as a
  center-pinned quadratic luma falloff to the corners. The eleven controls ride in
  an **additive** `params_ext` overflow slot on the serialized smart-filter entry
  (legacy four-param kinds and older documents serialize byte-for-byte
  unchanged). The per-pixel tone/WB math has a pure CPU reference
  (`app::camera_raw::develop_pixel`) that the shader mirrors bit-for-bit. Tested:
  identity is a no-op (per-channel); exposure brightens/darkens (~2× per EV at the
  midpoint); contrast pivots about mid-gray; highlights/shadows/whites/blacks each
  bite their tonal range; temperature warms R / cools B (green untouched); tint
  trades green↔magenta; saturation/vibrance behave (vibrance favours muted
  colours); the vignette darkens corners more than the center with the center
  pinned; and the controls round-trip through the smart-filter stack incl. the
  missing-overflow (legacy) case — 13 CPU unit tests + 1 GPU pixel test
  (skip-on-no-adapter). Additive `prism-io` field (`SmartFilterMeta.params_ext`);
  all four suite apps still build.
- **Filter · Blur · Iris Blur** (Blur Gallery — radial focus blur). The radial
  sibling of Tilt-Shift: a new destructive, undoable entry in the **Filter ▸
  Blur** submenu that keeps the image sharp inside an **elliptical** region at the
  canvas center and blurs progressively outside it (the classic Iris/lens focus
  look). Controls: **ellipse width / height** (px radii), **feather** (a
  normalized fraction of the ellipse radius — how far past the boundary the blur
  ramps to full), and **max blur** (px). The GPU pass (filter shader **kind 31**)
  computes a per-pixel focus weight from the normalized elliptical radius
  `e = sqrt((dx/rx)² + (dy/ry)²)` — `0` inside the ellipse (`e ≤ 1`), ramping
  smoothly to `1` once past the boundary by `feather` — and runs a local 2D
  Gaussian whose radius is `max blur × weight`, so inside-ellipse pixels are
  untouched. Mirrors the Tilt-Shift variable-radius blur with a radial mask
  instead of a band. The focus-weight falloff (`iris_weight`) and the full
  `iris_blur` are pure, testable CPU references bit-for-bit matching the shader.
  Tested: the weight is `0` inside the ellipse, `1` at full feather, monotonic in
  normalized radius; the center stays sharp while corners blur/bleed; zero max
  blur is identity (4 unit tests) + 1 GPU pixel test (skip-on-no-adapter).
  App-local (no shared-crate change).
- **Filter · Blur · Spin Blur** (Blur Gallery — rotational motion blur). A new
  entry in the **Filter ▸ Blur** submenu that smears the active layer along the
  tangential arc about the canvas center, the amount set by a **blur angle**
  (0–90°) with a **quality (samples)** control. Reuses the existing radial-blur
  **Spin** mode (filter shader **kind 6**, samples spread symmetrically so a flat
  image is unchanged) under a clean Blur Gallery name. Tested: rotational blur of
  a flat (constant) image is the identity, and an off-center bright pixel smears
  tangentially (not radially) — 1 GPU pixel test (skip-on-no-adapter), atop the
  existing radial-spin kernel unit tests. App-local (no shared-crate change).

## [0.5.0] - 2026-06-13

### Added
- **Edit · Pattern fill** (define a pattern from a selection, then tile-fill).
  Two new **Edit** menu entries: **Define Pattern** captures the active layer's
  pixels inside the current selection's bounding box (or the *whole layer* when
  there's no selection) as an in-memory **tile** — a soft/partial selection
  premultiplies the captured tile by its coverage; **Fill with Pattern** tiles
  that pattern across the active layer (clipped to the selection when one
  exists), source-over the existing pixels, with **scale** (0.05–8×, log slider)
  and **offset x/y** (doc px) controls. The fill reuses the destructive
  `begin_command → read layer → blend → upload` path (region-COW undo) shared
  with the gradient fill, working in linear premultiplied RGBA. The pattern tile
  is **session-scoped** (not persisted to `.pigment` — re-define after a reload).
  The tiling/coverage math is a pure, testable function (`pattern_texel`: dest
  pixel → pattern texel via scale + offset + Euclidean wrap) plus a pure
  `tile_fill` that blends the tiled pattern over a buffer. Tested: the texel map
  wraps beyond the tile size, applies offset and scale, and is total for a
  degenerate (zero) tile; a 2×2 checker tile reproduces the checker exactly over
  a 4×4 fill; the selection mask gates the fill (outside-selection pixels
  unchanged) and partial coverage blends proportionally. App-local (no
  shared-crate change).
- **Filter · Blur · Tilt-Shift** (Blur Gallery — graduated/positional blur). A
  new destructive, undoable entry in the **Filter ▸ Blur** submenu that keeps the
  image sharp inside a horizontal **focus band** and blurs progressively outside
  it, the classic miniature-faking / tilt-shift look. Controls: **focus center**
  (0–1 of canvas height), **band half-width** (px, fully sharp), **feather** (px,
  the ramp past the band), **max blur** (px), and a **tilt angle** (±45°) to lean
  the band. The GPU pass (filter shader **kind 30**) computes a per-pixel focus
  weight from the signed distance to the (optionally tilted) focus line — `0`
  inside the band, ramping smoothly to `1` once past `half-width + feather` — and
  runs a local 2D Gaussian whose radius is `max blur × weight`, so in-band pixels
  are returned untouched and out-of-band pixels blur more the further they sit
  from the band. Reuses the existing destructive `begin_command → filter pass →
  copy-back` (region-COW undo) pattern. Tested: a pure CPU reference of the
  focus-weight falloff (zero in the band, symmetric, monotonic, clamped to 1 past
  the feather) and of the full tilt-shift (in-band stays a hard edge, far-from-
  band blurs, max-radius 0 is identity), plus a GPU pixel test (skip-on-no-
  adapter) confirming the band stays sharp while a far row blurs. App-local (no
  shared-crate change).

## [0.4.0] - 2026-06-13

### Added
- **Smart Filters** (non-destructive, re-editable filter stack). A raster layer
  can now carry an ordered stack of **smart filters** that stay editable instead
  of being baked: the layer keeps its un-filtered **source** pixels, and the
  displayed/composited result is the source with the *enabled* filters applied in
  order, re-applied from the source whenever the stack changes. A **Smart
  Filters** section in the Properties panel lets you **add** (Gaussian Blur /
  Sharpen / Posterize), **remove**, **toggle**, **re-order** (apply earlier /
  later), and **edit the parameters** of each filter live — toggling a filter off
  or removing the last one restores the original pixels exactly. The pixel math
  **reuses the existing GPU filter passes** (the same shader kinds the destructive
  Filter menu uses — separable blur runs H then V), driven by a tidy GPU re-apply
  that resets the layer to a one-time source snapshot and runs the enabled passes
  into it (no source is ever overwritten, so edits always start from pristine
  pixels). The stack persists in the `.pigment` document (additive
  `LayerMeta.smart_filters` entries — `kind` + params + `enabled`; old documents
  with no stack load unchanged), saving each smart-filtered layer's **source**
  pixels so the filters re-apply cleanly on load. Tested: a pure model test suite
  (add/remove/reorder/toggle ordering, enabled-pass extraction, serde round-trip
  incl. legacy no-stack) plus GPU pixel tests (skip-on-no-adapter) — a smart
  Gaussian blur softens a hard edge, disabling it restores the hard edge, and
  editing a filter re-applies from the pristine source. The serialized stack
  lives in the shared `prism-io::document_file` (additive, byte-compatible — all
  four suite apps still build); the model, GPU re-apply, and UI are app-local.
  **Per-filter masks** are a documented follow-up.
- **Filter · Adjustments · Posterize / Threshold** (destructive tonal ops). The
  Filter menu gains an **Adjustments** submenu with two destructive, undoable
  bake-into-the-layer operations (the counterpart to the existing non-destructive
  Posterize/Threshold *adjustment layers*): **Posterize** quantizes each colour
  channel to *N* evenly spaced levels (a **levels** slider, 2–32; the engine
  accepts 2–255) and **Threshold** collapses the layer to pure black/white at a
  Rec.709 luma cutoff (a **threshold** slider, 0–1). Both quantize in **display
  (sRGB) space** so the steps/cutoff land where the user sees them — quantizing in
  linear light would bunch the levels in the shadows — unpremultiplying, encoding
  to sRGB, snapping, decoding back to linear and re-premultiplying, with alpha
  preserved. They run as single GPU filter passes (shader kinds 28/29 via
  `filter_pass_c`), destructive and undoable (region-COW) like the blur / distort
  / stylize / noise filters, and are mirrored by CPU references
  (`filter_math::posterize` / `::threshold`) sharing the sRGB transfer constants.
  Tested: CPU unit tests (Posterize at 2 levels snaps every channel to its 0/1
  extreme and quantizes a ramp to ≤N steps; Threshold splits a ramp at the cutoff
  into a strictly binary result, with the 0 / >1 extremes passing all / nothing)
  plus GPU pixel tests with the skip-on-no-adapter convention (same properties,
  alpha preserved, uniform per-pixel transfer). Implemented entirely app-local —
  no change to the shared `prism-core` `Adjustment` enum.
- **Stylize · Oil Paint** (Filter Gallery). A new entry in the **Stylize** filter
  menu applies a Kuwahara quadrant filter to the active layer: each pixel is
  replaced by the mean colour of the lowest-luma-variance quadrant of its
  `(2·radius+1)²` window (a **brush radius** slider, 1–8 px). Picking the flattest
  quadrant smooths interiors while snapping to one side of an edge, yielding the
  classic painterly patches with crisp boundaries — a true edge-preserving paint
  effect, not a blur. It runs as a single neighbourhood pass in
  linear-premultiplied space (GPU shader kind 27 via `filter_pass_c`), is
  destructive and undoable (region-COW) like the existing blur / distort /
  stylize / noise / pixelate filters, and is mirrored by a CPU reference
  (`filter_math::oil_paint`). Tested: CPU unit tests (flat field is identity; a
  hard black/white step snaps every pixel to a pure side, never a ~0.5 blend) and
  GPU pixel tests with the skip-on-no-adapter convention (same two properties).
- **Layer comps** (Layer power). The Layers panel gains a **Layer Comps** section
  where you can **Capture** a named snapshot of every layer's appearance —
  visibility, opacity, and blend mode — and later **Restore** it with one click,
  plus inline **rename** and **delete**. The capture/restore logic is a pure
  function over the layer list (`app::comps`), keyed by stable `LayerId`, so
  restoring is robust to layers being reordered, added, or removed since capture
  (added layers are left untouched; entries for removed layers are ignored).
  Position/transform is intentionally out of scope — Pigment layers carry no
  persistent position in their model (Move/Transform bakes into pixels), so a
  comp captures the appearance attributes that live on the layer. Comps persist
  in the `.pigment` document via a new additive `DocMeta.comps` field
  (`#[serde(default)]` + `skip_serializing_if`), so existing documents round-trip
  unchanged and comp-free documents stay byte-compatible. On load, saved layer
  ids are remapped to the freshly allocated ids. Tests cover capture→restore
  round-trips, restore reverting edits, robustness to add/remove/reorder, the
  runtime↔serde conversion with id remap, and the doc serde round-trip incl. the
  legacy (no-`comps`) case.

### Fixed
- **Default gradient background now shows on first load.** A freshly staged
  document (the startup default, plus any opened image/`.pigment`) came up as a
  blank/transparent canvas until the user's first edit. Root cause: the per-frame
  composite is recorded into egui's paint-callback (`prepare`) command encoder,
  which doesn't reliably execute while egui is **idle** immediately after load —
  so the uploaded layer pixels were never composited into the displayed buffer
  until an edit (which composites through a self-submitting path) forced it. (The
  layer texture was correct all along — verified by GPU read-back — but the
  composite output stayed transparent.) Staging a document now runs an initial
  composite through the **self-submitting** `composite_now` (its own encoder +
  `queue.submit`, the same path edit operations use) and builds the display bind
  group from it, so the document — including the sample gradient background — is
  presented on the very first frame. The pixel upload is also no longer consumed
  until the GPU state is actually ready (it retries next frame), so a not-yet-
  initialized render state can't silently drop the document. A headless test
  asserts the default background's upload buffer is a real, varying gradient.

## [0.3.0] - 2026-06-13

### Added
- **Font-family selection for Text layers** (Type richness). The Text layer
  panel gains a **font family** dropdown next to the text/size/color/align
  controls. It lists "Default" (the renderer's default sans-serif face) plus
  every family in the system font database, enumerated once via the shared
  `prism_io::text::available_families()` and cached (the DB scan is expensive).
  Picking a family sets the layer's `TextDef.family`, which changes the layer
  fingerprint and re-rasterizes the text in the chosen face through
  `render_text(..., family)`. Back-compatible: `family` defaults to `None`
  (`#[serde(default)]`), so existing text defs and `.pigment` documents
  round-trip unchanged. Tests cover default- and selected-family rasterization
  and the `TextDef` serde round-trip incl. the legacy (no-`family`) case.

### Fixed
- **Text layers keep their position when a property changes (font family, size,
  color, alignment).** Text/Vector layers carry no position in their definition
  — their pixels rasterize at the canvas origin — so a Move/Transform *bake*
  lived only in the layer's pixels. Any property edit re-rasterized at the origin
  and overwrote the whole texture, discarding the bake and snapping the text back
  to the top-left. The recently-added font-family change merely made the reset
  visible; the underlying defect was that re-rasterize ignored the layer's
  placement. The Move/Transform bake of a generated layer now records its
  accumulated translate (`gen_offset`, doc px), and `sync_generated_layers`
  re-applies that offset to the freshly-rasterized buffer (via the existing
  `reposition` helper) before upload — so *every* re-raster (family/size/color/
  align) preserves position, undoable as before. The placement is general (not
  font-family-specific) and needs no model change, so `.pigment` files round-trip
  unchanged. Tests: a pure unit test that a moved text layer keeps its position
  across a property/font change (and that a zero/absent offset is a no-op),
  exercising the exact buffer-placement seam without a GPU.
- **Move / Transform no longer snaps a layer back to its origin on release**
  (most visible with **Text** layers, which a user cannot reposition at all
  without this). The Move/Transform tools translate the active layer with a
  composite-time uv affine and *bake* it into the layer's pixels on pointer
  release. The drag-stop frame turns off `xform_active` *before* the frame's
  affine is computed, so the bake frame was sending `set_layer_transform(None)`
  — clearing the GPU's `xform_layer` so `bake_transform` returned early (a
  no-op) while the live preview affine was also dropped, snapping the layer back
  to where the drag started. The affine is now kept live for the bake frame too
  (`send_layer_xform(active, bake)`), so the move bakes and persists for every
  layer kind, undoable exactly as before. Tests: a pure unit test that the
  affine is still sent on the bake frame (and the translate→uv-offset mapping),
  plus a GPU regression that a baked move persists (gated on a GPU adapter,
  skip-on-no-adapter, so the default headless `cargo test` stays green). No
  model/serde change, so `.pigment` files round-trip unchanged.

## [0.2.0] - 2026-06-09

### Added
- **Render filter — Clouds & Difference Clouds** (Phase 8). A new pair of
  destructive *generator* filters wired through the exact existing filter pattern
  (GPU shader pass keyed by `kind` in `filter.wgsl` → `apply_clouds` on the
  compositor → `do_clouds` in the app → a new **Filter ▸ Render** submenu →
  tests), undoable (region-COW) like the existing blur / distort / stylize /
  noise / pixelate filters. Both paint the active layer with a deterministic
  multi-octave value-noise (fBm) field — a soft cloud texture — built on the same
  `hash21` lattice the noise/diffuse filters use, so it is **reproducible for a
  given seed** and reproduced bit-for-bit by the test-only `canvas::filter_math`
  CPU reference. **Clouds** (kind 25) fills the layer with the field (ignoring the
  source); **Difference Clouds** (kind 26) composites the field against the
  existing pixels via per-channel absolute difference (Photoshop-style), so
  repeated application folds the field and builds the characteristic veins. The
  fBm sums `octaves` value-noise layers (each doubling frequency, scaling
  amplitude by `roughness`), renormalised into `[0,1]`; controls expose **seed**,
  **scale** (base feature size px), **roughness** (per-octave falloff) and
  **octaves** under the new Render submenu. (Also tightened the shared CPU
  `hash21` to WGSL's floor-based `fract` so negative samples match the shader
  exactly.) Tests: the CPU reference module gains `value_noise` / `fbm` plus
  `clouds` / `difference_clouds` and **6 CPU unit tests** (determinism for a
  fixed seed; different seeds differ; output in range / opaque / gray; the field
  is spatially smooth, not white noise; difference-clouds = |base − noise|; the
  fold keeps transforming on repeat) — all pass under a normal `cargo test` — plus
  **1 GPU pixel test** gated on a GPU adapter (skip-on-no-adapter), so the default
  headless `cargo test` stays green. The new control fields are runtime UI state
  on the (non-serialized) app, so `.pigment` files round-trip unchanged.

## [0.1.0] - 2026-06-09

### Added
- **Sharpen filter — High Pass** (Phase 8). A new destructive filter wired
  through the exact existing filter pattern (GPU shader pass keyed by `kind` in
  `filter.wgsl` → `apply_high_pass` on the compositor → `do_high_pass` in the app
  → Filter ▸ Sharpen submenu → tests), undoable (region-COW) like the existing
  blur / distort / stylize / noise / pixelate filters, all in linear-premultiplied
  working space (the difference is taken on the unpremultiplied colour, then
  re-premultiplied, so a transparent edge doesn't bias it). **High Pass** (kind
  24) — the classic Photoshop sharpen prep: subtract a Gaussian-blurred copy from
  the original and re-centre at mid-gray, so flat areas go neutral gray (0.5) and
  only the high-frequency detail/edges survive as a signed deviation about 0.5.
  It reuses the existing separable Gaussian (kind 1, two passes) for the blur,
  saving the untouched source in the `ping` buffer, then a two-input combine pass
  (the filter bind group gains a back-compatible secondary texture at binding 3,
  aliased to the primary input for every single-input kind) subtracts the blur
  from the original. A `radius` (blur scale → coarser detail kept) and an
  `amount` (detail gain; 1 = identity high pass, 0 = flat mid-gray) control it,
  exposed under a new **Filter ▸ Sharpen** submenu (alongside the existing
  Sharpen). Tests: the test-only `canvas::filter_math` CPU reference module gains
  a separable `gaussian_blur` (matching the kind-1 shader weights bit-for-bit)
  and `high_pass` with **5 new deterministic unit tests** — Gaussian preserves a
  flat field, High Pass flattens locally-uniform areas to mid-gray while the edge
  carries detail, the two sides of an edge deviate in opposite directions about
  mid-gray (signed), amount 0 is a flat mid-gray field, and a larger amount
  scales the detail — plus **1 new headless-GPU pixel test**
  (`high_pass_flattens_flats_and_keeps_edges`) mirroring the existing GPU
  filter-test pattern (skip-on-no-adapter). App test count 109 → 115. No
  shared-crate changes (raster-only, pigment-app per PLAN §0a). *Still open
  (Sharpen family backlog):* Smart Sharpen, Unsharp Mask refinement.
- **Pixelate filters — Mosaic, Crystallize, Color Halftone, Mezzotint**
  (Phase 8). Four new destructive filters wired through the exact existing
  filter pattern (GPU shader pass keyed by `kind` in `filter.wgsl` → `apply_*`
  on the compositor → `do_*` in the app → Filter ▸ Pixelate menu → tests), all
  undoable (region-COW) like the existing blur / distort / stylize / noise
  filters, all in linear-premultiplied working space. The legacy **Pixelate**
  (kind 3, which point-samples each block's centre) is preserved and moved into
  the new submenu alongside the cell-based family.
  **Mosaic** (kind 20) — averages each `cell`×`cell` block to one colour (the
  true block mean, alpha-weighted in premultiplied space), so every pixel in a
  cell shares that average. **Crystallize** (kind 21) — Voronoi-like cells: each
  pixel snaps to the colour of its nearest jittered seed (one seed per
  `cell`×`cell` block, offset within the block by a hash of the block index +
  `seed`; the 3×3 block neighbourhood is searched so adjacent seeds can win),
  giving irregular polygons. It snaps to the seed's exact texel centre (a true
  snap to one source colour, never a blend) and is seeded-deterministic per the
  `diffuse` hash convention. **Color Halftone** (kind 22) — a per-channel dot
  screen: tile into `cell`-px cells rotated by a screen `angle` (with a 22.5°
  per-channel offset, CMY-rosette style); each cell's channel average sets a dot
  radius (darker channel → bigger dot of full ink, brighter → smaller), output
  binary per channel (full ink / paper). **Mezzotint** (kind 23) — a seeded
  threshold dither of Rec.709 luma against a per-pixel hashed threshold (biased
  by `amount`) to pure black/white grain, stable for a given seed. A new
  **Filter ▸ Pixelate** submenu hosts the legacy Pixelate plus the four with
  their controls (mosaic/crystallize cell size + crystallize seed; halftone cell
  + screen angle; mezzotint threshold + seed). Tests: the test-only
  `canvas::filter_math` CPU reference module gains the four filters (`mosaic`,
  `crystallize`, `color_halftone`, `mezzotint`) with **7 new deterministic unit
  tests** — mosaic cell = block average (each cell uniform = its mean) + flat
  identity, crystallize determinism (same seed ≡, different seed ≠) + cell
  snapping (output ⊂ source colours, never blended) + flat identity, color
  halftone dot grows as the cell darkens (more ink) + binary per channel,
  mezzotint binary + deterministic + brightness-tracking — plus **4 new
  headless-GPU pixel tests** (`mosaic_cell_is_uniform_block_average`,
  `crystallize_is_deterministic_and_snaps`,
  `color_halftone_dot_tracks_brightness`,
  `mezzotint_is_binary_and_tracks_brightness`) mirroring the existing GPU
  filter-test pattern. App test count 98 → 109. No shared-crate changes
  (raster-only, pigment-app per PLAN §0a). *Still open (Phase 8 pixelate
  backlog):* Fragment, Pointillize, selection-clipped pixelate, non-destructive
  smart-filter form.
- **Noise filters — Add Noise, Median, Dust & Scratches** (Phase 8). Three new
  destructive filters wired through the exact existing filter pattern (GPU
  shader pass keyed by `kind` in `filter.wgsl` → `apply_*` on the compositor →
  `do_*` in the app → Filter ▸ Noise menu → tests), all undoable (region-COW)
  like the existing blur / distort / stylize filters, all in
  linear-premultiplied working space (noise added to the unpremultiplied colour,
  then re-premultiplied, so a transparent edge doesn't bias the result).
  **Add Noise** (kind 17) — seeded-deterministic per-pixel noise, **gaussian**
  (Box–Muller) or **uniform** (a symmetric difference of two i.i.d. hashes so
  it's zero-mean: the raw `fract(sin)` hash is biased on a regular grid), with an
  `amount`, a **monochromatic** toggle (same noise on R/G/B), and a `seed`. It
  follows the `diffuse` hash philosophy exactly — stable for a given seed, no
  temporal randomness — and is **zero-mean so the channel average is preserved**.
  **Median** (kind 18) — per-channel median over a `(2·radius+1)²` window
  (despeckle / salt-pepper / impulse removal); radius param. **Dust &
  Scratches** (kind 19) — a thresholded median: a channel is replaced by the
  window median only when the original differs from it by more than the
  `threshold`, so specks are removed while sub-threshold detail is preserved.
  A new **Filter ▸ Noise** submenu hosts the three with their parameter
  controls (add-noise amount + gaussian/uniform + monochromatic + seed; median
  radius; dust threshold). Tests: the test-only `canvas::filter_math` CPU
  reference module gains the three filters (`add_noise`, `median`,
  `dust_and_scratches`) with **8 new deterministic unit tests** — add-noise
  amount-0 identity / determinism (same seed ≡, different seed ≠) / monochromatic
  equal-RGB-delta / mean-preserved (gaussian + uniform) + perturbation, median
  removes an impulse / radius grows the window, dust & scratches changes only
  above-threshold pixels / high-threshold identity — plus **3 new headless-GPU
  pixel tests** (`add_noise_is_deterministic_and_zero_mean` covering
  determinism + zero-mean + monochromatic R=G=B, `median_removes_an_impulse`,
  `dust_scratches_only_changes_above_threshold`) mirroring the existing GPU
  filter-test pattern. App test count 87 → 98. No shared-crate changes
  (raster-only, pigment-app per PLAN §0a). *Still open (Phase 8 noise backlog):*
  Reduce Noise, Despeckle, selection-clipped noise, non-destructive smart-filter
  form.
- **Distort filters — Twirl, Pinch/Spherize, Ripple/Wave, Polar Coordinates**
  (Phase 8). Four new destructive coordinate-displacement filters, wired through
  the exact existing filter pattern (GPU shader pass keyed by `kind` in
  `filter.wgsl` → `apply_distort` on the compositor → `do_*` in the app →
  Filter ▸ Distort menu → tests), all undoable (region-COW) like the existing
  blur / Gaussian / Sharpen / Pixelate filters. Each is a per-pixel
  coordinate-remap sampling filter (sample the source at a displaced coordinate,
  edge-clamp), working in pixel space about the canvas center.
  **Twirl** (kind 8) — rotates the image about the center by up to an `angle`,
  falling off quadratically to 0 at `radius` (untouched outside). **Pinch /
  Spherize** (kind 9) — a signed radial remap: positive pulls toward the center
  (pinch), negative pushes outward (spherize/bulge), with a smooth falloff to
  `radius`. **Ripple / Wave** (kind 10) — sinusoidal displacement where each axis
  is offset by a sine of the other, parameterised by `amplitude` (px) and
  `wavelength` (px). **Polar Coordinates** (kinds 11/12) — rectangular→polar
  (x = angle, y = radius) and the inverse polar→rectangular, an exact (modulo
  resampling) round-trip pair. A new **Filter ▸ Distort** submenu hosts the four
  with their parameter sliders (twirl angle + radius; pinch signed amount +
  radius; ripple amplitude + wavelength; polar rect↔polar toggle). All reuse the
  existing edge-clamped filter sampler and run in linear-premultiplied working
  space. Tests: the test-only `canvas::filter_math` CPU reference module gains
  the four remaps with **13 new deterministic unit tests** — twirl identity at
  angle 0 / outside-radius + center fixed / rotates a probe off its column /
  determinism, pinch identity at amount 0 / signed-opposite displacement /
  center fixed, ripple identity at amplitude 0 / wavelength periodicity /
  non-trivial warp, polar round-trip recovery + radius→rows mapping +
  determinism — plus **4 new headless-GPU pixel tests** (`twirl_rotates_within_
  radius`, `pinch_and_bulge_are_signed`, `ripple_is_periodic`,
  `polar_round_trips`) mirroring the existing GPU filter-test pattern. App test
  count 66 → 83. No shared-crate changes (raster-only, pigment-app per PLAN §0a).
  *Still open (Phase 8 distort backlog):* Warp (mesh + presets), Shear, Displace
  (displacement map), Lens Correction (`lensfun`), Adaptive Wide Angle,
  on-canvas center/handle UI, selection-clipped distort, non-destructive
  smart-filter form.
- **Blur family filters — Motion, Box, Radial** (Phase 8). Three new
  destructive blur filters, wired through the exact existing filter pattern
  (GPU shader pass keyed by `kind` in `filter.wgsl` → `apply_*` on the
  compositor → `do_*` in the app → Filter ▸ Blur menu → tests), all undoable
  (region-COW) like the existing Gaussian / Sharpen / Pixelate filters.
  **Motion Blur** (kind 4) — a directional/linear box average of `2·distance+1`
  taps along an `angle`, the classic streak blur (single pass). **Box Blur**
  (kind 5) — a flat, correctly-normalized kernel run separably (horizontal then
  vertical pass, reusing the Gaussian two-pass path) for a fast even blur.
  **Radial Blur** (kinds 6/7) — about the canvas center, in two modes:
  **Spin** (rotational, amount = degrees; smears tangentially) and **Zoom**
  (radial, amount = % ; smears toward/from the center), with a quality
  (sample-count) control; spin corrects for non-square pixels so the rotation
  is circular in pixel space. All operate in linear-premultiplied working space
  (averaging premultiplied samples is a correct blur) and reuse the existing
  edge-clamped filter sampler. A new **Filter ▸ Blur** submenu hosts the three
  with their parameter sliders (box radius; motion angle + distance; radial
  spin/zoom toggle + amount + samples). Tests: a new CPU reference module
  (`canvas::filter_math`, test-only) gives **12 deterministic unit tests** of
  the kernel math the shader implements — motion-blur axis-only smearing +
  energy conservation + radius-0 identity, box-blur separability/normalization
  (flat-field preserved, impulse → 3×3, two-axis equivalence, radius-0
  identity), radial spin-vs-zoom directionality + both-mode identity at amount 0
  + determinism — plus **3 new headless-GPU pixel tests** (`motion_blur_smears_
  along_angle`, `box_blur_normalizes_and_spreads`, `radial_spin_vs_zoom`)
  mirroring the existing GPU filter-test pattern. App test count 51 → 66.
  *Still open (Phase 8 blur backlog):* Surface Blur, Smart Blur, the Blur
  Gallery (Field/Iris/Tilt-Shift/Path/Lens), on-canvas radial-center handle,
  selection-clipped blur, non-destructive smart-filter form.
- **Gradient editor / gradient fill** (Phase 4 completion). The gradient tool is
  now a full multi-stop gradient editor: an independent **color rail** and
  **opacity rail** (Photoshop's two-rail model — add/remove/position color stops
  and opacity stops separately), all five **gradient geometries** (Linear,
  Radial, Angle, Reflected, Diamond), and **ordered dithering** to suppress 8-bit
  banding. A drag defines the gradient axis (`start→end`) which every geometry
  reinterprets (radial uses the drag length as the radius, diamond as the
  half-extent, angle as the reference direction, etc.); a **"fill layer"** toggle
  + button fills the whole layer across the canvas without dragging. Fills are
  clipped to the active selection and composited source-over onto the active
  layer (region-COW undo, like every other fill). Built-in **presets**
  (Foreground→Transparent, Black→White, Spectrum, Sunset) seed the editor. The
  gradient sampling/rasterization/dither math lives in the shared, app-agnostic
  `prism_core::gradient` (multi-stop interpolation in the working/linear space,
  premultiplied output matching `shape.rs`); the app converts the editor's sRGB
  stops to linear at fill time. Because the fill writes pixels directly (the
  established CPU fill path), gradient fills **persist to `.pigment`** as layer
  pixels with no format change; the shared `Gradient` type is also serde-ready
  (serialized round-trip tested in prism-io) so saved gradient presets/fills can
  be embedded later. Tests: 18 new `prism-core` unit tests (stop interpolation in
  the working space incl. multi-stop and unsorted stops, independent opacity
  rail, each geometry's parameterization, seeded/deterministic dither + presence
  + average-preservation, premultiplied render, id/zero-dim edge cases), 1 new
  prism-io serde round-trip, 3 new app editor tests (sRGB→linear conversion,
  preset load), and 1 new headless-GPU pixel test (read→render→upload gradient
  fill across a real f16 layer). *Still open:* on-canvas draggable stop handles,
  per-effect blend/reverse, noise gradients, `.grd` import.
- **Adjustment layers persist to `.pigment`** (Phase 7). Closes a known
  data-loss gap: adjustment layers were runtime-only — saving and reopening a
  document silently dropped every adjustment layer's parameters (and the
  adjustment layer itself reloaded as a blank raster layer). They now round-trip
  losslessly for **every** adjustment kind (Brightness/Contrast, Levels,
  Hue/Saturation, Exposure, Invert, Threshold, Black & White, Curves, Vibrance,
  Photo Filter, Posterize, Gradient Map, Color Balance, Channel Mixer — kinds up
  to 14). Follows the proven layer-styles persistence pattern: the `.pigment`
  doc model (`prism-io::document_file`) gains an optional per-layer `adjustment`
  payload (`Option<prism_core::Adjustment>`), reusing the shared `Adjustment`
  enum's own serde derive verbatim so the kind + every param (including Curves'
  variable-length per-channel control points and Channel Mixer's 3×4 matrix)
  serialize unchanged. The field uses `#[serde(default)]` + `skip_serializing_if`
  so **old documents (no `adjustment` key) still load** (as raster, as before)
  and non-adjustment layers stay byte-compact. On save, each
  `LayerKind::Adjustment` is written to `LayerMeta.adjustment`; on open, layers
  with that payload are rebuilt as adjustment layers and the existing recomposite
  path (`sync_curve_luts` / compositor params) rebuilds their LUTs/matrices so
  the restored adjustment renders immediately. Save→load round-trip unit-tested
  for Curves, Color Balance, and Channel Mixer (kind + every param), plus a
  raster back-compat case; prism-io adds a full-payload serde round-trip + an
  old-doc back-compat test. This was the last open item on the Phase-7
  adjustment work.
- **Adjustments: Color Balance + Channel Mixer** (Phase 7). Two more
  non-destructive adjustment layers, wired end-to-end through the established
  pattern (model variant in `prism-core::adjust` → composite-shader kind →
  inspector UI → GPU/unit tests). **Color Balance** (shader kind 13) applies a
  per-tonal-range RGB push — independent `cyan↔red / magenta↔green / yellow↔blue`
  sliders for Shadows / Midtones / Highlights, plus a *preserve luminosity*
  toggle. Because each output channel depends only on that same input channel, it
  rasterizes to a per-channel transfer LUT (reusing the Curves/Gradient-Map LUT
  texture + `curve_luts` slot), built CPU-side by `ColorBalanceLuts::build`
  (shadows weight darks, highlights weight lights, midtones a bell at 0.5).
  **Channel Mixer** (shader kind 14) computes each output channel as a linear mix
  of all input channels plus a constant (`[from_r, from_g, from_b, const]` per
  output), with a *monochrome* mode that collapses to a single weighted gray —
  output mixes all inputs, so it can't use a 1-D LUT and instead rides a small
  3-row matrix added to the compositor params (`CompositeParams` grew from 352 to
  400 bytes, still within the 512-byte `PARAMS_STRIDE` slot). New kinds appear
  automatically in the Add-Adjustment menu (it iterates `Adjustment::defaults()`).
  Tests: 7 new `prism-core` unit tests (LUT identity/shadow/highlight weighting,
  mixer swap/monochrome/clamp, encode kind+name stability) and 2 new
  headless-GPU pixel tests (shadow red-push lifts red; red↔blue mixer swap turns
  red into blue). *Still open:* Selective Color, multi-stop Gradient Map, Color
  Lookup (`.cube`/`.3dl` LUT), Shadows/Highlights, HDR Toning, Equalize,
  Replace/Match Color. (Adjustment params — including Color Balance / Channel
  Mixer — now persist to the `.pigment` doc; see the adjustment-persistence
  entry above.)
- **Layer styles persist to `.pigment`** (Phase 7). Closes a known data-loss
  gap: the 8 non-destructive layer styles (Stroke, Drop Shadow, Color Overlay,
  Inner Shadow, Outer Glow, Inner Glow, Gradient Overlay, Bevel & Emboss) were
  runtime-only — saving and reopening a document silently dropped them. They now
  round-trip. The `.pigment` doc model (`prism-io::document_file`) gains an
  optional, per-layer `styles` payload (`Option<LayerStyles>` with one optional
  struct per style, units documented: colors as straight RGBA/RGB, pixel
  offsets/sizes/blur in document px, angles in degrees), serialized with serde
  `default` + `skip_serializing_if` so **old documents (no `styles` key) still
  load** and **new documents with no styles stay byte-compact** (no empty keys).
  On save Pigment maps each layer's runtime style HashMaps into `LayerMeta.styles`;
  on open it re-installs them under the freshly-allocated layer ids and forces a
  recomposite so restored styles render immediately. Pure mapping functions
  (`runtime_styles_to_meta` / `meta_styles_to_runtime`) are unit-tested for a
  full 8-style lossless round-trip (GPU upload untested per convention); prism-io
  adds tests for a full-payload serde round-trip and old-doc back-compat.
- **Pen tool + work paths, path → selection, vector mask** (Phase 4 completion).
  A cubic-Bézier **pen**: click to drop corner anchors, click-drag to pull
  symmetric Bézier handles, and click the first anchor (within 8 screen px,
  ≥ 3 anchors) to **close** the path. A companion **Direct Select** tool grabs
  on-curve points (moves the whole anchor) or individual handles to reshape the
  curve after the fact. The work path renders as a vector **overlay** via the
  egui painter (flattened curve + anchor dots + handle lines/rings) — it is *not*
  part of the GPU composite. From the tool-options bar: **Path → selection**
  flattens the closed interior and fills it into the selection mask (reusing the
  selection pipeline, replace mode), and **Apply as vector mask** rasterizes the
  same interior into the active layer's mask via the existing layer-mask pipeline
  (`set_mask`). Bézier evaluation, polyline flattening, even-odd point-in-polygon
  and interior fill all live in-app (`path.rs`, no new shared-crate dep); nine
  unit tests cover curve evaluation, flatten-on-curve, smooth-handle mirroring,
  point-in-polygon, and exact fill-mask coverage on a known square (= the
  path→selection core, GPU-free). *Deferred:* shape layers from paths, boolean
  path ops, stroking a path with a brush, and persisting paths to the `.pigment`
  doc (doc-format change out of scope this pass).
- **Layer style: Bevel & Emboss (Inner Bevel)** (Phase 7). The last and hardest
  of the common Photoshop layer styles, evaluated live in the compositor with no
  separate height pass. A screen-space surface normal is derived from a central
  difference of the layer's alpha (height) field; a directional light (azimuth
  *angle* + *altitude*) shades it, painting a **highlight** where the surface
  faces the light and a **shadow** where it faces away, concentrated within *size*
  pixels of the edge (with optional *soften*). Per-layer highlight/shadow color +
  opacity, size, soften, angle, altitude controls; one GPU pixel test (highlight
  brighter on the light-facing edge, shadow darker on the opposite edge, nothing
  outside the shape). This rounds out the common PS layer-style set — only
  **Satin** and **Pattern Overlay** remain. (`CompositeParams` is now 352 bytes,
  still within the 512-byte `PARAMS_STRIDE` slot.)
- **Layer styles: Inner Shadow, Outer Glow, Inner Glow, Gradient Overlay**
  (Phase 7). Four more non-destructive layer FX evaluated live in the compositor,
  reusing the Drop-Shadow / Stroke alpha-neighborhood machinery. **Inner Shadow**
  casts a blurred, offset copy of the layer's *inverse* alpha clipped to its own
  coverage (dark band inside the edge). **Outer Glow** halos a centered, soft
  colored copy of the alpha outward; **Inner Glow** tints inward from the edge.
  **Gradient Overlay** recolors the layer's fill with an angled two-color linear
  gradient at adjustable opacity. Per-layer controls (color / offset / blur / size
  / angle / opacity as each needs); four GPU pixel tests. With Stroke, Drop
  Shadow, and Color Overlay this brings the common PS layer-style set near
  completion (Bevel & Emboss landed next). (`CompositeParams` outgrew the
  256-byte uniform slot at 288 bytes, so `PARAMS_STRIDE` is now 512, guarded by a
  compile-time size/alignment assert.)
- **Patch tool** (Phase 6 retouch). Lasso/freehand-select a region and drag it to
  a source area; on release a gradient-domain Poisson solve
  (`prism_core::heal::seamless_clone`) transplants the source's *texture* into the
  destination while tone-matching the region boundary — seamless, not a hard copy.
  PS-style **Source / Destination** mode toggle in the tool-options bar;
  selection-clipped, region-COW undo. Four unit tests (texture transplant,
  selection clipping, identity-offset no-op, mask translate/clip).
- **Layer style: Color Overlay** (Phase 7). Recolors a layer's covered pixels
  toward a chosen color by strength, evaluated live in the compositor. Per-layer
  color picker; GPU pixel-tested. Completes the Stroke/Drop-Shadow/Overlay trio.
- **Gradient Map adjustment** (Phase 7). Non-destructive — maps the backdrop's
  luminance through a two-color gradient (shadows→highlights) built into the
  per-layer LUT texture and sampled in the compositor (shader kind 12). Two color
  pickers; GPU pixel-tested.
- **Layer style: Drop Shadow** (Phase 7). Non-destructive — a blurred, offset,
  tinted copy of the layer's alpha drawn behind it, evaluated live in the
  compositor (16-tap disk blur). Per-layer color (premultiplied) + offset + blur;
  GPU pixel-tested. Reuses the stroke FX alpha-neighborhood machinery.
- **Layer style: Stroke** (Phase 7). Non-destructive outer stroke — an alpha-edge
  ring sampled live in the compositor shader, tinted and drawn behind the layer.
  Per-layer color + width sliders; GPU pixel-tested. (First layer FX; the
  alpha-neighborhood machinery generalizes to drop shadow / glow.)
- **Channels panel** (Phase 7). Save the current selection as a named alpha channel,
  load a channel back into the selection, or delete it (Channels panel section).
  GPU round-trip-tested.
- **Clipping masks** (Phase 7). A layer can clip to the layer directly below — its
  alpha gates where the layer shows, evaluated in the compositor via a clip-base
  texture binding. Per-layer "Clip to layer below" toggle; GPU pixel-tested.
- **Blend-If** (Phase 7). Per-layer "this layer" + "underlying" luma-range sliders
  (soft-feathered) that gate where the layer shows, evaluated in the compositor
  shader against the source and backdrop luma. GPU pixel-tested.
- **Adjustment expansion** (Phase 7). Three new non-destructive adjustment layers —
  **Vibrance** (saturation weighted to low-sat pixels), **Photo Filter**
  (luminosity-preserving warm/cool tint), **Posterize** (level quantize) — as
  compositor shader kinds 9/10/11, with sliders + a GPU pixel test.
- **Detail brush** (Phase 6). One brush, four modes — **Saturate / Desaturate**
  (sponge, `prism_core::tone::sponge`) and **Blur / Sharpen**
  (`prism_core::detail::blur_sharpen`) — applied over a soft brushed coverage mask
  on release. Unit-tested core math.
- **Liquify** (Phase 6). Mesh warp via a per-pixel displacement field with
  **Push / Twirl / Pucker / Bloat** modes (panel selector). Live preview
  re-warps a frozen snapshot each frame (no compounding blur);
  `prism_core::warp` provides the bilinear resample + brush stamps (unit-tested).
- **Dodge & Burn** (Phase 6 retouch). Brush to lighten (dodge), or hold Alt to
  darken (burn); a soft coverage mask accumulates over the stroke and is applied
  in linear light on release (`prism_core::tone::dodge_burn`, unit-tested).
- **Content-Aware Fill** (Phase 6 retouch). Brush a region; on release
  `prism_core::inpaint::content_aware_fill` synthesizes it from the surrounding
  texture via PatchMatch (approximate-NNF propagation + random search + patch
  voting, deterministic). Better than translate-and-blend for textured fills /
  larger removals. Unit-tested (uniform fill, content-awareness, determinism).
- **Spot Healing** (Phase 6 retouch). Brush over a blemish — **no manual source**;
  on release `prism_core::heal::spot_heal` auto-finds a clean nearby source by
  scoring boundary-ring match across candidate translations, then gradient-domain
  blends it in. Unit-tested (blemish removal, empty-mask no-op).
- **Healing Brush** (Phase 6 retouch). Alt-click sets a source; brush over the area
  to repair; on release a gradient-domain Poisson solve transplants the source's
  *texture* while matching the destination's tone/color at the region boundary —
  seamless repair, not a hard-edged copy. Solver lives in the shared core
  (`prism_core::heal::seamless_clone`, Gauss–Seidel membrane), unit-tested for
  tone-matching and texture transfer.

### Fixed
- Healing Brush: the Poisson guidance read source gradients at the region
  boundary from an unfilled (zero) source buffer, causing tone overshoot. The
  source is now built over the full image (offset-shifted, edge-clamped).

### Added
- **Clone Stamp tool** (Phase 6 retouch). Alt-click sets a source anchor; dragging
  stamps pixels copied from a frozen pre-stroke snapshot at a locked (aligned)
  offset, through a dedicated GPU clone-dab pass (`clone.wgsl`). Soft brush +
  opacity, selection-clipped, region-COW undo, on-canvas source crosshair.
  Pixel-verified by a headless-GPU test.
- **Curves adjustment** (completes Phase 4). Draggable monotone-cubic tone-curve
  editor with a composite (master) curve **plus** per-channel R/G/B; built to a
  256×1 LUT uploaded as a texture and sampled in the compositor (master then
  per-channel). Add/move/delete knots, pinned endpoints. Pixel-verified.

## [0.0.1] - 2026-06-06

First end-to-end raster editor on a GPU, linear-light, non-destructive engine.
Phases 0–5 of [PLAN.md](./PLAN.md), plus the suite's shared crates and first
cross-app interop.

### Added
- **Phase 0 — GPU canvas.** wgpu 29 shell; document textured-quad render
  with cursor-anchored pan/zoom, HiDPI; checkerboard transparency; open
  PNG/JPEG/etc; fit/100%.
- **Phase 1 — tiles, layers, paint.** Ping-pong compositor (Rgba16Float
  linear-premultiplied), blend modes; layers panel (add/delete/reorder/rename/
  visibility/opacity/blend/opacity); brush engine with arc-length dab walker,
  **wet-layer** stroke separation, velocity→size dynamics; eraser; bucket fill +
  eyedropper (sample-all-layers); undo/redo with a History panel; **region-COW
  undo**; frame-level dirty compositing; `.pigment` save/load (lz4 + JSON).
- **Phase 2 — selection & transform.** Marquee/ellipse/lasso/magic-wand with
  marching ants; feather/grow/shrink/invert + add/subtract/intersect; move +
  transform (translate/scale) with bake; crop, canvas/image resize (Lanczos),
  flip; copy/cut/paste + layer-from-selection; phosphor-icon dark-theme UI.
- **Phase 3 — adjustments, masks, filters.** Non-destructive adjustment layers
  (Brightness/Contrast, Levels, Hue/Saturation, Exposure, Invert, Threshold,
  Black&White); layer masks (paint reveal/hide); Gaussian blur / sharpen /
  pixelate; all 18 blend modes incl. HSL non-separable; histogram panel.
- **Phase 4 — text, vector, gradient.** Editable text layers (cosmic-text);
  rectangle/ellipse vector shape layers; linear gradient tool; generated layers
  stay editable.
- **Phase 5 — interchange.** PSD import (layers/opacity/blend/visibility);
  EXR/HDR open; export to PNG/JPEG/WebP/TIFF/BMP.
- **Suite interop.** Place a Contour `.contour` artboard as a rasterized layer;
  **live Dynamic-Link** — linked `.contour` layers re-render when the source
  file changes.
- **Shared engine.** Depend on suite-level `prism-core` / `prism-color` /
  `prism-io` crates (was app-local `pigment-core`/`-io`).

### Testing / CI
- Core unit tests (color/blend/tile/fill/raster/curve/histogram/shape), IO
  round-trips, and **headless-GPU pixel assertions** (compositor, wet brush,
  region undo, selection clip, transform bake, adjustment, layer mask).
- CI: `fmt --check` + `clippy -D warnings` + `test` on Linux/macOS/Windows.
