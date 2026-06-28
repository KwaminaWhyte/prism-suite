# Changelog

All notable changes to **Contour** (the Prism suite's vector editor) are
documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project is pre-1.0, so versions are `0.x` milestones and track the workspace
`version` in the root `Cargo.toml`. History before 0.9.0 lives in the git log.

## [Unreleased]

## [0.14.0] - 2026-06-28

### Added — Width Tool / variable-width stroke profiles (real outline geometry)
- **Variable-width profiles** — a `WidthProfile` is an ordered list of width
  points, each with an arc-length position (`t`) plus independent **left/right**
  half-widths (asymmetric strokes); the half-width is **smoothstep**-blended
  between points.
- **`stroke_outline`** — flatten a path's centreline, sample the per-vertex
  tangent/normal, offset each side by the interpolated half-width, and build a
  single closed, fillable outline contour (left forward + right reversed) with
  **butt / round / square** caps at open ends; closed paths yield an annulus.
- **Profile presets** — Uniform, Width Profile 1 (taper both ends), 2 (taper to
  start), 3 (taper to end), and Bulge → `WidthApplyPreset`.
- **Width-point editing** — add / move (clamped between neighbours) / delete /
  set-widths via `WidthPointAdd` / `WidthPointMove` / `WidthPointDelete` /
  `WidthPointSetWidths`.
- **Expand Stroke** — bake the variable-width outline into a real filled
  `Shape::Path` (fill = old stroke colour, no stroke) → `WidthExpandStroke`.
- All implemented as pure, unit-tested geometry in `app_state/width_tool/`.

### Changed — File organization
- Extracted the `Tool` enum + its `label`/`glyph`/`ALL` metadata out of
  `app_state/mod.rs` into `app_state/tool.rs` to keep `mod.rs` under the
  ~1000-line limit (1020 → 902).

## [0.13.0] - 2026-06-28

### Added — Distort & Transform family (real path geometry)
- **Offset Path** — parallel-offset a closed outline outward/inward with
  miter/round/bevel corner joins (open paths get a normal offset) →
  `DistortOffsetPath`.
- **Roughen** — subdivide segments to a detail count then jitter every point by
  a size amount, driven by a **seeded SplitMix64** PRNG (fully deterministic, no
  globals); optional smoothing → `DistortRoughen`.
- **Zig-Zag** — convert each segment into N alternating ridges (corner) or a
  wave (smooth) → `DistortZigZag`.
- **Pucker & Bloat** — push anchors toward/away from the path centroid while
  pulling bezier handles oppositely → `DistortPuckerBloat`.
- **Twist** — swirl points about the centroid by an angle scaling with distance
  from centre → `DistortTwist`.
- **Transform Each** — per-copy scale/move/rotate about each shape's centre with
  optional replication → `DistortTransformEach`.
- All implemented as pure, unit-tested geometry functions in
  `app_state/path_distort.rs`. +33 tests (833 → 866).

## [0.12.0] - 2026-06-24

### Added — Multi-line + comprehensive text input
- **Typeable inspector numerics** — X/Y/W/H, stroke width, opacity, rotate-by, font size (steppers kept).
- **Multi-line text objects** — on-canvas type editor upgraded to `prism_ui::TextArea` (newlines preserved; Cmd+Enter / click-away commits).
- **Layers filter box** — `TextField` filters layer rows by name → `SetLayerFilter`.
- +12 tests (821 → 833).

## [0.11.0] - 2026-06-24

### Added — Real text input (`prism_ui::TextField`)
- **Document Setup dimensions** — typeable width/height/bleed (unit-aware → points) → `SetDocSetupSize`/`SetDocSetupBleed`.
- **Color-picker hex** — `#RRGGBB` field → `SetPickerHex`.
- **Export path** — full output-path field (extension follows format) → `ExportDocument`.
- **Rename** — layer/symbol/artboard inline rename via double-click → `RenameLayer`/`RenameSymbol`/`RenameArtboard`.
- **Editable text-object content** — Type-tool field re-shapes glyphs live → `SetTextObjectContent`.
- +8 tests (813 → 821).

## [0.10.0] - 2026-06-24

### Added — UI (GPUI floating windows)
- **Document Setup** window — width/height/unit/color-mode/bleed →
  `SetDocSetup*` + `NewDocumentFromSetup`.
- **Export** window — PNG/SVG/EPS/PDF format picker + path → `ExportDocument`.
- **Color Picker** window — RGB/HSB/CMYK/Hex bound to the picker state via
  `SetPicker*`; "Apply to Selection" → `ApplyPickerToSelection`.
- **Preferences** window — undo/snap/grid/unit → `SetPref*`.
- All child windows use `WindowKind::Floating`; UI-only (emits existing 0.9.0
  actions, no new state).

## [0.9.0] - 2026-06-24

### Added — Feature waves 1–3 (parity push)
- **Outline Text** — convert a text object into a compound of glyph outline
  contours (even-odd so counters stay holes).
- **Pathfinder** — full set wired to the `i_overlay` boolean pipeline: Unite,
  Minus, Intersect, Exclude, Divide, Trim, Merge, Crop, Outline, Minus-Back.
- **Perspective distort** — 4-corner homography warp of selected geometry.
- **Envelope distort** — warp-preset mesh (15 Illustrator warp styles) deforming
  anchors + bezier handles; expand bakes the warp (`app_state/geometry_warp.rs`).
- **Live Paint** — fill/stroke a bounded region detected from overlapping path
  outlines under a hit point.
- **Image Trace (real)** — raster→vector pipeline: threshold/posterize → Moore
  boundary tracing → Douglas–Peucker simplify → speckle filter (`trace_contour.rs`).
- **Pattern brush** — tile instances placed and oriented along a path's tangent at
  spacing intervals (`pattern_brush.rs`).
- **Chart / graph tool** — Column/Bar/Stacked/Line/Pie geometry + label anchors
  from chart data (`graph_gen.rs`).
- **Document setup** — artboard size/unit/color-mode/bleed model; new-document
  sizes its artboard from the setup.
- **Symbol-edit mode** — enter a symbol's definition, edit master shapes in place,
  propagate to all instances on exit.
- **Extrude 3D + Revolve 3D (CPU)** — extrude/lathe a path to a mesh, project
  (perspective/ortho), depth-sort + flat-shade, and expand to flat vector faces
  (`extrude3d.rs`).
- **Gradient mesh** — grid of color nodes with bilinear/Coons interpolation;
  add/move nodes, set node color, tessellate for preview (`gradient_mesh.rs`).
- **Export formats** — PNG / SVG / EPS (PostScript) / single-page PDF writers
  (`export_formats.rs`).
- **Color picker + preferences** — HSB/RGB/CMYK/Hex picker with conversions;
  preferences (undo levels, snap, grid, units) with JSON load/save
  (`app_state/prefs_color.rs`).

### Changed — File organization
- New apply logic lives in dedicated domain files (`apply_batch11.rs`,
  `apply_batch12.rs`, `apply_batch13.rs`) chained from the action dispatcher; no
  source file exceeds the ~1000-line limit.
