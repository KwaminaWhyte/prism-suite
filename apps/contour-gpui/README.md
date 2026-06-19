<p align="center">
  <img src="assets/branding/contour-master.png" width="120" alt="Contour">
</p>

# Contour

Illustrator-analog vector graphics editor and **app #2 of the Prism creative suite**
(sibling to [Pigment](https://github.com/KwaminaWhyte/prism-suite), the raster editor).

Built in Rust with [GPUI](https://www.gpui.rs/) (Zed's GPU-accelerated UI framework). Vectors
are authored through a shared `contour-app` engine (Bézier math on `kurbo`, boolean ops on
`i_overlay`, CPU rasterization on `tiny-skia`), with `contour-gpui` as the sole shipping binary.

## Status

Feature-rich and actively developed. Ships tools, pathfinder, gradients, appearance stacks,
symbols, image trace, SVG/PNG/PDF export, and a full Illustrator-parity roadmap. See
phased roadmap.

## Build & run

Requires **Rust stable** and the **Metal toolchain** (macOS).

```sh
cargo run                          # launch the editor
cargo build                        # debug build
cargo build --release              # release build
cargo test --workspace             # run all tests
cargo clippy --workspace           # lint
cargo fmt --all                    # format
```

Binary: `contour-gpui` (crate `contour-gpui`).

## Workspace layout

```
prism-suite-contour/
├── Cargo.toml                   # workspace root + shared lint config
└── crates/
    ├── contour-app/             # shared engine library (document, tools, export)
    │   └── src/
    │       ├── lib.rs
    │       ├── document/        # Shape model, history, hit-testing
    │       ├── boolean.rs       # pathfinder (i_overlay)
    │       ├── export.rs        # SVG / PNG / PDF export
    │       └── ...
    └── contour-gpui/            # GPUI host binary
        └── src/
            ├── main.rs
            ├── app_state.rs     # App struct + Action enum + apply()
            ├── canvas_host.rs   # CPU raster bridge → GPUI RenderImage
            └── panels/          # per-panel views
```

## Contributing

Fork the repo and open a PR to `main`.

```sh
# 1. Fork on GitHub, then clone your fork
git clone https://github.com/<you>/prism-suite-contour
cd prism-suite-contour

# 2. Create a branch
git checkout -b my-feature

# 3. Build and test
cargo build -p contour-gpui
cargo test --workspace

# 4. Push and open a PR against main
git push origin my-feature
```

See [CONTRIBUTING.md](./CONTRIBUTING.md) for the full checklist.

## Companion docs

- [PLAN.md](./PLAN.md) — phased feature roadmap toward ≥85% Illustrator parity
- [RESEARCH.md](./RESEARCH.md) — cited crate choices and algorithmic findings
- [CHANGELOG.md](./CHANGELOG.md) — release history
- [SUITE.md](https://github.com/KwaminaWhyte/prism-suite/blob/main/SUITE.md) — four-app Prism suite vision
