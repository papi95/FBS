# FBS architecture

## Shape of the program

```
                 ┌──────────────┐
 capture ───────▶│ frame pool   │──▶ compositor ──▶ program texture ──┬──▶ preview window
 (DXGI/cam)      │ (bounded)    │      (wgpu)                          └──▶ encoder
                 └──────────────┘                                         │
                                                                          ▼
 audio (WASAPI) ─▶ mixer strips ─▶ master ───────────────────────────▶ muxer
                                                                          │
   UI (egui) ─▶ director commands ─────────────────────────────────────────┘
```

One clock owns the frame: the render thread asks the director "what composits now",
builds a single texture, then hands it to the window and to the encoder. No component
draws on its own timer, which is why transitions stay smooth on a two-core laptop.

## Crates

| Crate | Responsibility | Key dependency |
| --- | --- | --- |
| `fbs-core` | scenes, sources, buses, transitions, mixer, project files | serde only |
| `fbs-render` | wgpu device, texture pool, layer compositing, blend modes | `wgpu` |
| `fbs-capture` | desktop (DXGI), camera (Media Foundation), audio (WASAPI) | `windows` |
| `fbs-encode` | ffmpeg sidecar control, muxing, recording state | none (process) |
| `fbs-app` | window, panels, hotkeys, T-bar, scene grid | `eframe` |

`fbs-core` has no platform code and no `unsafe`, so it builds and tests anywhere.

## Why ffmpeg as a sidecar instead of a linked library

The 840 G2 has Quick Sync (Broadwell) and 8 GB of RAM. A bundled `ffmpeg.exe` with
`h264_qsv` gives hardware H.264 without adding a C toolchain to the build or shipping
LGPL/GPL static objects inside the binary. FBS pipes raw BGRA frames plus mixed PCM to
the child process, reads back progress, and owns the file. Encoding failures are
isolated to one process that can be restarted mid-show.

## Budget for the 840 G2

| Item | Target | Why |
| --- | --- | --- |
| Canvas | 1280x720 at 30 fps default, 1080p30 opt-in | 5200U iGPU composites 720p30 comfortably |
| Frame pool | 3 textures per source max | 8 GB RAM, no growth over a long show |
| Scene textures | only visible scenes are composited | preview stays cheap |
| Audio | 48 kHz stereo, WASAPI shared mode | loopback works without exclusive-mode resets |
| Disk | flat `.fbsproj` JSON, no embedded media | 500 GB HDD, quick save |
| Threads | render thread + capture threads + encoder child | leaves one core for Windows |

## Capture notes

* Desktop capture uses the DXGI Desktop Duplication API. If the driver is missing
  (common on trimmed Windows images), FBS falls back to GDI BitBlt at a lower rate and
  logs a warning in the status bar instead of failing to start.
* Camera capture goes through Media Foundation with NV12 preferred, converted on the
  GPU when possible.
* Loopback audio uses WASAPI in shared mode; the render endpoint is selected in
  Settings and stored by device ID, not by index, so replugging a headset does not
  break the mix.

## Clean-room policy

OBS Studio is GPL-2.0. FBS reimplements familiar workflows from public documentation
and observed behaviour. It does not copy OBS source, headers, or resources, and does
not link OBS binaries. If a user later wants an OBS plugin bridge, that bridge must be
a separate optional process and is not part of this workspace.
