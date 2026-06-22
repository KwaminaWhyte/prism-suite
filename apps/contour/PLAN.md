# Contour — Open Source Illustrator Alternative

> **Status: ~77% parity (Batches 1–10 complete). Target ≥85%.**

## Batch 10 — Completed (2026-06-22)

- [x] **Variable Fonts & OpenType** — `VariableAxisValue` / `OpenTypeFeatures` types in `types_text.rs`; `variable_axis_values` / `opentype_features` (HashMaps) on App. `Action::SetVariableAxis/ResetVariableAxes/SetOpenTypeFeature/SetStylisticSet/ApplyAllSmallCaps`.
- [x] **Character & Paragraph Panel** — `CharacterStyle` (tracking, kerning, baseline shift, h/v scale, underline, strikethrough) / `ParagraphStyle` (alignment, spacing, indent, hyphenation, tab stops) / `KerningMode` / `ParaAlignment` types; `char_styles` / `para_styles` (HashMaps) on App. `Action::SetCharacterTracking/SetCharacterKerning/SetBaselineShift/SetHorizontalScale/SetVerticalScale/SetUnderline/SetStrikethrough/SetParagraphAlignment/SetParagraphSpacing/SetFirstLineIndent/SetHyphenation/AddTabStop/RemoveTabStop`.
- [x] **Blend Tool (depth)** — `BlendSpacing` / `BlendOrientation` / `BlendObject` types in `types_effects.rs`; `blends` / `next_blend_id` on App. `Action::MakeBlend/ReleaseBlend/ExpandBlend/SetBlendSpacing/SetBlendOrientation/ReplaceBlendSpine/ReverseBlend/ReverseBlendSpine`.
- [x] **3D Effects (Extrude & Revolve)** — `Extrude3D` / `Revolve3D` / `BevelKind` / `BevelExtent` / `SurfaceShading` / `RevolveFrom` types; `extrude_3d` / `revolve_3d` (HashMaps) on App. `Action::Apply3DExtrude/Update3DExtrude/Remove3DEffect/Apply3DRevolve/Update3DRevolve/Set3DLighting/Set3DPerspective`.
- [x] **PDF Export State** — `PdfExportConfig` / `PdfStandard` / `PdfCompatibility` / `PdfColorSpace` / `PdfMarks` / `PdfLayerVisibility` types; `pdf_export_config` on App. `Action::SetPdfStandard/SetPdfCompatibility/SetPdfEmbedFonts/SetPdfFlattenTransparency/SetPdfColorSpace/SetPdfBleed/SetPdfMarks/SetPdfPassword/SetPdfPermissions/ExportAsPdf`.
- 45 tests added → **712 total**

## Batch 9 — Completed (2026-06-20)

- [x] **Type on Path depth** — `TextOnPathSpacing` enum (Auto/Fixed/Optical); `text_on_path_offsets/above/spacing` (HashMaps) on App. `Action::SetTextOnPathOffset/SetTextOnPathSide/SetTextOnPathSpacing/FlipTextOnPath`.
- [x] **Recolor Artwork depth** — `RecolorConfig { harmony_rule, preserve_black/white, randomize, brightness_scale }`; `recolor_config/recolor_color_count/recolor_history` on App. `Action::SetRecolorConfig/SetRecolorColorCount/SetRecolorPreserveBlack/SetRecolorPreserveWhite/RandomizeRecolor/SaveRecolorSet/ApplyRecolorToSelected`.
- [x] **Live Paint depth** — `live_paint_gap_detection/highlight_color/group_ids` on App. `Action::LivePaintFill/LivePaintStroke/MakeLivePaintGroup/ReleaseLivePaintGroup/ExpandLivePaintGroup/SetLivePaintGapDetection/SetLivePaintHighlightColor`.
- [x] **Symbol Sprayer** — `SymbolSprayConfig { density, diameter, scatter }`; `symbol_spray_config` on App. `Action::SetSymbolSprayConfig/SpraySymbols/SetSymbolSprayDensity/SetSymbolSprayDiameter/SymbolShift/SymbolScale/SymbolSpin/SymbolStain/SymbolScreen`.
- 13 tests added → **614 total**

## Batch 8 — Completed (2026-06-19)

