# Changelog

All notable changes to **Contour** (the Prism suite's vector editor) are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2026-06-19

### Fixed

- **GPU sprite-atlas RAM leak (canvas redraws).** RAM grew unbounded over an editing session and never released. Root cause: gpui's sprite atlas only frees an image's GPU tile via an explicit `Window::drop_image`, and the canvas host (`canvas_host.rs`) produces a brand-new `RenderImage` (new image id) on every dirty re-rasterize — so every pan/zoom/edit/notify redraw leaked one atlas tile, unbounded. Fixed by holding the previously-painted image (`Contour::last_image`) and calling `window.drop_image(prev)` whenever a new-id frame replaces it (skipped when the host returns the same cached image so an unchanged redraw keeps its still-painted tile), bounding the atlas to ~one preview frame.
- **Toolbar overflow / right-edge bleed.** Top toolbar was a single non-wrapping flex row of ~30 fixed-width buttons whose total width exceeded the window; trailing accent-colored align-to-artboard buttons painted past the right edge (purple sliver). Fixed: split into a scrollable `left` group (`flex_1` + `min_w(0)` + `overflow_x_scroll`) and a pinned `right` status group (`flex_shrink_0`); outer row `overflow_hidden` clips the bleed; `flex_shrink_0` added to separators, labels, and all icon-button helpers so buttons keep their 28px shape.

### Added — Batch 5

- **Type on a Path — real arc-length rendering.** Finished the Batch 3 `text_on_path` stub. `contour_app::text_on_path::layout_on_path(params, &BezPath, closed, TextOnPathParams)` lays the flat glyph run out, takes each glyph contour's horizontal centre as its arc-length distance *s* along the spine (`kurbo` `PathSeg::arclen` / `inv_arclen` / `deriv`), rigid-transforms the glyph about its baseline pivot to ride the tangent, and shifts it by the path normal — baking the warped glyphs into the text object's existing `glyphs` cache so **canvas + SVG + PNG render it with zero per-surface work**. `TextOnPathParams { offset, start, flip }` is a serde-default struct; `spine_bezpath(&Shape)` resolves the spine from any path/compound. New `App::text_on_path_params` map + `App::relayout_text_on_path(text_id)` re-bake on attach / param-edit / detach; `Action::SetTextOnPathParams { text_id, params }`. `AttachTextToPath` / `PlaceTextOnPath` now checkpoint + bake; `DetachTextFromPath` re-lays-out flat. Inspector "Path Text" section grows **Offset / Start steppers + Flip toggle** when attached. Glyphs that run off an open path are dropped (Illustrator behaviour); a closed spine wraps. 4 engine tests + 1 host integration test.
- **Perspective Distort — real homography.** Finished the Batch 3 `perspective` stub. `homography(&Corners)` now solves the closed-form unit-square→quad projective map (Heckbert), with `project` / `warp_points` / `warp_handles` (handles warped as endpoint-deltas so bezier curvature survives). New `Action::ApplyPerspectiveDistort { shape_id }` reduces the shape via `to_path`, pushes every anchor + out-handle of each contour through the homography, and replaces the shape with the distorted `Path` / `Compound` (one undo step). Uses the on-canvas corners when a distort is being edited, else a default top-narrowing trapezoid (one-click "Persp▶" toolbar button). `warp_shape_perspective` host helper covers Path + Compound. 4 engine tests + 1 host test.
- **Variable-width stroke outline (width profiles).** Closes the long-open Phase 3 "width profiles" item. `contour_app::pathedit::variable_width_outline(centreline, half_start, half_end)` walks a flattened centreline by **arc-length**, interpolates the half-width per vertex, offsets each side along the local normal, and joins them into one tapered filled band (butt caps). New `Action::OutlineWidthProfile(shape_id)` reads the stroke's existing `StrokeStyle::width_profile` (start/end multipliers, already settable via the Batch 2 Width presets / Width tool), flattens the editable centreline (works on **open** paths/lines where taper matters), and inserts the band as a filled path above the source. Toolbar "Width▶" button. 3 engine tests + 1 host test.
- **Roughen distort (Object ▸ Path ▸ Roughen).** New `contour_app::pathedit::roughen(pts, closed, size, detail)` — resamples the contour with `detail` inserts per segment then jitters every vertex within a disc of radius `size` using a deterministic index hash (reproducible / undo-friendly, no RNG state). New `Action::RoughenPath { size, detail }` demotes each selected shape to a corner path and roughens it (one undo step). Toolbar "Roughen" button (size 6, detail 2). 3 engine tests + 1 host test.
- **Gradient Mesh object.** New `contour_app::mesh_gradient::seed_grid(bbox, base)` produces a 4×4 control grid (edge nodes = fill colour, interior nodes brightened halfway to white — Illustrator's Create-Gradient-Mesh default). `Action::MakeMeshGradient` seeds it from the selected shape's bounds + fill into `App::mesh_points` (canvas-pixel space, consumed by the existing Wave-15 `render_mesh_overlay` compositor); `Action::ClearMeshGradient` removes it. Toolbar "Mesh" button toggles make/clear; nodes remain editable via the existing `EditMeshPoint`. 2 engine tests + 1 host test.
- **Test-build repair (carried-over Batch 4 fields).** Added the missing additive `envelope_mesh: None` (55 sites) and `font_axes: Default::default()` (5 sites) to `Shape` / `TextParams` struct literals across `document/tests.rs`, `export.rs`, `eyedropper.rs`, `blend.rs`, `boolean.rs`, `symbols.rs`, `clipboard.rs`, `history.rs`, `recolor.rs`, `shapebuilder.rs`, so `cargo test -p contour-app` compiles again (was broken by Batch 4's additive-field rollout). No logic touched. Full suite: **559 tests green** (506 contour-app + 53 contour-gpui).

### Added — Batch 4

- **Envelope Distort.** New `Tool::Envelope` variant (glyph `E`). `contour_app::envelope::EnvelopeMesh` (rows×cols bilinear-interpolation grid with `warp(u, v)`) stored per-shape via `#[serde(default)] envelope_mesh: Option<EnvelopeMesh>` on all 6 Shape variants. New actions: `MakeEnvelopeWithMesh { shape_id, rows, cols }`, `MoveEnvelopePoint { shape_id, idx, pos }`, `ReleaseEnvelope { shape_id }`. Shape getters: `envelope_mesh()`, `envelope_mesh_mut()`, `set_envelope_mesh()`.
- **Outer Glow live effect.** `Effect::Glow { radius, color, opacity }` added to the appearance stack. `effects.rs` composites a blurred alpha-spread glow behind the shape. `export.rs` emits `feDropShadow dx=0 dy=0` in SVG. `appearance.rs` adds `label()`, `is_active()`, `bounds_pad()`, and `Effect::glow()` constructor. Inspector wired via `AppearanceAddGlow`, `ToggleEffect`, `RemoveEffect`, `RemoveEffect`, `ReorderEffect`.
- **Group blend modes.** `App::group_blend_modes: HashMap<u64, BlendMode>` and `App::group_opacities: HashMap<u64, f32>`. Actions: `SetGroupBlendMode { group_id, mode }`, `SetGroupOpacity { group_id, opacity }`.
- **Pathfinder improvements.** `OutlineStroke { shape_id }` converts a stroked path to its outline compound. `ExpandAppearance { shape_id }` stub. Toolbar now shows Outline and Expand buttons.
- **Graphic Styles panel.** `panels/graphic_styles.rs` grid panel with swatch preview, style name, "New Style" button, click-to-apply. Seeded 5 default styles in `App::new()`: Bold Stroke, Drop Shadow, Neon Glow, Woodcut, Wireframe.

### Added — Batch 3

- **Live Paint Bucket.** New `Tool::LivePaint` variant. `Action::ApplyLivePaint { fill }` sets the fill of the topmost selected closed shape to the given color, enabling click-to-fill workflow. Tool icon wired in `tools.rs`.
- **Type on Path.** New `Action::PlaceTextOnPath` finds one text + one path shape in the current selection and attaches them via the existing `text_on_path` map (extends the Wave 9 `AttachTextToPath` plumbing). New `contour_app::text_on_path` stub module defines `TextOnPathParams` for future arc-length rendering.
- **Perspective Distort.** New `Tool::PerspectiveDistort` variant. `Action::SetPerspectiveDistort { shape_id, corners }` stores 4-corner state on `App` (`perspective_distort_corners`, `perspective_distort_active`). `Action::ConfirmPerspectiveDistort` logs the stub. New `contour_app::perspective` module provides `homography()` and `warp_pixels()` stubs for future full DLT implementation.
- **Recolor Artwork — HSL sliders.** `contour_app::recolor::shift_hsl(color, hue_shift, saturation_scale, brightness_scale)` public function added. `Action::RecolorArtworkHSL { hue_shift, saturation_scale, brightness_scale }` applies HSL adjustments to all selected shapes' fills and strokes in one undo step. New `App` fields `recolor_hue_shift`, `recolor_sat_scale`, `recolor_bright_scale`, `recolor_hsl_open`. New `Action::OpenRecolorHSLPanel` / `CloseRecolorHSLPanel`.
- **Distribute & Align improvements.** Toolbar now shows HorizontalGap and VerticalGap distribute buttons (using existing `Distribute::HorizontalGap` / `VerticalGap` variants). New `Action::AlignToArtboard { alignment }` aligns the selection relative to the active artboard bounds (falls back to the first artboard). Six align-to-artboard buttons added to the toolbar with accent background to distinguish them from the selection-bounds align row.

### Added — Batch 2

- **Symbol library (real).** `App.symbol_lib: contour_app::symbols::Symbols` replaces the old stub `Vec<Symbol>`. New actions: `DefineSymbol(name)` snapshots selected shapes into a master; `PlaceSymbol2(id)` resolves the master through a translate transform and inserts the shapes; `EditSymbol2(id)` logs a stub; `DeleteSymbol(id)` removes master + all instances. `panels/symbols.rs` rewritten to use `symbol_lib.list` and `symbol_lib.instances`.
- **Variable font axes.** `TextParams` gains `font_axes: HashMap<String, f32>` (`#[serde(default)]` for backward compat). `Action::SetFontAxis { axis, value }` mutates it via `edit_text_params`. Character panel now shows a "Variable Font" section with wght / wdth / slnt steppers.
- **Named artboards.** New `ArtboardEntry { id, name, rect }` struct and `App.artboard_entries: Vec<ArtboardEntry>` alongside the legacy `artboards` vec. `App.active_artboard: Option<u64>` tracks selection. New actions: `AddArtboard2`, `RemoveArtboard`, `RenameArtboard`, `SelectArtboard`, `DuplicateArtboard`. Legacy `AddArtboard` now also registers in `artboard_entries`. Canvas overlay uses named entries when present (active artboard shown in accent color), falls back to legacy vec. New `panels/artboards.rs` panel wired into right dock.
- **Graph / chart tool stub.** New `Tool::Graph` variant added to the tools enum (now `[Tool; 16]`). `GraphKind { Bar, Line, Pie, Scatter }` enum. `Action::InsertGraph { kind, rect }` generates shapes from hardcoded sample data `[[10.0, 20.0, 15.0], [5.0, 25.0, 30.0]]`. `tools.rs` icon mapping updated.
- **Stroke width profile presets.** `WidthProfilePreset { Uniform, TaperIn, TaperOut, BulgeCenter, TaperBoth }` enum with `profile() -> (f32, f32)`. `Action::ApplyWidthPreset(preset)` writes to `shape.stroke_style_mut().width_profile`. Inspector now shows a "Width Presets" section with five preset buttons.

### Added — Wave 15

- **Mesh gradient renderer.** New `contour_app::mesh_gradient` module rasterizes a 4×4 bilinear-interpolated control-point grid into an RGBA8 overlay. `CanvasHost::image()` now accepts `mesh_points` and composites the overlay onto the base raster via source-over alpha-blending when 16 control points are present. `App::mesh_points` (set via `SetMeshGradient` / `EditMeshPoint`) is now live-rendered on every frame.
- **Shape Builder canvas interaction.** New `Drag::ShapeBuilderDrag` and `Action::ApplyShapeBuilder { subtract }`. With the Shape Builder tool active, each mouse-down accumulates the clicked shape into the multi-selection via `AddToSelection`; mouse-up triggers `ApplyShapeBuilder` which calls `shapebuilder::build_faces` + `apply_build` (Unite or Subtract mode based on Alt/Option modifier) and replaces the selected shapes with the result. Wire-up closes the stub gap left by `MergeRegion` / `SubtractRegion`.
- **Appearance reorder.** `AppearanceRaiseFill`, `AppearanceLowerFill`, `AppearanceRaiseStroke`, `AppearanceLowerStroke` actions were already wired in `apply()` via `Appearance::raise_fill` / `lower_fill` / `raise_stroke` / `lower_stroke`. Confirmed complete — no further work needed.

### Changed — UI cleanup
- Toolbar stripped to essentials: app name, undo/redo, pathfinder, align/distribute, isolation badge, zoom. Removed export/import/recolor/image-trace/perspective-grid buttons (all accessible via menus).

### Added — Wave 14

- **Width tool.** New `Tool::Width` + `Drag::WidthDrag`. Horizontal drag on canvas maps ±200 px to a `width_profile (start, end)` multiplier on the selected shape's `StrokeStyle`. `SetWidthProfile` action persisted. Keyboard shortcut: none yet (tool panel button added).
- **Blend tool.** New `Tool::Blend`. Click-select selects blend operand; `CreateBlend { steps }` / `SetBlendSteps` actions drive `blend::make_steps`. Inspector exposes a step-count stepper.
- **Perspective grid.** `PerspectiveGrid` struct (`vp1`, `vp2`, `horizon_y`, `visible`). `TogglePerspectiveGrid` action toggles visibility; `MovePerspectiveVP` action repositions vanishing points. Toolbar button added. Canvas overlay renders horizon line + VP dots.
- **3D Extrude effect.** `Effect::Extrude3D { depth, angle_deg, color }` added to the appearance stack. `export.rs` renders it as an SVG feDropShadow-0. `effects.rs` renders an offset silhouette behind the artwork using `shadow_layer`. Inspector exposes depth / angle / color controls.
- **AI / EPS import.** `contour_app::ai_eps::import` parses text-format PostScript (CS4 and earlier). Handles `m M l L c C h H closepath b B f F S s rg RG g G w`. Binary AI files return a helpful error. Toolbar "AI/EPS" button wires `Action::ImportAiEps(PathBuf)`.
- **Mesh gradient model.** `SetMeshGradient { points }` and `EditMeshPoint { row, col, pos, color }` actions store a `Vec<((f32,f32),[f32;4])>` of control points on `App`. Renderer TODO.

### Removed — Wave 14 (egui strip)

- **Old eframe/egui binary removed.** `crates/contour-app/src/{main.rs,app/,canvas.rs,icons.rs,theme.rs}` deleted. `eframe`, `egui`, and `egui-phosphor` removed from `contour-app/Cargo.toml`. `contour-gpui` is now the sole Contour binary.

### Added — Wave 12

- **On-canvas transform handles.** Eight scale handles (squares) appear around the
  selection bounding box when the Select tool is active. Drag any handle to scale
  all selected shapes; hold Shift for uniform scale. Implemented via snapshot-restore
  pattern (`BeginTransformScale` → `TransformScaleDrag` steps → clear on release).
- **Move selection via drag.** Clicking a selected shape with the Select tool and
  dragging emits coalesced `MoveSelection {dx, dy}` actions (window px → doc units
  via zoom scale). No handle hit required.
- **SVG `<line>` import.** `x1/y1/x2/y2` attributes map to `Shape::Line`.
  Stroke color falls back to fill when stroke is transparent.
- **SVG `<g>` group import.** Nested `<g>` tags push/pop a group-id stack;
  all shapes parsed inside a `<g>` receive `Shape::set_group(gid)`.
- **Named CSS colors in SVG.** `parse_color` now handles `black`, `white`,
  `red`, `green`, `blue`, `yellow`, `orange`, `purple`, `cyan`, `magenta`,
  `gray`, `silver`, `darkblue`, `darkgreen`, `darkred`, plus short hex `#rgb`.

### Changed
- **`contour-gpui` — Wave 10: prism-ui token enforcement.** ~120 hardcoded `rgb()` panel
  colors replaced with `prism_ui::colors::*` tokens. Text-label buttons replaced with
  `Icon` equivalents from `prism-ui`.
- **`contour-gpui` — gpui upgraded to git rev `1d217ee` + gpui-component 0.5.2
  adopted.** `prism_ui::init(cx)` called at startup. Rust 1.95.0 required.

### Added

- **GPUI host — Wave 9 parity: eyedropper, shape builder, text-on-path, SVG I/O, keyboard shortcuts (`contour-gpui`).**
  All implemented through the single `App::apply` choke point:
  - **Eyedropper tool.** `CanvasHost::sample_pixel` indexes the cached BGRA8 frame; canvas click emits
    `Action::PickColor([u8;4])` which sets `App::fg_color`, applies it as fill to the selected shape,
    and auto-reverts to `prev_tool`. Keyboard shortcut `I`.
  - **Shape Builder tool.** Stub: `MergeRegion(ids)` selects all mentioned shapes; `SubtractRegion(ids)`
    logs. Tool strip entry wired.
  - **Text on Path.** Inspector shows "Text on Path" / "Detach" button when selection is exactly one text
    + one path. `Action::AttachTextToPath` / `DetachTextFromPath` maintain `App::text_on_path` map.
  - **SVG export.** Toolbar "Export SVG" → `Action::ExportSvg(path)` → plain-string SVG serializer
    covering Rect, Ellipse, Line, Path, Text. Status message shown for 3 s.
  - **SVG import.** Toolbar "Import SVG" → `Action::ImportSvg(path)` → tag-scan parser (no deps)
    supporting `<rect>`, `<ellipse>`, `<circle>`, `<path>`, `<text>`. Status message on completion.
  - **Keyboard shortcuts.** `V` Select, `A` Direct-Select, `P` Pen, `E` Ellipse, `R` Rect, `T` Type,
    `I` Eyedropper; `Cmd+G` group, `Cmd+Shift+G` ungroup; `Cmd+[`/`Cmd+]` send backward / bring forward.
  - **Group / Ungroup.** `Action::GroupSelected` assigns a shared `group` id (via `Shape::set_group`);
    `Action::UngroupSelected` clears it. Both undoable.
  - **Layer order.** `Action::MoveLayerOrder { id, delta }` moves a shape in paint order with selection
    tracking. Cmd+[ / Cmd+] bound.
  - 5 new tests (38 total passing).

- **GPUI host — live-shape + text inspector, marquee multi-select, align & distribute (`contour-gpui`).**
  Three parity waves over the same `App::apply` choke point and the shared
  `contour-app` engine (no fork):
  - **Live-shape + text inspector params.** When the primary selection is a live
    Polygon / Star, the inspector grows a **Live Shape** section with `−/＋`
    steppers for **Sides / Points**, **Radius**, and (star) **Inner ratio**; each
    click rebuilds the `LiveShape` and emits `SetLiveShape`, regenerating the
    outline live via `Shape::set_live_shape` and mirroring the working defaults —
    the GPUI mirror of the egui `live_shape_section`. When the primary is a text
    object, a **Type** section exposes a font-**size** stepper, **alignment**
    toggles (Left / Center / Right), and a **font family** cycle button; each
    re-lays-out the glyph cache via `Shape::set_text_params`
    (`TextParams` / `TextAlign`, `fonts::families`). One undo step per edit.
  - **Marquee + multi-select.** The selection is now a canonical `Vec<usize>`
    set (primary = last, secondary = previous, so boolean front/back and every
    existing single-select path keep working). Dragging on empty canvas with the
    Select tool rubber-bands a **marquee** (`MarqueeSelect`) that selects every
    shape whose bounds it intersects; **Shift+click** toggles a shape in/out
    (`AddToSelection`). A multi-selection draws a thin ring per member plus a
    bright union ring, and drives the inspector, boolean (≥2 operands), and the
    new align/distribute group. **Delete** removes the whole set in one undo step.
  - **Align & distribute.** A toolbar group reuses `contour-app`'s pure
    `align::{align_deltas, distribute_deltas, union_bounds}` over the
    multi-selection's bounding boxes: **align** left/center-H/right/top/center-V/
    bottom (≥2 shapes) and **distribute** horizontal/vertical centers (≥3 shapes),
    each applied via `Shape::translate` as one undo step. Buttons grey out until
    the selection is large enough.
- **GPUI host — more shape tools, the Type tool, and boolean ops (`contour-gpui`).**
  The GPUI host gains the rest of the egui app's creation tools plus a real
  pathfinder, all routed through the same `App::apply` choke point and the shared
  `contour-app` engine (no fork):
  - **Line / Polygon / Star tools.** With the Line tool, dragging rubber-bands a
    `Shape::Line` between the two endpoints; with the Polygon / Star tools the
    press point is the centre and the drag distance is the outer radius, building
    a closed live-shape `Shape::Path` (`LiveShape::Polygon { sides, radius }` /
    `LiveShape::Star { points, radius, inner_ratio }`) that stays parametrically
    editable. Counts/ratio come from host defaults (`poly_sides` 6, `star_points`
    5, `star_ratio` 0.5), mirroring the egui app's `shape_from_drag` /
    `live_shape_at`. Degenerate (sub-unit / zero-radius) drags are dropped. All
    inherit the host's default fill/stroke/width, append in paint order, and
    become the selection (one undo step).
  - **Type / Text tool.** Clicking the canvas with the Type tool places a
    point-type `Shape::Text` at the click and enters edit mode; typing appends to
    the string and re-lays-out its glyph outlines live via the shared
    `contour_app::text::layout` (fonts resolved through `fonts::resolve` /
    `ttf-parser`). **Backspace** deletes, **Enter** inserts a newline, **Escape**
    (or switching tools) finishes. Clicking an existing text object re-edits it.
    An object placed but never typed into is discarded; the whole place→type→
    finish session collapses to a single undo entry (coalesced via `History`).
  - **Boolean / pathfinder ops.** Select a shape, **Shift+click** a second
    (amber-ringed) operand, then **Unite / Minus / Intersect / Exclude** from the
    new toolbar group runs `contour_app::boolean::apply` (the same `i_overlay`
    pipeline the egui app drives) on the pair — front = primary, back = secondary
    — replacing both inputs with the styled result batch (a `Shape::Path`, or a
    `Shape::Compound` when the result has holes) and selecting it. One undo step;
    the buttons grey out until two distinct shapes are selected.
  - **Engine reuse.** Geometry, text layout, fonts, and booleans all come from the
    shared `contour-app` library; the host stays a thin CPU bridge. The egui
    `contour` binary is untouched and still green. Build / test: both `contour`
    and `contour-gpui` build clean (clippy too); `contour-app` passes 491 tests
    (0 failed) and `contour-gpui` now has 27 unit tests (10 new for
    line/polygon/star/type/boolean), 0 failed.

- **GPUI host — Pen tool, node editing, and undo/redo (`contour-gpui`).** The
  GPUI host gains the core vector-authoring loop and parity-critical history:
  - **Pen tool.** With the Pen active, clicking the canvas places anchor points
    in document space; press-dragging an anchor sets its (mirrored) cubic-bezier
    out-tangent. The path is closed by clicking back on its first anchor (within
    a tolerance) — or finished open with **Enter** — and committed as a real
    `Shape::Path` that inherits the host's default fill/stroke/width and becomes
    the selection. **Escape** abandons a half-drawn path; switching tools also
    cancels it. An overlay shows every placed anchor (the first one emphasised as
    the close target) and its tangent knobs, tracking pan/zoom. The pen mirrors
    the egui app's `handle_pen` / `commit_pen` exactly (anchors + per-anchor
    out-handle offsets, in-handle mirrored), routed through new
    `Action::PenAddAnchor` / `PenSetHandle` / `PenFinish` / `PenCancel`.
  - **Direct-select (node edit).** With the Direct-Select tool and a `Path`
    selected, the path's anchors and tangent knobs draw as an overlay; clicking
    grabs the nearest anchor (or a handle knob, which takes priority — mirroring
    the egui `hit_path_edit` 6-document-unit pick) and dragging reshapes the path
    via the document model's own `set_anchor` / `set_handle`. Clicking empty
    space re-picks the path under the cursor. Drags coalesce into one undo entry.
  - **Undo / redo.** The GPUI host now drives `contour-app`'s shared `History`
    (the same snapshot model the egui app uses — no fork). Shape create, delete,
    appearance edits, the pen commit, and node edits are all undoable; discrete
    edits checkpoint before mutating, drags coalesce via `BeginInteraction` /
    `EndInteraction` (no-op drags record nothing). **Cmd+Z** undoes, **Cmd+Shift+Z**
    (or **Cmd+Y**) redoes — wired through a focused root view's `on_key_down` —
    and the toolbar gains greyed-when-empty **Undo / Redo** buttons. **Delete /
    Backspace** removes the selected shape (one undo step).
  - **Engine reuse.** All geometry and history come from the shared `contour-app`
    library (a small additive `Shape::path(..)` convenience constructor was added
    alongside `Shape::rect` / `Shape::ellipse`); the host stays a thin CPU bridge.
    The egui `contour` binary is untouched and still green. Build / test: both
    `contour` and `contour-gpui` build clean (clippy too); `contour-app` passes
    491 tests (0 failed) and `contour-gpui` now has 17 unit tests (9 new for
    pen/node/undo-redo/delete), 0 failed.

- **GPUI host — shape tools, pan/zoom, and a selection ring (`contour-gpui`).**
  The GPUI host's canvas is now a real editing surface, not just selection +
  appearance:
  - **Rectangle & Ellipse tools.** With the Rect or Ellipse tool active, dragging
    on the canvas rubber-bands a new shape; release commits it. The new shape
    inherits the host's default fill / stroke / stroke-width, is appended in paint
    order, and becomes the selection. Creation routes through
    `App::apply(Action::CreateShape { tool, a, b })` (corners in document space),
    which normalizes the rect, drops a degenerate sub-unit drag, builds a
    `Shape::rect` / `Shape::ellipse` from the **reused** `contour-app` document
    model, and marks the host dirty so the preview re-rasterizes. A live preview
    box tracks the drag.
  - **Pan & zoom.** A view transform (`offset` + `scale`) is applied both when
    displaying the bridged preview (the artboard image is placed at `offset` and
    scaled by `scale`) and when mapping window ↔ document coordinates, so
    click-select stays correct under any pan/zoom. Pan = middle-drag or
    Alt/Cmd + left-drag (`Action::PanBy`); zoom = scroll wheel, anchored to the
    point under the cursor so it stays fixed on screen (`Action::ZoomBy`). The
    toolbar gains `−` / `％` / `＋` / `Reset` zoom controls (`ResetView`); zoom is
    clamped to 0.1×–16×.
  - **Selection ring.** The selected shape's document-space bounds
    (`App::selected_bounds`, from the model's own `Shape::bounds`) are mapped
    through the view transform and drawn as an accent-colored box overlay on the
    preview — so the selection is visible on the canvas, not just in the dock.
  - **Scope / engine reuse.** All geometry comes from the shared `contour-app`
    document (no fork); the host stays a thin CPU bridge. Pen/path editing,
    gradients, text, boolean ops, and the other tools remain later waves. The
    egui `contour` binary is untouched and still green. Build / test: both
    `contour` and `contour-gpui` build clean (clippy too); `contour-app` passes
    491 tests (0 failed) and `contour-gpui` adds 8 unit tests for the
    create/pan/zoom/selection math (0 failed).

- **GPUI host — canvas click-select + Inspector panel (`contour-gpui`).** The
  GPUI host is now editable, not just a read-only preview:
  - **Click to select.** A left click on the artboard preview selects the
    topmost shape under the cursor. The root view maps the window click →
    preview pixel → document space (adding the active artboard's origin, since
    the preview is the artboard cropped + translated by `(-ox, -oy)`), then
    emits `Action::HitTestSelect { x, y }`. Selection reuses the document
    model's own `Shape::hit` (scanning back-to-front for the first *selectable*
    hit) so the GPUI host and the egui canvas pick identically; a click that
    misses every shape clears the selection. An overlaid `canvas()` element
    captures the painted content rect each frame (the same idiom Pigment's host
    uses) so the handler has a live window→doc map.
  - **Inspector panel (right dock).** For the selected shape it shows and EDITS
    fill colour, stroke colour, stroke width, and opacity. gpui `0.2.2` has no
    native slider, so editing uses compact `−/＋` stepper rows (R/G/B channels,
    stroke width, opacity) plus clickable preset swatches. Every edit routes
    through `App::apply` (`SetFillColor` / `SetStrokeColor` / `SetStrokeWidth` /
    `SetOpacity`), mutates the reused `contour-app` document, and marks the host
    dirty so the preview re-rasterizes. Opacity writes the fill-alpha channel —
    Contour's legacy shape model has no separate opacity field, so that is the
    closest single-field analogue that re-rasterizes visibly. No selection shows
    an empty-state hint.
  - **Scope.** This pass is selection + appearance editing only; pen/path
    editing, gradients, text, boolean ops, and transforms are later waves. The
    egui `contour` binary is untouched and still green. Build / test: both
    `contour` and `contour-gpui` build clean (clippy too); the suite still
    passes (491 tests, 0 failed).

- **GPUI UI migration — Phase 1+2 starter (`contour-gpui`).** A second binary
  crate stands up a [GPUI](https://www.gpui.rs/) host that coexists with the
  shipping eframe/egui `contour` binary (no rip-out — the egui app is untouched
  and stays green). It follows the pattern proven in the sibling Pigment app
  (`pigment-gpui`):
  - **One shared state + one mutation choke point.** `app_state::App` owns the
    document, the active tool, the selection, and the preview host; panels never
    mutate it directly — they emit an `Action` that the root view routes through
    `App::apply`, the single place state changes (and the host is marked dirty
    when pixels change). New interactions add an `Action` variant + an `apply`
    arm, so panels port in parallel without colliding.
  - **CPU preview bridge (no GPU path).** Contour rasterizes vectors CPU-side
    (tiny-skia), so the host reuses the existing export raster path: a new
    additive `export::to_rgba8_artboard` shares the PNG exporter's pixmap and
    hands back the active artboard's raw straight-RGBA8 pixels. `canvas_host`
    converts RGBA8 → BGRA8 (GPUI's `RenderImage` byte order), wraps it as a
    `RenderImage`, and dirty-caches it (re-rasterizes only on change). Drawing
    vectors as native GPUI shapes is a later optimization.
  - **Live chrome.** The root view lays out a top toolbar, a left tools strip
    (one button per tool, active one highlighted, click → `SetTool`), the center
    artboard preview, and a right dock. The **Layers panel** is real: one row per
    shape (top row = topmost), with a visibility toggle (→ re-rasterizes) and
    row-select wired through the `Action` round-trip — the verbatim reference
    documented in `panels/mod.rs`.
  - **Bin + lib split (structural only).** `contour-app` now also builds as a
    library so `contour-gpui` can reuse its document model + rasterizer without
    forking the engine; `main.rs` re-enters through the new `lib.rs`. Every module
    already referenced its siblings via `crate::…`, so the egui binary's behavior
    is unchanged. Two convenience `Shape::rect` / `Shape::ellipse` constructors
    were added (additive) to seed the host's sample document.
  - **Build / test:** both `contour` and `contour-gpui` build green; the existing
    suite still passes (491 tests, 0 failed). GPUI `0.2.2`, `image 0.25`, requires
    the Metal toolchain.

- **Placed / linked images bake into SVG & PNG export.** Placed images were
  canvas-only; they now render into both exporters in the correct z-order (over
  the vector artwork, matching the canvas), honouring each image's placement
  transform and clipping mask:
  - **PNG (raster):** each placed image's RGBA pixels are composited through its
    placement `Affine` (position / scale / rotation) as a tiny-skia `Pattern`
    quad, source-over the rasterized artwork. A clip ring restricts the fill via
    a `Mask`. Embedded sources use their stored bytes; **linked** sources are
    re-read from disk at export time (skipped with a warning if unreadable, the
    same graceful degradation the canvas uses).
  - **SVG (vector):** each placed image emits an `<image>` element carrying the
    placement matrix as `transform="matrix(a b c d e f)"`. An **Embedded** source
    becomes a self-contained base64 PNG `data:` URI; a **Linked** source becomes
    an `href` / `xlink:href` to its file path (Illustrator-style linked export). A
    clip ring becomes a `userSpaceOnUse` `<clipPath>` the image references.
  - **Additive / back-compat:** a document with no placed images exports a
    byte-identical SVG (the `xmlns:xlink` declaration is only added when an
    `<image>` is present) and a byte-identical PNG (the bake loop is a no-op on an
    empty list). Hidden placed images are skipped.
  - **Pure & unit-tested:** the placement→SVG matrix string (`PlacedImage::
    svg_transform`), the data-URI/path href selection by source kind, a tiny
    RFC 4648 base64 encoder (test vectors), embedded→data-URI / linked→href /
    clip→clipPath emission, the PNG bake compositing into the right pixels in
    z-order over a shape, the PNG clip honouring the ring, and the
    no-placed-images byte-identical SVG + PNG guards.

## [0.6.0] - 2026-06-17

### Added

- **Place / Link images — embedded or linked rasters with a clipping mask.**
  Place a raster image into the document as a first-class element the way
  Illustrator's *File ▸ Place* does:
  - A pure, fully unit-tested `placed_image` module. A `PlacedImage` carries an
    `ImageSource` — **`Embedded`** (decoded RGBA8 pixels stored in the document,
    self-contained) or **`Linked`** (only the on-disk path + cached natural size,
    pixels re-read from disk) — plus its own placement `Affine` (position /
    scale / rotation) applied to the image's natural pixel rectangle, and an
    optional **clipping mask** (a closed ring of document points outside of which
    the image isn't drawn). The placement / clip math is pure `f32`: `corners`,
    `drawn_bounds`, `clip_bounds` (image ∩ clip), `clip_contains` (per-pixel
    test) and `hit`. Carried on the document as additive `placed_images`
    (`#[serde(default)]` → empty; pre-Place `.contour` round-trips unchanged).
  - **File ▸ Place Image ▸ Embed… / Link…** picks a raster (`rfd`), decodes it
    (`image`), and places it centred on the active artboard. Placed images render
    on the canvas as a textured quad (position / scale / rotation all show),
    clipped to the clip-path bounds when a mask is set; a linked image's pixels
    are read once into a per-session cache so the render loop never touches disk.
  - **Placed Images inspector**: place embed/link, list + select an image,
    numeric move/scale/rotate (composed onto its transform), **Make clipping
    mask** from the topmost selected path (the mask path is consumed) + **Release
    clip**, **Relink / refresh** a linked image from disk, and delete — each a
    labelled undo step. A placed image is also click-selectable on the canvas
    (topmost-hit, respecting its clip).
  - **Guarded** by unit tests: a drawn rect reflects position + scale; corners
    are the transformed rectangle; a clip limits the drawn region to the path
    bounds (and a non-overlapping clip excludes the image); `clip_contains`
    respects the ring; embed stores pixels / link stores the path; ids never
    reuse within a session; serde round-trip (embedded + linked + clipped) incl.
    legacy (no `clip`/`visible`/`locked`, no `placed_images` key) and
    determinism; plus document-level round-trip + legacy-load tests.
  - **Follow-ups (noted):** crop the placed image, live mtime polling of linked
    files (relink is explicit today), and round-tripping a placed `.pigment` via
    suite interop. _(SVG/PNG export baking — landed in [Unreleased].)_

- **Recolor Artwork — palette remap, reduce-to-N & harmony rules.** Remap a
  selection's colours in one step, the way Illustrator's *Recolor Artwork*
  dialog works:
  - A pure, fully unit-tested `recolor` module. `extract_palette(shapes)` walks
    each shape's fill, stroke, gradient stops and appearance-stack paints —
    exactly the coverage of `Shape::remap_color` — and returns the artwork's
    distinct colours in first-seen order, de-duplicated with the swatch
    picker-rounding tolerance.
  - **Reduce to N colours** (`reduce_to_n`): **k-means** clustering in a
    perceptual-ish space (sRGB RGB scaled by Rec. 601 luminance weights) with a
    **deterministic, seedless** farthest-point initialisation, so the reduction
    is reproducible with no RNG. Returns the reduced palette plus a per-colour
    cluster mapping; `n ≥ len` is the identity.
  - **Harmony rules** (`harmony`): Complementary, Analogous, Triadic and
    Tetradic palettes generated by rotating a base colour's hue in HSL (keeping
    its saturation, lightness and alpha), base colour first.
  - `recolor(old, new)` pairs the old and new palettes by index, dropping
    no-op pairs, and the app applies each pair through the existing
    `Document::remap_color` / `Shape::remap_color` artwork-walk — the same
    machinery a **global swatch** edit uses — as **one undo step**, scoped to the
    selection (or the whole document when nothing is selected).
  - A **Recolor Artwork dialog** (Object ▸ Recolor Artwork…): shows the current
    palette beside an editable new palette, with reduce-to-N and harmony
    controls, a live remap-count preview, Reset, and Apply / Cancel.
  - **Guarded** by unit tests: extraction returns the distinct used colours (in
    order, with picker-tolerance dedup, covering gradient stops); reduce-to-2 on
    a 5-colour artwork yields 2 clusters with every colour mapped and warm/cool
    split correctly; reduction is deterministic and identity at `n ≥ len`; each
    harmony rule produces the expected count and hue relationship (base-first,
    complementary = hue-opposite, triadic = 120° spacing); HSL round-trips; an
    identity remap produces no pairs; an applied recolour remaps bound shapes'
    colours and leaves the rest alone; and the dialog state seeds an identity
    palette, keeps index pairing through reduce, and fills from a harmony rule.

## [0.5.0] - 2026-06-13

### Added

- **Symbols — library, instances & edit-master propagation.** A document symbol
  library plus placed instances, the way Illustrator's Symbols panel works:
  - A pure, fully unit-tested `symbols` module — `Symbol { id, name, shapes }`
    (a reusable *master* = a set of shapes), `SymbolInstance { id, symbol,
    transform }` (a reference to a master + its own placement `Affine`), and an
    ordered, name-unique, id-stable `Symbols` collection holding both. Carried on
    the document as additive `symbols: Symbols` (`#[serde(default)]` → empty), so
    pre-symbols `.contour` files round-trip unchanged.
  - **Edit-master propagation** falls out of one indirection: an instance stores
    no geometry — it is **resolved** on demand (`Symbols::resolve`) by cloning
    the *live* master's shapes and pushing them through the instance transform.
    Editing a master (move / recolour / reshape) therefore changes what every
    instance resolves to, with no per-instance fix-up. Resolution is a pure,
    deterministic function.
  - A **Symbols panel** (right inspector): **Create from selection** turns the
    selection into a master and replaces it in place with an identity instance;
    **Place** drops more instances (cascaded so they fan out); **Update master**
    redefines a symbol from the selection and every instance follows; per-symbol
    **rename** / **delete** (delete cascades to its instances). A placed instance
    can be selected (with a canvas selection ring), nudged / scaled / rotated
    numerically about its centre, **Break link**-ed into editable shapes, or
    deleted — each one a labelled undo step.
  - Instances render on the canvas resolved against their masters, and are
    **baked into SVG / PNG export** (`Document::flattened_for_export`) so output
    matches the canvas. The `Affine` transform is now serializable so a
    placement persists in `.contour`.
  - **Guarded** by unit tests: define-from-N-shapes, two instances resolving
    through their own transforms, master-edit propagation (geometry + colour) to
    all instances, instance break / delete, symbol-delete cascade, dangling-
    instance safety, id non-reuse, serde round-trip (incl. legacy / no-symbols),
    and determinism.
  - *Follow-ups (noted):* the symbol **sprayer / shifter / sizer** tool set, and
    on-canvas direct drag/scale/rotate of an instance (today's instance edits are
    driven from the panel's numeric buttons).

- **Color system — Swatches, global colours & eyedropper.** A document colour
  palette you build from your artwork and click to paint, with Illustrator-style
  **global** swatches and an eyedropper:
  - A pure, fully unit-tested `swatches` module — `Swatch { id, name, color,
    global }` and an ordered, name-unique, id-stable `Swatches` palette — carried
    on the document as additive `swatches: Swatches` (`#[serde(default)]` → a
    starter palette of primaries + black / white / grey). Pre-swatches `.contour`
    files round-trip and their shapes keep their literal colours unchanged.
  - A **Swatches panel** (left dock) **adds** a swatch from the current fill
    (`+`, de-duplicated by colour within picker tolerance), shows a colour-chip
    grid (the chip naming the active fill is ringed; **global** swatches carry a
    corner dot), and **applies** a swatch to the selection's **fill** (click) or
    **stroke** (shift-click) — one labelled undo step each, or sets the app
    default paint when nothing is selected. An inline editor **renames**,
    **recolours**, toggles **global**, and **deletes** the selected swatch.
  - A swatch marked **global** propagates a recolour to all artwork painted with
    it: editing a global swatch remaps every matching fill, stroke, gradient
    stop, and appearance-stack paint across the document (and reports the count);
    editing a **non-global** swatch is a one-time copy that changes only the
    swatch, never the artwork. The resolution is two pure, testable functions —
    `Swatches::recolor` (hands back the `(old, new)` pair only for a global edit,
    `None` otherwise) and `Document::remap_color` (the artwork walk) — kept out
    of any egui / canvas state.
  - An **Eyedropper** tool (`I`) samples the topmost shape's paint appearance
    under the cursor into the active fill / stroke (and stroke width / style /
    gradient) and applies it to the current selection as one undo step;
    **Alt-click** picks up the look without painting (Illustrator's modifier).
  - **Guarded** by unit tests: starter-palette id/name uniqueness, dedup-by-
    colour with picker rounding tolerance, unique-name and self-rename
    discipline, global recolour returns the `(old, new)` pair while a non-global
    edit returns `None` but still mutates the swatch, `Document::remap_color`
    across fills + strokes + gradients with an accurate affected-shape count, a
    same-colour recolour is a no-op, serde round-trip including a legacy
    no-swatches document (loads the starter palette, literal colours untouched),
    and eyedropper sample / apply (paint transfers, geometry / grouping do not).

## [0.4.0] - 2026-06-13

### Added

- **Image Trace.** A new `File ▸ Image Trace…` (and an `Object ▸ Image Trace`
  submenu with the controls) loads a raster image and traces it into editable
  vector paths, as one undo step:
  - Backed by **`vtracer`** (the visioncortex tracer behind Illustrator's Image
    Trace): the picked PNG/JPEG/BMP/GIF is decoded to RGBA, traced into
    colour-region outlines, and converted into the document's path model — one
    filled, closed `Shape::Path` per traced contour, all tagged into a single
    **group** so the result selects / moves as one object and lands on top of the
    document at the image's pixel coordinates.
  - **Mode preset:** **Black & White** (binary, with an adjustable luminance
    **Threshold** 0–255) and **Color** (multi-region, with **Color precision**).
    Both expose vtracer's key knobs — **Filter speckle** (drop tiny regions),
    **Corner threshold** (corner vs smooth spline), and **Path precision** — with
    vtracer's own defaults.
  - The vtracer-output → anchor/handle step is a pure, dependency-light SVG
    path-`d` parser (`trace::svg_path_to_contours`: `M`/`L`/`C`/`Z`, absolute,
    plus the per-region translate offset vtracer emits), reusable as a general
    line/cubic SVG path importer. **Guarded** by 8 unit tests: cubic / line / Z
    parsing and handle correctness (round-tripped through `bez_path`), multiple
    sub-paths, negative/decimal coords, a synthetic black square traces to a
    closed path whose bbox ≈ the square, B/W and Color modes both produce grouped
    shapes, the trace is deterministic, and empty input errors. No `.contour`
    model change (results are ordinary grouped paths).
- **Path editing — Outline Stroke.** A new `Object ▸ Path ▸ Outline Stroke`
  command (Illustrator's `Object ▸ Path ▸ Outline Stroke`) converts the selected
  path's **stroke** into a filled outline shape, as one undo step:
  - The result is the region the centred stroke covered — each (sub)contour is
    offset by ±(stroke width / 2) and combined into a band: an **open** path
    becomes one closed band (butt caps) whose area ≈ path length × stroke width;
    a **closed** path becomes an **annulus** (outer + inner ring) filled
    even-odd so its interior is carved out.
  - After the op the new shape's **fill** is the former stroke colour with **no
    stroke**. A single open band stays a plain `Shape::Path`; any annulus /
    multi-contour result is an even-odd `Shape::Compound`. `live` parametric
    params are dropped (consistent with Simplify / Offset). A shape with no
    visible stroke is a no-op (the menu item is disabled).
  - Implemented as a pure `pathedit::outline_stroke` helper (points + closed flag
    + half-width → closed ring(s)), reusing `stroke::offset_contour` /
    `pathedit::offset_path`. **Guarded** by unit tests: an open segment outlines
    to a band whose bbox ≈ length × width, a closed path to an annulus of
    positive area, zero / negative width and degenerate input are no-ops, and the
    result is deterministic. No `.contour` model change.
- **Path editing — Simplify & Offset Path.** Two new commands under a new
  **`Object ▸ Path`** menu act on the selected shape as one undo step:
  - **Simplify** reduces a path's anchor count while preserving its shape:
    it flattens the outline (honouring bezier handles) and runs a
    **Douglas–Peucker** line-reduction at an adjustable **Tolerance** (document
    units; higher drops more anchors). Endpoints are always kept, a closed path
    stays closed (never collapsing below 3 anchors, open below 2), and re-running
    on an already-minimal path is a no-op.
  - **Offset Path** produces a new contour offset by a signed **Offset** distance
    from the flattened source, using **miter joins** (angle-bisector offset with a
    miter clamp): positive grows a closed path outward, negative shrinks it
    inward (winding-independent), and an open path shifts to one side. Offset by 0
    is the identity.
  - Both live in a new pure, headless-testable **`pathedit`** module
    (`simplify` / `offset_path`, input points → output points). The UI demotes
    the selected shape to a plain corner path first (`Shape::to_path`, preserving
    paint / group / membership tags and dropping any `live` parametric params,
    consistent with the existing live-shape→path demotion); a **Compound** path is
    simplified / offset per sub-contour and stays a compound. No `.contour` model
    change.
  - **Guarded** by unit tests: Simplify drops redundant collinear anchors while
    keeping endpoints + genuine corners, is idempotent on a minimal path, honours
    the closed-ring floor, and is deterministic; Offset grows the bbox / area on
    `+d`, shrinks on `−d`, is identity at `0`, grows outward regardless of
    winding, is an exact miter on a right angle, and is deterministic.
- **Live shapes — Polygon & Star.** Two new creation tools draw parametric
  primitives that stay editable after the fact, the suite's first live shapes:
  - **Polygon** and **Star** tools in the toolbar (and the `polygon` / `star`
    icons). Each draws from the **centre** outward — the press point is the
    centre and the drag distance sets the outer radius — with a live rubber-band
    preview, like the Rectangle / Ellipse tools.
  - A new pure, headless-testable `liveshape` module generates the closed outline
    from a small parameter set: a **Polygon** is `sides` vertices (3–100) on a
    circle of `radius`; a **Star** is `points` tips (3–100) on `radius`
    alternating with inner vertices on `radius × inner_ratio` (0.05–1.0).
    Vertices are emitted clockwise from 12 o'clock; counts / radii are clamped so
    even a hand-edited file yields a valid closed ring.
  - The parameters ride on the existing `Shape::Path` in a new additive
    `live: Option<LiveShape>` field (`#[serde(default)]` → `None`), so the
    generated `points` / `handles` render, hit-test, boolean-op, and export
    exactly like any other path with no special cases, and **pre-existing
    `.contour` files load and round-trip unchanged** as plain (non-live) paths.
  - A **Live Shape** inspector section (shown only for a selected polygon / star)
    edits the sides / points, radius, and inner ratio; each change regenerates
    the outline about the shape's current centre — so a *moved* shape stays put —
    as one undo step (the same cache-from-params idea point type uses for its
    glyph cache). The last-used counts / ratio are remembered for the next draw,
    and the layer row labels the object **Polygon** / **Star**.
  - **Illustrator-style demotion:** directly editing an anchor or handle (move /
    add / delete / smooth-corner toggle) drops the live parameters, expanding the
    shape into a plain editable path so the geometry and parameters never drift
    apart.
  - **Guarded** by unit tests for the pure geometry (per-side vertex count, all
    vertices on the radius, first vertex straight up, linear radius scaling, star
    outer/inner alternation, `inner_ratio == 1.0` degenerating to a polygon,
    count clamping, determinism, centre translation) plus document tests for
    serde round-trip, the layer label, regeneration about a moved centre, and
    anchor-edit demotion.

## [0.3.0] - 2026-06-13

### Added

- **Font family selection for the Type tool.** Point type can now be set in any
  installed system typeface, not just the single bundled face. A new pure
  `fonts` module enumerates installed families with the lightweight
  [`fontdb`](https://crates.io/crates/fontdb) crate (built on the same
  `ttf-parser` already used for outline extraction) and resolves a family name to
  cached face bytes:
  - A **Font** dropdown in the inspector's **Type** section (above Size/Align)
    lists every available family — the bundled default (**Ubuntu**) first —
    plus any system font. Picking a family re-extracts the glyph outlines from
    the chosen face and re-lays-out as one undo step, so the canvas, SVG export,
    and PNG export all render with the selected typeface (text continues to
    compose / export as real even-odd glyph paths).
  - The chosen family is **persisted per text object** in `.contour` as a new
    additive `font_family` field (`#[serde(default)]` → `None`), so pre-existing
    files — which carry no family — load and round-trip unchanged, defaulting to
    the bundled face.
  - Resolution is **forgiving**: an unknown / not-installed family falls back to
    the bundled face rather than vanishing, and the inspector flags such a
    family as `(missing)` so the fallback is visible. Loaded faces are cached so
    a font file is read at most once, never per frame.
  - **Guarded** by regression tests that lock in correct placement across a
    property edit: changing a text object's **font family** or **size**
    re-extracts the glyph outlines about the object's existing `origin` (via
    `Shape::set_text_params`), so a placed — and moved — text object stays put
    rather than jumping to the canvas corner on a font/size change.

## [0.2.0] - 2026-06-09

### Added

- **Graphic styles** (Phase 3). A pure, unit-tested `graphic_styles` module adds
  a document-level **named-appearance library** — each style captures a full
  [`Appearance`] snapshot (its whole fill / stroke / effect stack, with every
  item's paint, opacity, blend mode, and visibility), surfaced through a new
  **Graphic Styles** section in the inspector:
  - **Save** the current selection's effective appearance as a new named style
    (`+`); a freshly-saved style is selected, ready to rename. The captured
    appearance is the shape's explicit stack *or* one migrated from its legacy
    single fill/stroke fields, so a style can be saved off any shape.
  - **Apply** a style to the whole selection by clicking it — overwriting each
    shape's appearance with the style's snapshot (replacing whatever stack it
    had) through the existing `set_appearance` path, as a **single labelled undo
    step** (`Apply Graphic Style`). Alt-click selects a style for editing without
    applying.
  - **Rename** and **delete** styles in the per-style editor, with the same
    unique-name discipline (numeric-suffixed clashes) the Swatches panel uses.
  - The library is a new optional document field (`graphic_styles`, additive
    `#[serde(default)]` → empty), so every pre-existing `.contour` file loads
    with no styles and round-trips unchanged; saved styles serialize with the
    document.

## [0.1.0] - 2026-06-09

### Added

- **Align & distribute** (Phase 2). A pure, unit-tested `align` module turns a
  slice of bounding rects into per-object `(dx, dy)` translation deltas, surfaced
  through both an inspector **Align section** and an **Object ▸ Align /
  Distribute** menu over the current multi-selection:
  - **Align** the selection's left / horizontal-centre / right edges and top /
    vertical-centre / bottom edges to a reference frame, switchable between the
    **selection bounds** and the **artboard** (so a lone shape can be centred on
    the artboard).
  - **Distribute** three-or-more shapes by evenly spacing a chosen feature (left
    / right / top / bottom edges or centres) *or* by equalising the **gaps**
    between them (horizontal / vertical "distribute spacing", à la Illustrator).
    Distribution sorts by visual position, so selection order does not matter and
    the two outermost shapes stay fixed.
  - Each action applies its deltas through the existing `translate` + checkpoint
    undo path as a **single, labelled undo step** (`Align Left`, `Distribute
    Horizontal Gaps`, …); controls disable until the selection is large enough
    (2+ to align, 3+ to distribute). No model change — it only moves existing
    shapes, so `.contour` files are unaffected.

- **Type tool — point type with real glyph outlines + Convert to Outlines**
  (Phase 5 "Type", foundational slice). Contour gains a **Type tool** (`T`):
  click the canvas to drop a point-type object, then type — characters append,
  **Backspace** deletes, **Enter** starts a new line, **Esc** finishes (an empty
  placeholder is discarded). A blinking baseline caret marks the active edit. The
  inspector's new **Type** section edits the selected text's **string**, **font
  size**, and **alignment** (left / centre / right); fill + stroke reuse the
  existing paint / Appearance model.
  - Text renders as **real vector outlines**: a bundled default font
    (Ubuntu Light, Ubuntu Font Licence — `assets/fonts/`) is parsed with
    `ttf-parser`, and each glyph's TrueType contours (quadratics elevated to
    cubics) are mapped straight onto the document's `(points, handles, closed)`
    path geometry. Those glyph contours are cached on the new **`Shape::Text`**
    variant as sub-paths, so a text object **composes / exports like any vector**
    — it hit-tests, transforms, fills (even-odd, so glyph counters are holes),
    and exports to **SVG** (one even-odd `<path>`) and **PNG** (tiny-skia) through
    the same pipelines as a compound path. Multi-line (`\n`) is supported.
  - **Object ▸ Type ▸ Create Outlines** (and the inspector button) replaces the
    live text with a real editable **`Shape::Compound`** of the glyph outlines.
    A general transform (rotate / scale / shear) on live text bakes it to
    outlines first, so transformed type always renders / exports exactly.
  - `Shape::Text` carries its editable `params` (string + size + align) + `origin`
    plus the cached `glyphs`; every field is additive (`#[serde(default)]`), so it
    **persists to `.contour`** and older files load unchanged. The glyph cache is
    advisory — `Document::relayout_text` rebuilds it from `params` on load so text
    always tracks the current font build. The Layers panel shows a text object's
    string as its row name.
  - New pure module `text.rs` (layout + glyph-outline → sub-path conversion) is
    unit-tested: a known glyph yields non-empty closed contours, advance grows
    with text and scales with font size, multi-line offsets the second line down,
    alignment shifts a line horizontally, and "o" yields its outer + inner
    counter. Model + export tests cover the `.contour` serde round-trip (with the
    additive-default back-compat path), `set_text_params` relayout,
    convert-to-outlines → non-empty compound, translate-moves-origin-and-glyphs,
    and SVG/PNG emission. New dep (contour-app only): `ttf-parser`. *Still open
    (noted):* **area type**, **type-on-path**, threaded/overflow text, rich
    **character / paragraph** panels, **font selection** + OpenType shaping /
    kerning / variable fonts, the glyphs panel, text wrap, and character /
    paragraph **styles** — this pass lands point type only.
- **Layers panel — real object list with visibility / lock / name / colour +
  group nesting + reorder** (Phase 2 "Layers panel"). The placeholder layers
  list grows into a real panel listing every object **top-to-bottom in z-order**,
  with grouped objects gathered under an **expandable / collapsible group
  header** (the closest the flat `Vec<Shape>` model gets to a layer tree). Each
  shape row carries:
  - a **visibility** toggle (hide/show),
  - a **lock** toggle — locked objects render but can't be selected, picked, or
    marquee-grabbed; the gate is the new `Shape::selectable()` (`visible &&
    !locked`) shared by *every* canvas pick path (click, drag-to-move,
    eyedropper, direct-select, marquee) so the panel and canvas never drift,
  - an **editable name** (inline editor on the active row; a blank name reverts
    to the type label),
  - a **layer-colour** swatch (click to set, right-click to clear),
  - **click-to-target** selection kept in sync with the canvas (shift-click
    toggles; a group header selects the whole group),
  - **reorder** controls — up / down (bring-forward / send-backward) and
    bring-to-front / send-to-back — routed through the already-tested
    `arrange::reorder` permutation so the live selection follows the move.
  The new `name` / `locked` / `layer_color` fields are additive on every `Shape`
  variant (`#[serde(default)]`), so they **persist to `.contour`** and older
  files load unnamed / unlocked / un-coloured and unchanged. New pure module
  `layers.rs` builds the panel's row layout (group nesting + collapse) and is
  unit-tested; lock-blocks-selection, hidden-excluded-from-pick, the metadata
  serde round-trip, and back-compat defaults are unit-tested on the model. *Still
  open:* a true recursive **sublayer tree**, **drag-to-reorder** (button reorder
  ships instead), and a per-layer appearance **target dot**.
- **Transform — free-transform tool (scale / rotate / shear) + numeric
  transform + Transform Again** (Phase 4 "Transform"). A pure affine module
  (`transform.rs`: `Affine` with pivot-anchored scale/rotate/shear/reflect,
  per-handle scale & shear factor math, `NumericTransform`, `angle_between`) is
  driven by an on-canvas transform box: drag a corner/edge handle to **scale**
  (Shift = lock aspect), the ring just outside a corner to **rotate**, and
  **Cmd/Ctrl-drag an edge handle to shear**. The inspector Transform section
  adds numeric **scale** (x/y) and **move** (x/y) about the selection centre, on
  top of the existing 90°/180° rotate, flip H/V, and rotate-by controls.
  **Transform Again** (Cmd/Ctrl+D) replays the last gesture — scale, rotate,
  shear, reflect, move, or a full numeric transform — about the *current*
  selection's centre. Every action is one undo step and works on paths and the
  compound-path sub-contours. The affine math is unit-tested. *Still open:*
  transform-each (per-object pivots) and a floating numeric dialog with
  shear/rotate fields (shear is on-canvas; rotate via the rotate-by field).
- **Gradients — Angle (conic) type + perceptual interpolation + dithering**
  (extends the Phase 3 "Gradients" item, building on the already-shipped
  linear/radial multi-stop fills *and* gradient-on-stroke). All three additions
  are modelled, persisted, edited in the inspector, and rendered on every surface
  (egui canvas, SVG, PNG), reusing the shared `prism_core::gradient` primitive as
  the design model (its `Angle` geometry, Bayer dither matrix, and linear-light
  philosophy):
  - **Angle (conic) gradient.** A third `GradientKind` that sweeps the ramp
    around the bounding-box centre starting at the gradient's `angle` (pure
    `gradient::angle_param`). The canvas previews the true conic via the existing
    per-vertex mesh sampler; **PNG** rasterizes the conic into a bbox-sized pixmap
    (per-pixel `color_at`, optional dither) handed to tiny-skia as a `Pattern`
    shader — since tiny-skia 0.11 has no native conic. **SVG 1.1 has no conic
    gradient**, so Angle exports as a `linearGradient` oriented at the angle (a
    documented limitation — canvas/PNG render the real sweep).
  - **Perceptual interpolation.** A per-gradient `Interpolation` toggle
    (Perceptual / sRGB). Perceptual blends stop colours in **linear light** (the
    suite's working space — smoother, no muddy mid-tones, IL-2025 parity);
    `color_at` blends directly, while the SVG/tiny-skia paths (which only
    interpolate stops in straight sRGB) consume a pre-expanded, linear-light
    sub-stop list (`Gradient::render_stops`) so all three surfaces match.
  - **Dithering.** A per-gradient `dither` toggle applying the suite's shared
    Bayer-8×8 ordered dither (no RNG, reproducible) on the conic raster path to
    kill 8-bit banding.
  - **Model + persistence.** Additive `interpolation: Interpolation` and
    `dither: bool` on `Gradient`, both `#[serde(default)]` — pre-existing
    `.contour` files load as **sRGB, un-dithered** (byte-identical to how they
    were authored), while *new* gradients default to perceptual + dithered to
    match the shared primitive's default. Carried through the Appearance stack,
    eyedropper, blend, boolean and clip paint copies for free.
  - **Inspector.** The gradient editor gains the Angle kind, an Interpolation
    combo, a Dither checkbox, and exposes the angle slider for Angle gradients
    (it already drove linear direction).
  - 13 new unit tests: perceptual vs sRGB blending (linear-light midpoint),
    `render_stops` expansion (count / monotonic offsets / colour match / no
    duplicate seams), `angle_param` conic sweep, `.contour` round-trip + legacy
    back-compat defaults, SVG Angle→linear fallback + perceptual stop expansion,
    PNG conic-sweep render, and the pure Contour→tiny-skia stop mapping
    (`ts_stops`). *(Open: separate **opacity stops** as their own editor rail
    (opacity is carried per-stop in the RGBA colour today), `Reflected`/`Diamond`
    geometries, a native SVG conic export, and **mesh / freeform** gradients.)*
- **Stroke options — align stroke + arrowheads** (completes the Phase 3
  "Stroke options" item alongside the already-shipped caps/joins/miter/dashes).
  Two new stroke attributes, modelled, persisted, edited, and rendered end to
  end on all three surfaces (egui canvas, SVG, PNG):
  - **Align stroke (Center / Inside / Outside).** Center is the existing
    behaviour. Inside / Outside shift the stroke band fully to one side of the
    path by offsetting the **centerline** by ±`w/2` along its outward normal and
    stroking the offset contour centered — a pure, renderer-agnostic emulation
    (`stroke::offset_contour` / `aligned_geometry`) that keeps every renderer's
    existing cap/join/dash machinery untouched. Winding-independent (a CCW path's
    "Outside" is still its exterior, via a signed-area sign flip).
  - **Arrowheads (None / Triangle / Open / Circle), start and end, scalable.**
    Markers are **baked geometry** (a filled / stroked outline at the endpoint,
    oriented along the path tangent, sized to the stroke width × a scale slider) —
    the most portable form, drawn identically on canvas, SVG, and PNG with no
    `<marker>` defs. Filled heads trim the line back so the base meets it cleanly
    (`stroke::arrowhead` / `arrow_decorations` / `trim_polyline`). Open paths
    only (Illustrator marks open ends).
  - **Model + persistence.** Additive `align: StrokeAlign`, `start_arrow` /
    `end_arrow: Arrowhead`, `arrow_scale: f32` on `StrokeStyle`, all
    `#[serde(default)]` (scale defaults to `1.0`) — pre-existing `.contour` files
    load as a centered, arrow-less stroke and render unchanged. Carried through
    the Appearance stack's per-stroke `style`, the eyedropper, blend, boolean and
    clip paint copies for free.
  - **Rendering.** The shared tiny-skia raster path (canvas + PNG) builds
    per-stroke align/arrow geometry (`export::StrokeContour` / `StrokeDecor`);
    the egui painter can express neither, so a shape with a non-center align or
    an arrowhead now routes through the raster path (new
    `Appearance::needs_stroke_decor` → `needs_raster`), keeping canvas == PNG.
    SVG emits the baked offset / trimmed centerline plus per-marker `<path>`s.
  - 27 new pure unit tests (offset-contour correctness incl. right-angle miter
    and winding independence, arrowhead tip/base/scale geometry, endpoint-tangent
    + line-trim math, align Inside/Outside grow/shrink) + `.contour` back-compat
    & round-trip + SVG-marker / PNG-align export tests. *(Open: **width profiles**
    / variable-width strokes, and align / arrowheads on **compound** paths and
    each Appearance stroke layer **individually in the inspector** — the panel
    edits the topmost stroke's options today, the model already supports per-layer.)*
- **Direct-Select tool (`A`) — anchor & handle editing** (closes the Phase 4
  "Direct-select" item). A dedicated tool that selects and reshapes individual
  anchor points and their Bézier control handles on a path **and** on the
  sub-contours of a compound path, end to end:
  - **Pick & move anchors.** Click an anchor to select it (Shift-click extends),
    then drag to move it; the whole selected anchor set drags together. Anchors
    are addressed by a `(contour, anchor)` pair so a `Shape::Path` and every
    sub-path of a `Shape::Compound` edit through the same code (new
    `Shape::contour` / `contour_mut` / `set_anchor` / `set_handle` accessors).
  - **Reshape handles.** A selected anchor shows its mirrored tangent handles;
    dragging the out- or in-knob bends the adjacent curves live (the in-knob
    mirrors through the anchor).
  - **Marquee anchors.** Rubber-band on empty canvas selects every anchor of the
    primary shape inside the box (Shift adds to the current set) — new pure
    `anchors_in_rect` helper.
  - **Add / delete / convert.** Click a path segment to **insert** an anchor
    (de Casteljau split, shape-preserving, across all sub-contours); **Delete**
    removes the selected anchors and re-fits the path (refusing to drop a contour
    below two points); **Alt-click** an anchor **converts** it smooth↔corner
    (corner = no handle, smooth = mirrored tangent synthesised from the
    neighbours) via new `make_corner` / `make_smooth` primitives.
  - **On-canvas overlay.** Pixel-aligned anchor glyphs (round = smooth, square =
    corner), drawn **selected** (filled accent) vs **unselected** (white with an
    accent ring), plus handle lines and knobs for each selected anchor — through
    the existing comp↔screen mapping (`canvas::paint_direct_select`).
  - All edits route through the existing undo system (drags coalesce into one
    step; no-op edits record nothing). Keyboard: `A` Direct-Select, `V` Select.
    9 new pure unit tests (marquee containment, handle mirror math, smooth/corner
    convert, compound sub-contour insert/delete/convert, min-points guard).
- **True compound-path object — `Object ▸ Compound Path ▸ Make / Release`**
  (closes the documented "compound-path object" gap left by the Pathfinder pass).
  A new `Shape::Compound` document variant keeps several sub-contours (an outer
  ring plus inner holes, or disjoint regions) as **one object** filled under a
  per-object **fill rule** (even-odd / non-zero), rendered and hit-tested as a
  unit:
  - **Model.** `Shape::Compound { subpaths: Vec<SubPath>, fill_rule, … }` carries
    the same paint / group / clip / opacity-mask / blend tags as the other
    variants. A `SubPath` is a `(points, handles, closed)` ring (curves and all).
    A new pure `point_in_rings` implements the **even-odd parity** and **non-zero
    winding** containment tests; `Shape::hit` uses it so a click in a hole misses
    and a click on the solid frame hits. `bounds` unions the sub-contours;
    `translate` / `apply_affine` move them together; `outline_polygon` returns the
    outer ring (so clip masks / single-ring consumers still work).
  - **End-to-end rendering.** A compound path always routes through the shared
    `tiny-skia` raster path on the **canvas** (egui's painter can't express a
    fill rule) and **PNG** export, threading a `FillRule` (Winding / EvenOdd) into
    `fill_path`; **SVG** export emits one `<path>` whose `d` concatenates every
    sub-contour with `fill-rule="evenodd"` / `"nonzero"`, so holes carve natively
    in any viewer. Selection / transform / snapping all treat it as one object.
  - **Pathfinder produces compounds.** `boolean::apply` now returns its
    area-conserving results **grouped by region**: a region with holes (e.g. a
    Difference where the front sits fully inside the back) becomes one compound
    path (outer ring + hole sub-contour) instead of two separate rings — the
    document model's real "expanded" Pathfinder output.
  - **Make / Release.** `Object ▸ Compound Path ▸ Make` (`Cmd/Ctrl+8`) combines
    the selected closed shapes' contours into one compound (inheriting the
    back-most shape's paint); `Release` (`Alt+Cmd/Ctrl+8`) splits it back into one
    `Path` per sub-contour. A fill-rule toggle (Non-zero / Even-odd) on the
    submenu sets the selected compound(s)' rule. Each is one undo step.
  - **Back-compatible `.contour`.** The `Compound` variant is additive (`serde`
    enum), and `SubPath`'s `closed` / `handles` default (`#[serde(default)]`), so
    every pre-existing single-ring document loads and renders identically; a
    compound path round-trips through serde (sub-contours + fill rule + paint).
  - **Tests.** Pure unit tests cover the even-odd / non-zero winding correctness
    (`point_in_rings`, `Shape::hit`), compound bounds / translate, a `.contour`
    round-trip of a compound, SVG `fill-rule` emission + two-sub-path `d`, and PNG
    rasterization of the hole (even-odd empties the centre, non-zero fills it).

- **Shape Builder tool (`M`)** — interactive merge / subtract by dragging across
  the selected shapes' overlapping regions, à la Illustrator (closes the
  documented Shape Builder gap). Reuses the `i_overlay` boolean backend:
  - **Region graph.** A new pure `shapebuilder` module builds the **atomic faces**
    of the selected shapes (each a maximal region lying inside a fixed subset of
    the inputs — for two overlapping shapes: `A−B`, `A∩B`, `B−A`) by iteratively
    subdividing with `i_overlay` intersect / difference. Each face records the
    back-most covering input shape, for paint inheritance.
  - **Pointer interaction.** Drag across the canvas: a plain drag **unites** every
    face the pointer crosses into one path (or a compound if the union has holes);
    an **Alt/Option-drag** **deletes** the crossed faces. The faces under the drag
    are picked by hit-testing the sampled path (`face_at` / `faces_along`); the
    untouched faces stay as separate paths, so the rest of the partition is
    preserved. On release the selected shapes are replaced with the result (one
    undo step). A live overlay shades the crossed regions (accent for unite,
    red for subtract) and traces the drag path.
  - **Paint.** A merged region inherits the back-most picked face's owner shape's
    fill / gradient / stroke / appearance (Illustrator colours a Shape-Builder
    result with the back object's look).
  - **Tests.** Pure unit tests cover the region-graph face count + tiling (two
    overlapping rects → three faces summing to the union; disjoint → one each),
    face picking (`face_at` picks the smallest covering face; outside → none),
    drag-path face collection, the unite merge (picked faces union, leftovers
    kept), the subtract delete, and back-owner paint inheritance.

- **Exact `Merge` Pathfinder rule** — replaces the old simplified weld (union to
  one region with the back colour) with Illustrator's real behaviour: **merge
  adjacent same-coloured faces** into single paths, and **trim** (remove the
  hidden parts of) **different-coloured** overlapping faces, **preserving each
  face's own fill**:
  - For two operands, Merge welds only when their fills match (within tolerance) —
    `subj ∪ clip` under that shared colour — and otherwise trims: the front shape
    stays whole on top, the back shape has its overlapped part removed, and the
    two abutting faces keep their own colours. **Trim** always trims and keeps each
    face's colour (it never welds), now also preserving each fill rather than
    flattening to one. Each produced face keeps any holes as a compound path.
  - **Tests.** New `boolean` unit tests: Merge of *different*-coloured rects keeps
    two trimmed faces each with its own colour (front whole, back trimmed); Merge
    of *same*-coloured rects welds to one region under that colour; Trim preserves
    each face's colour. The pre-existing same-colour Merge / area / face-count
    tests stay green.

- **Full Pathfinder — `Object ▸ Pathfinder`** (completes Illustrator's Pathfinder
  set on `i_overlay`). The boolean layer grows from three ops to the full ten,
  plus a selectable compound-path fill rule:
  - **All ten ops.** The original Union / Intersect / **Minus Front** (Difference)
    gain **Exclude** (symmetric difference), **Minus Back** (subtract the back
    shape from the front), **Divide** (split the pair into every non-overlapping
    region — overlap + each crescent — as separate filled faces), **Trim** (keep
    the front whole and remove the hidden part of the back), **Merge** (weld the
    abutting faces into one region with the back colour), **Crop** (keep only the
    part inside the front shape) and **Outline** (emit the combined boundary as
    unfilled, hairline-stroked paths). Each face inherits the right operand's paint
    (front vs back) the way Illustrator colours a Pathfinder result.
  - **Results expand to multiple paths.** `boolean::apply` now returns a **batch**
    of `Shape::Path`s instead of a single path: an op that produces disjoint
    regions or a ring-with-a-hole (e.g. a Difference where the front sits fully
    inside the back) is expanded into separate paths — the single-ring document
    model's equivalent of Illustrator's expanded Pathfinder output — instead of
    silently dropping all but the largest contour as before. The two operands are
    replaced by the whole batch in one undo step, and every produced path is
    re-selected.
  - **Compound-path fill rule.** A non-zero ↔ **even-odd** toggle threads through
    every op as the `i_overlay` fill rule, so nested / self-intersecting input
    fills the Illustrator way: under non-zero a same-wound inner ring is absorbed
    (solid), under even-odd it carves a hole. The choice lives on the app and is
    set from the Pathfinder submenu.
  - **UI.** A new **`Object ▸ Pathfinder`** submenu holds the fill-rule toggle, the
    five shape-mode ops, and the five pathfinder ops, each with its own icon,
    enable-gated on exactly two selected shapes. The status line reports the op and
    how many paths it produced.
  - **Tests.** `boolean.rs` grows from 3 to 13 unit tests (all pure, no egui / GPU):
    each op is checked by **total filled area** and **face count** (Union = 175,
    Intersect/Crop = 25, Exclude = 150, Minus Back = 75, Divide = three faces
    tiling 175, Trim/Merge = 175, an enclosing Difference = outer ring + hole ring
    of 900 / 100), Minus Back inheriting the front paint, Outline emitting
    transparent-fill / visible-stroke paths, the **fill-rule hole carving** (a
    same-wound ring-in-ring fills solid under non-zero, carves a 100-area hole
    under even-odd), and an open path / line yielding no result.
  - **Deferred** (noted as gaps): a true **compound-path object** — one path that
    keeps its holes as sub-contours rather than expanding to separate rings — and
    the interactive **Shape Builder** tool (drag across regions to merge / subtract).

- **Blend tool — `Object ▸ Blend ▸ Make / Release / Expand`** (Illustrator's
  Object Blend, specified-steps mode). Select two objects and Make generates *N*
  intermediate objects that morph between them, interpolating **position**, **path
  geometry**, and **appearance** together:
  - **Geometry.** Both outlines are resampled to a common point count along
    **arc length** with `kurbo` (`PathSeg::arclen` / `inv_arclen` / `eval`), so
    two paths with *different anchor counts* still interpolate corresponding
    points; each step linearly interpolates the matched points (point-by-point),
    so position and shape blend at once. Closed↔closed stays closed and open↔open
    stays open; the sample count adapts to the more complex end (clamped 8–256).
  - **Appearance.** Each step's fill colour, stroke colour, per-channel **opacity**
    (alpha), and stroke width interpolate linearly in the existing straight-sRGB
    colour space (reusing the gradient `lerp_color`) — so a red shape blended with
    a blue one steps through purple, a solid→transparent end fades out, etc.
  - **Expand-on-create, releasable.** The intermediate shapes are generated as
    real `Path` objects spliced between the two ends; the two ends plus the steps
    are tagged with a shared blend-set id (additive `blend` / `blend_step`,
    `#[serde(default)]`). **Release** deletes the generated steps and restores the
    two ends; **Expand** detaches the steps into independent objects. One undo
    step each.
  - **UI.** A "Blend" submenu under Object with a **Steps** count control (1–64)
    plus Make / Release / Expand, enable-gated on the selection. New `blend.rs`
    module holds the pure resample / interpolate / step-generation math, all
    unit-tested with no egui / GPU context: arc-length resample lands *N* points
    on a line and on a circle (sub-pixel), interpolating identical shapes
    reproduces them, a two-line blend's middle is the geometric midpoint, colour /
    opacity / width midpoints are correct, steps space `t` evenly and exclude the
    ends, and the `blend` tags round-trip through serde (back-compat verified —
    pre-blend `.contour` files load un-blended).
  - **Deferred** (noted as gaps): a persistent **live re-blend** (re-running when
    an end moves — this pass is expand-on-create); **smooth-color** (spread-steps)
    mode; **blend along a spine** path; bezier-**handle** interpolation (steps are
    corner paths today); **point-correspondence rotation** (index-aligned
    resamples); and **mixed open/closed** topology (blends as an open path).

- **Real blend-mode compositing for the Appearance stack + opacity masks** —
  closes the long-standing "blend modes stored but only Normal composites" gap
  the Appearance / live-effects passes left open, and adds Illustrator's **Make
  Opacity Mask**:
  - **Blend compositing.** The per-fill / per-stroke `BlendMode` now *actually
    composites*. The enum gains Illustrator's separable set (Multiply, Screen,
    Overlay, Darken, Lighten, **Color Dodge**, **Color Burn**, **Hard Light**,
    **Soft Light**, **Difference**, **Exclusion**, plus Normal), each with a pure,
    unit-tested per-channel blend function `B(cb, cs)` (the W3C compositing
    formulas). A non-Normal fill / stroke is rasterized alone with `tiny-skia`
    and composited against everything beneath it via a separable Porter-Duff
    composite on **premultiplied** RGBA8 (`co = αs·(1−αb)·Cs + αs·αb·B(Cb,Cs) +
    (1−αs)·αb·Cb`), so a Multiply layer darkens its backdrop, a Screen layer
    lightens, Difference subtracts, etc. — instead of all rendering as Normal.
  - **Every surface composites identically.** egui's painter can only
    source-over, so any appearance with a non-Normal blend (or a live effect, or
    an opacity mask) now routes through the shared `tiny-skia` rasterize-and-
    composite pipeline on the **canvas** and **PNG** export (one
    `render_shape_layer` path for both). **SVG** export tags each non-Normal
    paint layer with `style="mix-blend-mode:…"` so it composites natively in any
    viewer.
  - **Opacity masks** (`Object ▸ Opacity Mask ▸ Make / Release / Invert Mask`).
    An object can carry a mask defined by another shape whose **luminance**
    (Rec. 709, weighted by the mask's own coverage) drives the object's alpha —
    white reveals, black hides — with an **invert** option. Modelled the same
    non-destructive way as clipping masks: additive `omask` / `omask_path` /
    `omask_invert` tags (`#[serde(default)]`) on every shape, resolved at render
    time (`Document::opacity_mask_of`) by rasterizing the mask shape's luminance
    into the same scratch as the artwork and multiplying it into the alpha
    (`effects::apply_luminance_mask`), applied after live effects. The mask path
    paints nothing on its own (dropped by `render_shapes`, outlined when
    selected). SVG export emits a luminance `<mask>` def referenced on the
    content's group. Menu + inspector "Opacity Mask" section drive Make / Release
    / Invert, one undo step each.
  - **Backward compatible.** The new blend variants and the opacity-mask tags are
    all additive (`#[serde(default)]`), so every pre-existing `.contour` file
    loads and renders identically; an all-Normal, unmasked, effect-free shape
    still takes the original plain egui-painter / direct-`tiny-skia` paths. The
    separable blend formulas (Multiply / Screen / Overlay / Difference / dodge /
    burn edges), the premultiplied composite (Multiply darkens, Screen lightens,
    Difference, transparent-source no-op), the luminance→alpha mask
    (white/black/mid-grey/invert/no-coverage), and serde round-trips of a blended
    fill + a masked object are pinned by unit tests with no egui / GPU context.
  - **Deferred** (noted as gaps): a true **per-object** blend mode (modes are
    per-fill/stroke today; an object-level value still composites as Normal);
    non-separable **HSL** modes (Hue / Saturation / Color / Luminosity);
    **group / layer** opacity masks (single-mask-shape only this pass); and
    **knockout** groups.

- **Live (non-destructive) effects on the Appearance stack — Drop Shadow +
  Gaussian Blur** — a new pure, unit-tested `effects` module and an additive
  `effects: Vec<Effect>` on `Appearance` (`#[serde(default)]`), filling the
  effect seam the Appearance pass left open and bringing Illustrator's "Effect"
  menu staples to Contour as re-editable, parameter-driven entries:
  - An **`Effect`** is non-destructive data on the appearance stack, applied to
    the *rendered* fill/stroke raster (not the path), bottom-to-top after the
    paint layers. This pass ships **Drop Shadow** (offset x/y, blur radius,
    colour, opacity) and **Gaussian Blur** (radius); each effect is editable
    after the fact and round-trips through serde.
  - **egui's painter can't blur**, so live effects render the way the PNG
    exporter already does: the shape's fills + strokes are rasterized with
    `tiny-skia` into a padded scratch pixmap (the padding leaves room for a
    shadow / blur to spill past the artwork), the effect stack transforms that
    raster, and the processed pixmap is composited — uploaded as an egui texture
    on the **canvas**, drawn straight onto the page on **PNG** export. The blur
    is a three-pass separable box blur (converges on a Gaussian) running in
    premultiplied space so soft edges don't halo. Canvas and PNG share one
    `render_shape_layer` pipeline so the two surfaces match.
  - **SVG** export emits a standard `<filter>` (`feDropShadow` / `feGaussianBlur`,
    one primitive per active effect, chained bottom-to-top) and wraps the shape's
    paint stack in a `<g filter="url(#…)">`, so the effect renders natively in any
    SVG viewer rather than being baked to pixels.
  - A new **Effects** section in the Appearance panel: an **add** menu (Drop
    Shadow / Gaussian Blur), **remove**, **reorder** (move up / down), and
    per-effect parameter editors (offsets, blur / radius sliders, shadow colour +
    opacity). Each edit is one undo step.
  - **Backward compatible.** `effects` is additive (`#[serde(default)]` → empty),
    so every pre-effects `.contour` file loads with no effects and renders
    identically; a shape with no *active* effect takes the original plain
    painter / exporter path unchanged. The effect math (box blur, shadow tint /
    offset / composite, bounds padding, active-effect detection, reorder) is
    pinned by unit tests with no egui / GPU context.
  - **Deferred** (noted as gaps): **Transform** (offset / scale / rotate),
    **Outer Glow**, and the distort family (warp, zig-zag, roughen, pucker /
    bloat, round-corners). Effect blend modes also still composite as Normal.

- **Appearance panel (multiple fills / strokes per object, reorderable, with
  per-item opacity / blend / visibility)** — a new pure, unit-tested `appearance`
  module and an additive `appearance: Option<Appearance>` on every shape
  (`#[serde(default)]`), promoting Contour's former single-fill / single-stroke
  shape into Illustrator's non-destructive **Appearance stack**:
  - An **`Appearance`** is an ordered `Vec` of **`Fill`**s and a `Vec` of
    **`Stroke`**s. Each entry carries a **`Paint`** (a solid straight-sRGB RGBA
    colour or an overriding multi-stop `Gradient`), a per-item **opacity**
    (`0..=1`), a **blend mode**, and a **visibility** toggle; a stroke also keeps
    its own width and `StrokeStyle` (caps / joins / dashes). Painting walks the
    fills then the strokes **bottom-to-top**, so later entries sit over earlier
    ones — exactly like a layer stack. The reorder helpers (`raise_fill` /
    `lower_fill` / `raise_stroke` / `lower_stroke`), the legacy bridge
    (`from_legacy`), and the per-item `apply_opacity` are pure functions pinned by
    unit tests (no egui context).
  - A new **stack-aware inspector** Appearance section: **add / remove** fills and
    strokes, **reorder** each within its list, toggle a per-item **visibility**
    eye, and edit the selected item (paint — solid ⇄ gradient — opacity, blend,
    and a stroke's width / style). Edits land as single undo steps and feed the
    app's paint defaults so the next new shape inherits the look.
  - Rendered consistently across all three surfaces from the same model: the
    **on-canvas** painter, **PNG** export, and **SVG** export each walk the stack
    bottom-to-top and emit one layered paint per entry, with per-item opacity
    folded into each paint's alpha.
  - **Backward compatible.** `appearance` is additive (`#[serde(default)]` →
    `None`): a shape with `None` renders identically from its legacy single
    fill / stroke fields, so every pre-existing `.contour` file loads and renders
    the same. `Appearance::from_legacy` migrates a single fill / stroke into a
    one-element-each stack **on demand** (the first time the user opens the
    Appearance section on an old shape) — a zero-width or fully-transparent legacy
    stroke, and a fill-less line, migrate without inventing an empty entry.
    Appearance stacks round-trip through serde, with per-item fields
    (`opacity` / `blend` / `visible`) defaulted so a minimal or older stack still
    loads.
  - **Deferred this pass** (the struct is the seam where they attach): per-shape
    **blend compositing** (blend modes are stored and editable but only `Normal`
    composites today), **mesh gradients**, and **patterns**. *(The live
    **effects** vec is now filled — see "Live (non-destructive) effects" above —
    with Drop Shadow + Gaussian Blur shipping and Transform / Outer Glow /
    distorts still open.)*

- **Swatches panel (named colour library, global swatches)** — a new pure,
  unit-tested `swatches` module and an additive `swatches: Swatches` palette on
  the `Document` (`#[serde(default)]`), bringing the everyday Illustrator /
  Affinity colour-library staple Contour was missing:
  - A **`Swatch`** is a named straight-sRGB RGBA colour with a stable id and a
    **global** flag; a **`Swatches`** collection is the document palette — an
    ordered, name-unique list (a clash gets a numeric suffix, à la Illustrator)
    with colour de-duplication so adding the same colour twice never piles up
    duplicates. A fresh document opens with a small starter palette (white /
    black / grey + red / orange / yellow / green / blue). All of the palette
    bookkeeping — `next_id`, `unique_name`, `add` (de-dup by colour),
    `rename` (unique, but a self-rename is a no-op), `set_global`, `recolor`,
    `remove`, `id_for_color` — is pure and pinned by unit tests (no egui
    context).
  - A new **Swatches panel** docked on the left (toggled from the Window menu,
    which now lists it): a grid of colour chips you **click** to paint the
    selection's fill (and set the default fill so the next shape inherits it),
    **Shift-click** to paint the stroke, or **Alt-click** to select for editing.
    The chip naming the current fill gets an accent ring; a global swatch shows a
    small corner dot. A `+` button adds a swatch from the current fill, and an
    editor for the selected swatch renames it, recolours it, toggles its global
    flag, applies it to fill / stroke, or deletes it. Each edit is one undo step.
  - **Global swatches** recolour the artwork: editing a global swatch's colour
    hands back its `(old, new)` pair, and `Document::remap_color` walks every
    shape — remapping the old colour across solid fills, strokes, **and gradient
    stops** (with the same picker-rounding tolerance the swatch model uses) — so
    artwork painted with that swatch follows the edit, and the status bar reports
    how many shapes changed. A non-global swatch is a plain shortcut: editing it
    touches only the swatch, and deleting any swatch leaves the artwork's colours
    intact. The `swatches` field is additive (`#[serde(default)]`), so a
    pre-swatches `.contour` loads with the default starter palette; palettes
    (names, colours, global flags) round-trip through serde. The document-level
    `remap_color` integration is pinned by unit tests alongside the module's pure
    helpers.

- **Clipping masks (`Object → Clipping Mask → Make / Release`)** — a new pure,
  unit-tested `clip` module and two additive `(clip, mask)` tags on every shape
  (`#[serde(default)]`), bringing the everyday Illustrator / Affinity operation
  of cropping artwork to the shape on top:
  - **Make** turns the selection into a **clip set**: the **topmost** selected
    shape becomes the **mask**, and the shapes below it are clipped to its
    outline. **Release** restores the originals. Wired into an **Object →
    Clipping Mask** submenu, an inspector "Clipping Mask" section, and the
    Illustrator keys **Cmd/Ctrl + 7** (make) / **Alt + Cmd/Ctrl + 7** (release).
    Make needs 2+ unclipped objects; Release lights up whenever the selection
    touches a clip set. Each is a single undo step, and Make re-stacks the set
    into one contiguous block (mask on top) the way grouping does.
  - The mask is modelled non-destructively: the mask and the clipped content
    keep their original geometry (so Release is loss-free and the set round-trips
    through serde), and the *rendered* result is **derived**, never stored.
    `Document::render_shapes` resolves clip sets on the fly — the mask paints
    nothing (an Illustrator clipping path has no fill/stroke), and each content
    shape is replaced by its outline **intersected against the mask** via
    `i_overlay` (`clip::clip_polygon`), yielding an ordinary single-ring polygon.
    Content lying entirely outside the mask drops out; an unusable mask degrades
    gracefully to the unclipped original.
  - Rendered consistently across all three surfaces by routing each through
    `render_shapes`: the **on-canvas** painter (clipped bodies, with a selected
    mask shown as a dashed accent outline so it stays editable), **PNG** export,
    and **SVG** export (clipped content emitted already cropped, so the file
    matches the canvas without `<clipPath>` plumbing).
  - A clip set selects and moves as one unit, like a group: clicking,
    shift-clicking, or marquee-touching any member selects the whole set (the
    group/clip selection expansion is now unified). The `clip`/`mask` tags are
    additive (`#[serde(default)]` → `None` / `false`), so older `.contour` files
    load unclipped. The clip-set planning helpers (`next_clip_id`, `can_make`,
    `can_release`, `members_of`, `mask_of`, `selected_clip_ids`) and the
    intersection geometry are pure functions pinned by unit tests (no egui
    context), alongside document-level `render_shapes` resolution tests.

- **Eyedropper tool (sample / apply paint appearance)** — a new pure,
  unit-tested `eyedropper` module and an **Eyedropper** tool in the left palette
  (keyboard **I**), bringing the everyday Illustrator / Affinity "copy the look
  of that object onto this one" staple:
  - An **`Appearance`** is the *paint* of a shape — its fill (solid colour
    and/or an overriding gradient), stroke colour, stroke width, and stroke
    style (caps / joins / dashes) — detached from the shape's geometry, group
    membership, and visibility. `Appearance::sample` reads it off a shape;
    `Appearance::apply_to` writes it onto another shape **without disturbing
    geometry** (an ellipse stays an ellipse, only its colours change). Both are
    pure functions pinned by unit tests (no egui context).
  - **Click** a shape with the Eyedropper to sample its appearance: the look is
    applied to the current selection as one undo step, and the app's default
    paint is loaded from the sample so the next shape you draw inherits it.
    With **no selection**, the click only loads the defaults (nothing to apply
    to yet). **Alt-click** never paints — it samples into the defaults only
    (Illustrator's "pick up but don't apply" modifier). Clicking empty canvas is
    a no-op.
  - Edge cases match Illustrator: a **`Line`** carries no fill, so sampling it
    leaves a filled target's fill colour intact while clearing any gradient and
    copying the stroke; applying any appearance to a line takes only the stroke.
    New `Shape::stroke_color` / `set_stroke_color` / `stroke_width` /
    `set_stroke_width` accessors back the transfer (shaped like the existing
    `fill_color` / `stroke_style` accessors). The inspector shows a usage hint
    while the Eyedropper is active.

- **Clipboard (copy / cut / paste / duplicate)** — a new pure, unit-tested
  `clipboard` module and a `Clipboard` buffer on the app, bringing the
  everyday Illustrator clipboard staples Contour was missing:
  - **Edit → Cut / Copy / Paste / Paste in Place / Paste in Front / Paste in
    Back** and **Duplicate**, plus the Illustrator keys **Cmd/Ctrl + C**
    (copy), **X** (cut), **V** (paste), **Shift + V** (paste in place), **F**
    (paste in front), **B** (paste in back), and **D** (duplicate).
  - **Copy** snapshots the selection (expanded to whole **groups**, in paint
    order) into a detached buffer that lives *outside* the document, so it
    survives undo/redo and can be pasted repeatedly (or into a new document).
    **Cut** copies then deletes the selection as one undo step.
  - **Paste** drops the buffer on top of the stack, fanned out by a growing
    diagonal nudge (`clipboard::paste_offset`) so repeated pastes step
    down-and-right instead of hiding behind one another, à la Illustrator.
    **Paste in Place** lands the copies at their original coordinates on top;
    **Paste in Front / Back** do the same but force the copies to the front /
    back of the paint order. **Duplicate** makes a single nudged copy without
    disturbing the clipboard. Each paste / duplicate is one undo step and
    selects the new copies.
  - Pasted **group** membership is preserved without colliding with the
    destination: `clipboard::remap_group_ids` rewrites the buffer's group ids
    to fresh ones (starting above every id already in use), keeping shapes that
    shared an id in the buffer grouped together and distinct buffer groups
    distinct, while ungrouped shapes stay loose. The paste nudge growth and the
    group-id remap are pure functions pinned by unit tests (no egui context).

- **Window menu (show/hide panels) + status bar** — a new pure, unit-tested
  `workspace` module and a `Workspace` panel-visibility state on the app, giving
  the editor's shell the workspace controls a pro vector app needs:
  - A **Window** menu in the menu bar lists every dockable panel (Tools,
    Inspector, Status bar) with a checkbox to toggle its visibility — the central
    canvas always fills whatever space is left — plus a **Reset panels** command
    (disabled when the layout already matches the default) that restores the
    everything-shown layout. The checkboxes bind straight to the workspace flags
    (`Workspace::flag_mut`), so hiding the inspector or tool column reclaims the
    space for the canvas, à la Illustrator's Window menu.
  - A **bottom status / context bar** spanning the window under the artwork:
    live document-space cursor coordinates (`X / Y px`, a `–` placeholder when
    the pointer leaves the canvas), the selection count (`No selection` /
    `1 selected` / `N selected`), the active artboard name, and the zoom
    percentage. The right side carries quick **1:1** (reset zoom to 100%) and
    **Fit** (fit all artboards) buttons, mirroring Illustrator's bottom-left zoom
    field. The bar itself is toggleable from the Window menu.
  - The status-line wording and zoom-percentage formatting are pure functions
    (`workspace::status_line` / `workspace::zoom_percent`) so the exact text is
    pinned by unit tests; the `Workspace` struct's visibility bookkeeping
    (default-all-shown, per-panel flags, reset, is-default) is likewise tested
    without an egui context, and is `#[serde(default)]`-friendly for a future
    saved-workspaces feature.

- **Multiple artboards** — a new pure, unit-tested `artboard` module and an
  additive `artboards: Vec<Artboard>` + `active_artboard: usize` on the
  `Document` (both `#[serde(default)]`), promoting the editor's former single
  fixed-`Size` artboard into a stack of named rectangles, the way Illustrator
  lays out several artboards on one canvas:
  - An **`Artboard`** is a named `[x, y, w, h]` rectangle in document space. The
    module's pure geometry — default placement of a new board (tiled to the
    right of the rightmost existing board, top-aligned), topmost-hit picking, and
    the union of all boards — lives apart from any UI so it is unit-tested
    directly.
  - A new **Artboard tool** in the left palette: drag on empty canvas to create a
    board, drag an existing board to move it, click to make a board active. Every
    artboard paints under the artwork with its name labelled above the top-left
    corner; the **active** board gets an accent frame + label, inactive boards a
    muted grey frame. Create / move land as single undo steps (artboards live on
    the `Document`, so the snapshot history already captures them).
  - An inspector **Artboards** section lists every board (click to activate,
    trash to delete down to one), an **+ Add** button (and **Object → New
    Artboard**) tiles a fresh board sized like the active one, and a per-board
    editor renames + resizes the active board (one undo step). **View → Fit
    artboards** zooms/pans so the union of all boards fills the window.
  - **Per-artboard export** — SVG and PNG now crop to the *active* artboard's
    rectangle: the PNG canvas is the board's pixel size with the artwork
    translated by `(-x, -y)`, and the SVG sizes its `viewBox` to the board and
    wraps the body in a `translate(-x, -y)` group (omitted at the origin) — so a
    board placed anywhere on the canvas exports as its own cropped image, à la
    Illustrator's per-artboard export. **Align → To artboard** now measures
    against the active board rather than a fixed origin rectangle.
  - The fields are additive (`#[serde(default)]`), so a pre-artboards `.contour`
    loads with exactly one default 1000×700 board at the origin (its former
    behaviour); artboards round-trip through serde, and opening a file repairs a
    corrupt / hand-edited stack (always ≥1 board, active index clamped).

- **Grouping (group / ungroup)** — a new pure, unit-tested `group` module and an
  additive `group: Option<u64>` tag on every shape (`#[serde(default)]`), so
  shapes sharing an id form one group that selects, moves, transforms and arranges
  as a single unit, the way Illustrator's groups behave for day-to-day editing —
  without refactoring the flat `Vec<Shape>` document into a tree:
  - **Object → Group / Ungroup**, an inspector "Group" button row, and the
    Illustrator keys **Cmd/Ctrl + G** (group) / **Cmd/Ctrl + Shift + G**
    (ungroup). Group needs 2+ selected shapes; Ungroup lights up whenever the
    selection touches a group. Each is a single undo step.
  - **Group** assigns a fresh group id (never colliding with an existing one) to
    the selection and gathers its members into one contiguous block in paint
    order, anchored at the frontmost selected shape (so a group reads as a single
    stacked unit). The selection is remapped to the moved block so the group stays
    selected. **Ungroup** clears the tag on every member of each group the
    selection touches.
  - **Selection propagation** — clicking (or shift-clicking, marquee-selecting, or
    picking in the Layers list) any member of a group selects the *whole* group; a
    shift-click toggles the entire group in or out atomically. Dragging then moves,
    and the transform box scales/rotates, the whole group together. A leading group
    glyph marks grouped shapes in the Layers list.
  - The tag is additive (`#[serde(default)]` → `None`), so older `.contour` files
    load ungrouped and group membership round-trips through serde. Group ids
    survive `Shape::to_path` (so rotating a grouped rect/ellipse keeps it in its
    group), and boolean-op results are ungrouped.

- **Gradient fills (multi-stop, linear & radial)** — a new pure, unit-tested
  `gradient` module and an additive `fill_gradient: Option<Gradient>` on every
  filled shape (`Rect` / `Ellipse` / `Path`) that overrides the solid `fill`
  when present:
  - A `Gradient` is geometry-free — an ordered set of colour **stops** at
    parametric offsets `0..=1`, a **kind** (linear at a chosen **angle**, or
    radial), and a **spread mode** (Pad / Repeat / Reflect). The renderer maps
    the `0..=1` parameter onto the shape's bounding box (`linear_endpoints` /
    `radial_params`), so a gradient always follows the object's bounds the way
    Illustrator's gradient fills do. `color_at` interpolates between the two
    bracketing sorted stops, clamping/repeating/reflecting per the spread mode.
  - An inspector **Fill** section toggles **Solid ⇄ Gradient**; the gradient
    editor picks kind, spread and angle, and edits the **stop list** (add a stop
    at the widest gap sampled in place, recolour, drag the offset, remove down to
    two). Like the stroke section it edits the primary selected shape (one undo
    step per change) and tracks the app default so the next new shape inherits
    the fill.
  - Rendered consistently across all three surfaces: the **on-canvas** painter
    (a per-vertex gradient triangle-fan `Mesh` — exact for convex shapes, a
    faithful preview otherwise), **PNG** export (tiny-skia `LinearGradient` /
    `RadialGradient` shaders with matching spread mode), and **SVG** export
    (`<linearGradient>` / `<radialGradient>` defs in user space, one per
    gradient-filled shape, referenced via `fill="url(#…)"`; multi-stop with
    `stop-opacity` and `spreadMethod`).
  - The field is additive (`#[serde(default)]`), so older `.contour` files load
    as a solid fill; gradients round-trip through serde. Boolean-op results
    inherit the subject shape's gradient.

- **Arrange (stacking order)** — a new pure, unit-tested `arrange` module that
  reorders the flat paint-order list for the four Illustrator commands
  (**Bring to Front**, **Bring Forward**, **Send Backward**, **Send to Back**),
  always returning a true permutation that preserves the relative order of both
  the moved and the untouched shapes:
  - Wired into an **Object → Arrange** menu, an inspector "Arrange" button row,
    and the Illustrator keys **Cmd/Ctrl + ]** (forward) / **[** (backward), with
    **Shift** for to-front / to-back.
  - Multi-selections move as blocks; a command is a single undo step and the
    selection is remapped through the same permutation so the same shapes stay
    selected. No-op moves (selection already at the extreme) are disabled in the
    UI and skip the undo checkpoint.

- **Marquee (rubber-band) selection** — dragging the Select tool over empty
  canvas draws a translucent accent box and live-selects every visible shape
  whose bounding box intersects it (a new unit-tested `rects_intersect` helper).
  **Shift-drag** is additive (extends the prior selection); a plain marquee
  replaces it. The topmost intersected shape stays primary. A marquee never
  mutates the document, so it records no undo entry.

- **Snapping, guides, grid & rulers** — a new pure `snap` module (unit-tested
  nearest-target snapping over per-axis candidate coordinates) wired into the
  canvas and a new **View** menu:
  - **Rulers** (on by default) frame the canvas with top and left strips showing
    document-unit ticks at a zoom-aware "nice" step (1 / 2 / 5 × 10ⁿ), plus an
    accent cursor read-out tracking the pointer on both rulers.
  - **Ruler guides** — drag out of the left ruler for a vertical guide, the top
    ruler for a horizontal one; drag an in-progress guide back onto a ruler to
    discard it. Guides persist on the `Document` (additive
    `#[serde(default)]`, so older `.contour` files load with none) and are a
    single undo step. **View → Clear guides** removes them all.
  - **Grid** — an optional document grid at the configurable grid size (every
    fifth line emphasised), auto-hidden when it would be too dense to read.
  - **Snapping** — moving a selection, creating a shape, or dropping a guide
    snaps to any combination of the **grid**, **guides**, and **other objects'**
    edges/centres (each toggled independently in View → Snap to). The closest of
    the moving box's left/centre/right and top/middle/bottom features wins per
    axis, à la Illustrator's smart guides; the active snap lines draw in magenta.
    Snap tolerance is a fixed pixel distance pulled into document units, so it
    feels identical at every zoom. A dragged shape never snaps to itself.

- **Transform box (rotate / scale / reflect)** — a new pure `transform` module
  (a 2×3 `Affine` matrix with `scale_about` / `rotate_about` / `flip_*` pivot
  constructors, plus handle-drag → scale-factor and rotate-angle helpers; all
  unit-tested) wired into an on-canvas free-transform box and an inspector +
  **Object → Transform** menu:
  - The Select tool draws a dashed bounding box around the selection with eight
    handles (four corner + four edge). **Drag a handle** to scale — corner
    handles scale both axes, edge handles a single axis, and the *opposite*
    handle stays pinned as the pivot. **Shift-drag** a corner locks the aspect
    ratio. **Drag just outside a corner** to rotate about the box centre. Each
    drag is exact (re-applied from a start-of-drag snapshot every frame, so no
    float drift) and lands as a single undo step.
  - Inspector "Transform" section and **Object → Transform** menu add quick
    **Rotate 90° CW/CCW**, **Rotate 180°**, **Flip Horizontal/Vertical**, and a
    numeric **Rotate by** (degrees) about the selection's centre.
  - `Shape::apply_affine` transforms `Line`/`Path` in place (handles, being
    offsets, transform by the matrix's *linear* part only). Axis-aligned
    `Rect`/`Ellipse` stay their own variant under pure translate/scale/flip; a
    rotation or shear rasterises them into an editable `Path` via the new
    `Shape::to_path` (rect → four-corner polygon, ellipse → four-anchor cubic
    approximation), matching how Illustrator turns a rotated primitive into a
    path.

- **Multi-selection** — the Select tool now maintains a full selection *set*
  rather than a primary/secondary pair. Plain-click selects one shape;
  **shift-click** toggles a shape in or out of the set (in the canvas and the
  Layers list); dragging any selected shape moves the **whole selection**
  together as a single undo step. The most-recently-added shape is the *primary*
  (drives the inspector and direct-select path editing). Boolean ops now require
  exactly two selected shapes (subject = first, clip = second).

- **Align & distribute** — a new `align` module (pure, unit-tested geometry over
  bounding boxes) plus an inspector "Align" section and an **Object → Align /
  Distribute** menu:
  - **Align** the selection's left / horizontal-center / right edges and top /
    vertical-center / bottom edges to a reference frame, switchable between the
    **selection bounds** and the **artboard** (so a single shape can be centered
    on the artboard).
  - **Distribute** three-or-more shapes by evenly spacing a chosen feature (left
    / right / top / bottom edges or centers) *or* by equalising the **gaps**
    between them (horizontal / vertical "distribute spacing", à la Illustrator).
    Distribution sorts by visual position, so selection order does not matter and
    the two outermost shapes stay fixed.
  - Each align/distribute action is a single undo step; controls disable until
    the selection is large enough (2+ to align, 3+ to distribute).

- **Stroke options** — every shape now carries a `StrokeStyle` (line **cap**:
  butt / round / square; line **join**: miter / round / bevel; **miter limit**;
  and a **dash pattern** with phase **offset**), edited from a new "Stroke
  options" inspector section with dash presets (Solid / Dashed / Dotted /
  Dash-dot). The style applies to new shapes and, when a shape is selected, to
  the selection live as a single undo step. Rendered faithfully across all three
  surfaces: the on-canvas painter (dashes), **PNG** export (full caps / joins /
  miter / dashes via `tiny-skia`), and **SVG** export (`stroke-linecap`,
  `stroke-linejoin`, `stroke-miterlimit`, `stroke-dasharray`,
  `stroke-dashoffset`; default attributes omitted to keep output compact). The
  field is additive (`#[serde(default)]`), so older `.contour` files load as a
  solid butt/miter stroke.

- **Direct-select path editing** — complete on-canvas anchor/handle editing for
  `Path` shapes with the Select tool:
  - Double-click a path segment to **add an anchor** (straight segments split at
    the click point; curved segments split via de Casteljau, preserving the
    exact curve shape).
  - Double-click an anchor to **delete** it (refuses to drop below two anchors).
  - Alt-click an anchor to **convert** it between **smooth** (auto-tangent from
    neighbouring anchors) and **corner** (no handle).
  - Anchors render with Illustrator-style glyphs: round for smooth, square for
    corner.
  - A contextual "Edit path" hint appears in the inspector when a path is
    selected. All edits are a single undo step.

## [0.0.1] - 2026-06-06

### Added

- **Vector editor scaffold** — eframe/egui app shell, Prism dark theme, Phosphor
  tool-glyph icons, and an artboard canvas with cursor-anchored scroll zoom and
  drag / middle-drag pan.
- **Shapes & document model** — ordered `Vec<Shape>` (`Rect`, `Ellipse`, `Line`,
  `Path`) with hit-testing, bounds, and translation; `.contour` save (JSON via
  `serde`). Tools: Select, Rectangle, Ellipse, Line.
- **Bézier pen tool** — click-to-place anchors with drag-to-set cubic tangent
  handles; Enter / double-click closes the path. Anchor/handle dragging on a
  selected path.
- **Pathfinder v1** — Union / Intersect / Difference on closed shapes via
  `i_overlay`.
- **Export** — standalone SVG (with cubic curve commands) and PNG (rasterized via
  `tiny-skia`) sized to the artboard.
- **Layers panel** — shape list with per-layer visibility toggle, reorder
  up/down, delete, and click / shift-click selection.
- **Undo / redo** — snapshot history stack over the whole document
  (`Cmd`/`Ctrl`+`Z`, `Cmd`/`Ctrl`+`Shift`+`Z` or `Ctrl`+`Y`, plus Edit-menu
  entries); coalesces drags into single entries, drops no-op drags, capped depth.

### Changed

- Depend on the suite-level shared crate `prism-core` (was a `pigment-core` path
  dependency) for `Size`, `geometry::Rect`, and the sRGB↔linear color boundary.

[Unreleased]: https://github.com/KwaminaWhyte/prism-suite/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.6.0
[0.5.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.5.0
[0.4.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.4.0
[0.3.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.3.0
[0.2.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.2.0
[0.1.0]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.1.0
[0.0.1]: https://github.com/KwaminaWhyte/prism-suite/releases/tag/v0.0.1
