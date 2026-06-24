# Changelog

All notable changes to **Contour** (the Prism suite's vector editor) are
documented here. Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
this project is pre-1.0, so versions are `0.x` milestones and track the workspace
`version` in the root `Cargo.toml`. History before 0.9.0 lives in the git log.

## [Unreleased]

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