- [x] **Image Trace (extended)** — `ImageTraceMode` enum (Color/Grayscale/BlackWhite/Outlined); `image_trace_mode/threshold/colors/expanded` on App. `Action::SetImageTrace/ApplyImageTrace/ExpandImageTrace`.
- [x] **Opacity Masks** — `omask_id_counter` on App; `omask/omask_path/omask_invert` on Shape. `Action::MakeOpacityMask/ReleaseOpacityMask/InvertOpacityMask`.
- [x] **Symbol extras** — `Action::BreakSymbolLink/ExpandSymbol` stubs.
- [x] **Graph Tool** — `GraphType` enum (Column/Bar/Pie/Line/Scatter) + `GraphData { graph_type, cols, rows, values, labels }`; `graph_data/graph_style_fill/graph_show_legend` on App. `Action::SetGraphType/SetGraphData/SetGraphLabels/ApplyGraph/SetGraphStyleFill/ToggleGraphLegend`.

## Batch 7 — Completed (2026-06-19)

- [x] **Art Brush** — polyline sampling + `Shape::path()` constructor; `Action::PaintArtBrushPath`.
- [x] **Live Corners** — `Action::SetPolygonCornerRadius`; `corner_radius: f32` added to `LiveShape::Polygon`.
- [x] **Perspective Grid** — `PerspGrid` struct; `Action::TogglePerspGrid/SetPerspGridPreset/SetPerspGrid`.
- [x] **Color Guide** — `ColorGuide { harmony, swatches }`; `Action::SetColorHarmony/ApplyColorGuide`.
- [x] **Warp Tools** — pucker/bloat/scallop/crystallize/wrinkle; `Action::ApplyWarpStroke`.

## Batch 1–6 — Foundation

Core document model, path/anchor editing, boolean operations, Pathfinder, pen tool, type-on-path, pattern fills, variable-width stroke, opacity, layers, symbols, gradient editor, align/distribute, the full live-shape engine.

---

## Child Windows & Secondary UI

Contour has no child windows at all in the current implementation. All secondary UI is delivered as floating egui panels inside the single OS window. The items below define the full secondary-window surface needed to reach parity with Illustrator's dialog and palette model. Because Contour uses **eframe/egui** (not GPUI), secondary windows are implemented as `egui::Window::new("name").show(ctx, |ui| …)` floating panels within the same OS window rather than true OS-level child windows — unless Contour is migrated to GPUI in a future phase, at which point `cx.open_window(...)` applies.

### Welcome Screen
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (900×560 px) shown on launch when no document is open
- **Phase:** earliest unimplemented phase — add to Batch 11 or the next available batch
- Recent files list, New Document button (triggers Document Setup), template thumbnail grid (A4/Letter/Web 1920/Social), Open… button. Dismissed when a document is created or opened. Matches Illustrator's startup panel.

### Document Setup
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (520×400 px)
- **Phase:** Batch 11 / next batch
- Artboard dimensions (width, height, units: px/mm/in/pt), orientation toggle, color mode (RGB/CMYK), raster effects resolution (72/150/300 ppi), bleed fields. Opens on File ▸ Document Setup… and from the Welcome Screen "New" flow.

### Export Window
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (640×480 px)
- **Phase:** Batch 11 / next batch
- Format tabs: SVG / PDF / PNG / EPS. Per-format options (SVG: embed/link images, CSS properties; PDF: standard, flatten transparency; PNG: resolution, anti-alias; EPS: version). Asset export panel for multiple-artboard / multiple-scale export. Opened from File ▸ Export As… and File ▸ Export for Screens….

### Color Picker
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (280×360 px)
- **Phase:** Batch 11 / next batch
- HSB, HSL, RGB, Hex, CMYK modes; color swatches; eyedropper. Tear-off style — stays visible while editing paths. Syncs with the fill/stroke selector in the toolbar. Mirrors Illustrator's Color panel.

### Preferences
- **Kind:** `WindowKind::Floating` equivalent — egui floating panel (720×560 px)
- **Phase:** Batch 12 or later polish phase
- Tabs: General (undo levels, scale strokes/effects, double-click to isolate), Selection & Anchor Display (tolerance, snap radius), Type (glyph options, missing-font substitute), Units & Increments (ruler units, nudge amounts), Guides & Grid (color, style, spacing), Plug-ins & Scratch Disks, User Interface (UI brightness, canvas color). Persisted to `~/.config/prism/contour_prefs.json`.

### Implementation notes
- Until Contour is migrated to GPUI, all windows are `egui::Window` floating panels with `collapsible(false)`, `resizable(true)`, and a fixed default `Pos2` computed from the screen center.
- The Welcome Screen panel is shown only when `app.document.is_none()` and dismissed by setting an `app.welcome_dismissed` flag.
- Each panel's open/closed state is tracked in `AppState` (e.g. `show_export_window: bool`, `show_document_setup: bool`) and toggled via menu actions.
- When Contour eventually migrates to GPUI, each panel above maps 1:1 to a `WindowKind::Floating` `cx.open_window(...)` call with the sizes listed above.
