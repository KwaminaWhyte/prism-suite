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
