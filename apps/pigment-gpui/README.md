<p align="center">
  <img src="assets/branding/pigment-master.png" width="120" alt="Pigment">
</p>

<h1 align="center">Pigment</h1>

<p align="center">
  <b>An open source, GPU-accelerated, non-destructive raster image editor — the Photoshop analog in the Prism Suite.</b><br>
  Built in Rust.
</p>

---

Pigment is app #1 of the **[Prism Suite](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md)** — four
interoperating creative apps (raster, vector, video, motion) that work together
the way Adobe's Creative Cloud does. It is under active construction.

## Status

**Batch 2 complete** — the app builds, launches, renders a full GPU compositor,
and has near-complete Photoshop parity. See [PLAN.md](./PLAN.md) for the full
roadmap and [ARCHITECTURE.md](./ARCHITECTURE.md) for current internals.

| Milestone | State |
|-----------|-------|
| Phase 0 — GPU canvas, pan/zoom, open image | ✅ done |
| Phase 1 — layers, blend modes, brush/eraser/fill, wet-layer, undo+history, save | ✅ done |
| Phase 2 — selection, lasso/wand, transform, crop, resize, copy/paste, icon UI | ✅ done |
| Phase 3 — adjustment layers, masks, filters, HSL blends, histogram | ✅ done |
| Phase 4 — text layers, vector shapes, gradient (pen/smart-objects deferred) | ✅ mostly |
| Phase 5 — PSD import, EXR open, image export (color-mgmt/AI/plugins deferred) | ✅ interop done |
| Phase 5 — color mgmt, PSD, AI, plugins | ⏳ planned |

## Design principles

- **GPU-resident** — wgpu (Vulkan/Metal/DX12/WebGPU) compositor from day one.
- **Non-destructive** — layer tree / node graph; pixels re-derived and cached.
- **Linear-light, premultiplied** — correct color math everywhere; ICC/OCIO.
- **Sparse tiles** — paint huge documents; only touched tiles allocate.
- **Polish over feature count** — the engine is free; the product is the feel.

## Tech stack

`wgpu` 29 · `gpui` 0.2.2 · `winit` 0.30 · `image` 0.25 · `lcms2` ·
`cosmic-text` · `kurbo`/`lyon` · `ort` (ONNX) · `fast_image_resize`.
Full matrix + rationale in [RESEARCH.md](./RESEARCH.md).

## Workspace layout

```
crates/
  pigment-gpui/   GPUI desktop app + wgpu canvas host
assets/shaders/   WGSL
```

## Running

Requires Rust stable (1.88+).

```bash
cargo run
```

File → Open loads an image; drag to pan, scroll to zoom, View → Fit to screen.

## Building

```bash
cargo build
```

## Testing

```bash
cargo test --workspace
```

## Contributing

1. Fork the repo and branch off `main`.
3. Run `cargo test --workspace` — all tests must pass.
4. Open a pull request against `main`.

Shared engine logic (blend math, tile model, color transforms, file containers)
belongs in the shared crates inside `prism-suite-prism`, not here. See
[CONTRIBUTING.md](./CONTRIBUTING.md) for full details.

## Roadmap

- [PLAN.md](./PLAN.md) — phased, actionable task backlog for Pigment.
- [RESEARCH.md](./RESEARCH.md) — cited research backing the technical choices.

## License

Dual-licensed under MIT or Apache-2.0.
