# Pigment — Architecture

Module and data-flow reference. High-level rationale lives in [PLAN.md](./PLAN.md) §2;
cited research in [RESEARCH.md](./RESEARCH.md).

> This document was last updated at Batch 2 (2026-06-18). The egui/eframe
> `pigment-app` binary has been removed; `pigment-gpui` is the sole binary.

---

## Current workspace

```
prism-suite-pigment/
├── Cargo.toml              # workspace (resolver = "2")
└── crates/
    └── pigment-gpui/       # GPUI desktop binary
        └── src/
            ├── main.rs         # Application::new() + window setup
            ├── app.rs          # App state + Action dispatch
            ├── canvas_host.rs  # CanvasHost: owns wgpu device, bridges prism-canvas
            └── panels/         # tool strip, layers, color, histogram, adjustments, …
```

Shared engine crates (path-dep'd from `../shared/`):

| Crate | Contents |
|---|---|
| `prism-canvas` | `CanvasGpu`, compositor, filter passes, WGSL shaders, brush engine |
| `prism-core` | Document, LayerTree, Tile model (sparse COW), CommandStack, blend math, adjustments, color |
| `prism-color` | ICC profiles, color space conversions, OCIO |
| `prism-io` | PNG/JPEG/WebP/TIFF/EXR/PSD load+save, text rasterization, document file format |
| `prism-ui` | Design tokens, icon set |

---

## Frame data flow

```
GPUI event loop
  └─ pigment-gpui App::update(cx)
       ├─ dispatch Action (tool input, menu, keyboard)
       ├─ CanvasHost::composite_and_bridge()   ← prism-canvas CanvasGpu
       │    ├─ wgpu: composite layers → Rgba16Float texture
       │    ├─ copy_texture_to_buffer (CPU readback, ~3.5ms @ 8.7MP)
       │    └─ convert linear-premul → BGRA8 sRGB → RenderImage
       └─ GPUI renders chrome + RenderImage in canvas viewport
```

Idle frames skip the readback: `CanvasHost` holds a dirty flag set by any
`Action` that mutates the document; the bridge only runs when dirty.

---

## Key data structures

### Document (in `prism-core`)
- **Document** = canvas size, color profile, `LayerTree`, active selection mask.
- **LayerTree** = recursive: `Layer { Raster | Group | Adjustment | Text | Vector | SmartObject }`,
  each with blend mode, opacity, mask, visibility, smart filters, layer styles.
- **Tile** = 256×256 RGBA16F **linear premultiplied**. Sparse `HashMap<(layer_id, tx, ty), Arc<Tile>>`.
  COW: edits clone only touched tiles → cheap undo + layer clones.
- **CommandStack** = every edit is a reversible `Command`. Pixel ops store pre-edit
  dirty-tile copies (Arc-shared). Structural/param edits store small graph deltas.

### CanvasGpu (in `prism-canvas`)
- Owns all wgpu resources: layer textures (`Rgba16Float`), ping-pong composite buffers,
  filter passes, brush dab pipeline, selection mask texture.
- Compositor ping-pongs over two targets; each visible layer blends backdrop + layer
  per the layer's blend mode, opacity, adjustment, mask, and layer styles.
- Display pass: composite → tonemap/sRGB-encode → copy to bridge buffer.

### CanvasHost (in `pigment-gpui`)
- Owns the wgpu `Device` + `Queue` (separate from GPUI's blade-graphics device).
- Holds a `CanvasGpu` and a `Document`.
- `composite_and_bridge()` drives the GPU pass and copies the result into a
  `Vec<u8>` (BGRA8) that becomes a GPUI `RenderImage`.
- Action handlers mutate `Document` through `CanvasGpu`'s API (brush dabs, fill,
  selection ops, undo/redo, etc.) and set the dirty flag.

---

## Brush engine

- Input: GPUI pointer events → mapped to doc-space coordinates via the canvas
  viewport bounds (tracked via a `canvas()` prepaint callback).
- Arc-length dab walker: emits a dab every `spacing × radius` along the drag;
  Catmull-Rom interpolation through pointer samples.
- **Wet layer**: in-progress stroke renders to a dedicated `wet` texture composited
  above its owner layer live; flattened (wet→dry) on pointer release.
- Undo: `begin_command_now` snapshots the layer's dirty region before the stroke;
  `commit_command` trims the snapshot to the actual dirty rect.

---

## Compositor pipeline (per frame, driven by CanvasGpu)

1. For each visible layer bottom-to-top:
   a. Source pass — sample the layer's texture (or wet buffer).
   b. Blend pass — blend with the running composite using the layer's blend mode.
   c. Adjustment pass — if the layer is an adjustment layer, apply its LUT/matrix.
   d. Mask pass — multiply the layer alpha by its raster mask (if present).
   e. Layer style passes — stroke, drop shadow, glow, overlay, bevel computed from
      the layer's alpha and the composite backdrop.
2. Display pass — sRGB-encode the final composite into the bridge buffer.

All passes run on wgpu compute or render shaders in `prism-canvas/src/shaders/`.

---

## File format (`.pigment`)

`b"PIGMENT1"` magic + JSON `DocMeta` (layers, adjustments, styles, smart filters,
comps, prefs) + per-layer lz4-compressed RGBA16F pixel blobs. Opened via
`prism_io::document_file`; saved by GPU readback per layer.

---

## Testing approach

- **Unit tests**: pure CPU math in `prism-core` and `prism-canvas::filter_math`
  (blend/adjust/histogram/fill/raster/gradient/shape, filter kernel math).
- **IO round-trips**: `prism-io` serde + pixel round-trips for every layer kind.
- **Headless GPU tests**: `#[cfg(test)]` blocks that boot a real wgpu device via
  `pollster::block_on(wgpu::Instance::new(...).request_adapter(...))` and assert
  pixel-exact results for compositor, brush, filters, and transforms.
  Convention: `skip_if_no_adapter()` so `cargo test` stays green in CI without a GPU.
