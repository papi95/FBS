# FBS — working notes

Free Broadcast Studio: an original Rust live-production app for Windows, aimed at the
HP EliteBook 840 G2 (i5-5200U, 8 GB RAM, 500 GB HDD, Windows X-Lite 11 Pro).

## Layout

* `crates/fbs-core` — pure model: scenes, sources, buses, transitions, mixer, project
  files. No `unsafe`, no platform code. `cargo test -p fbs-core`.
* `crates/fbs-render` — wgpu compositor: layers, blends, transitions, pixel tests.
  `cargo test -p fbs-render`.
* `crates/fbs-app` — egui/eframe control room. `cargo run -p fbs-app`.
* `docs/ARCHITECTURE.md`, `docs/ROADMAP.md` — design and milestone plan.

## Decisions worth remembering

* **Clean room.** OBS Studio is GPL-2.0. Do not copy OBS source or link OBS binaries.
  Familiar workflows are reimplemented from observed behaviour and public docs.
* **egui pinned to 0.29.** `Margin::symmetric` and `Rounding::same` take `f32` here,
  not `i8` as in 0.31+. `Frame::none()` and `.rounding()` are the 0.29 spellings.
  Stroke widths are written as `1.0_f32` to avoid float-literal fallback warnings.
* **Panels paint their own frames.** Nested `rect_filled` calls stand in for
  `rect_stroke`, whose signature shifted across egui versions.
* **Mixer strips use a content-sized `egui::Frame`**, never a fixed-height rect with
  widgets drawn on top; the earlier version let controls spill outside the strip.
* **720p30 is the default canvas** and `Canvas::HD720` is what the demo uses. Disk
  format is flat JSON (`.fbsproj`), no embedded media.
* The build cache must live outside the workspace (`.gitignore` has `target/`) because
  a full eframe target dir blows the project storage budget.

## Build and verify on Linux (this sandbox)

```
export PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=/tmp/cargo-target
cargo test -p fbs-core
cargo build -p fbs-app
```

Headless UI check: run `Xvfb :99 -screen 0 1440x900x24`, then
`DISPLAY=:99 LIBGL_ALWAYS_SOFTWARE=1 ./fbs`, and capture with
`DISPLAY=:99 import -window root shot.png`. Needs `libxkbcommon-x11-0`.

Cross-compile to Windows from here (works, takes ~2.5 min per crate):

```
sudo apt-get install -y gcc-mingw-w64-x86-64 zip
rustup target add x86_64-pc-windows-gnu
CARGO_TARGET_DIR=/tmp/win cargo build --release -p fbs-app --target x86_64-pc-windows-gnu
CARGO_TARGET_DIR=/tmp/win cargo build --release -p fbs-render --examples --target x86_64-pc-windows-gnu
```

Binaries land in `/tmp/win/x86_64-pc-windows-gnu/release/`. Copy them to
`dist/windows/`, refresh the ZIP, and ship it. They depend only on stock Windows DLLs,
so nothing needs installing on the laptop. Verify with
`x86_64-w64-mingw32-objdump -p <exe> | grep 'DLL Name'`.

## Shipping the Windows build

`fbs-windows.zip` in the project root is the deliverable; `dist/windows/` holds its
contents. `download/` is a tiny static page served by the preview so the ZIP has a real
URL (the workspace file browser is unreliable for binaries). Keep `download/fbs-windows.zip`
a hard link to the root ZIP so the two never drift.

## Gotchas already paid for

* wgpu 22: `RenderPipelineDescriptor` needs `cache: None`, and `VertexState`/`FragmentState`
  need `compilation_options`. `Backends::from_env` does not exist — use the local
  `backends_from_env()`.
* WGSL: `target` is a reserved word. Use `target_px`.
* Uniform structs containing `vec3<f32>` are 32 bytes, not 16. Under-allocating makes
  wgpu fail the draw at validation, not at buffer creation.
* `wgpu::Buffer` and `wgpu::BindGroup` are not `Clone` in wgpu 22 — pool them in `Vec`
  slots instead of caching clones.
* A render pass needs a target the shader cannot also sample. Multiply/Screen therefore
  use scratch + snapshot + combine passes; only pay for it when a scene asks.
* Software Vulkan needs `mesa-vulkan-drivers` installed or `Gpu::new()` finds no adapter.
  Test with `--test-threads=1`; llvmpipe is happier that way.

## Not built yet

Capture (`fbs-capture`: DXGI Desktop Duplication, Media Foundation camera, WASAPI audio),
encoder sidecar (`fbs-encode`), and feeding real frames into the app's monitors.
