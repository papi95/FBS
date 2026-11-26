# FBS roadmap

Ordered so the app is usable early and stays fast on an 840 G2.

## Milestone 1 — control room shell (done in this workspace)
* `fbs-core` scene graph, sources, transforms, stacking, blend modes
* Program/preview buses, fade/slide/wipe/stinger enum, T-bar with partial mix
* Mixer strips with solo, muting, peaks, clipping check
* Versioned project JSON, validation, bounded undo
* Unit tests for transitions, mixing, persistence

## Milestone 2 — picture
* `fbs-render`: wgpu surface, one texture per visible source, composite in stack order
* Layer transforms, rotation, crop, opacity
* Preview and program side by side in the UI, with safe-area guides
* Color source and image source first; no capture needed to see output

## Milestone 3 — live inputs
* DXGI desktop capture with cursor, monitor picker, GDI fallback
* Media Foundation camera with device list and format negotiation
* Nested scenes (vMix-style) and browser overlays via an offscreen WebView2
* Per-source sync offset applied at composite time

## Milestone 4 — sound
* WASAPI loopback and microphone capture
* Mixer strips bound to real streams, meter feed from `report_peak`
* Audio-follows-video on take, monitor output with latency compensation

## Milestone 5 — record and stream
* ffmpeg sidecar, `h264_qsv` with `libx264` fallback, AAC audio
* Recording health panel: dropped frames, encoder queue, disk space
* RTMP output using a stream key held in Windows Credential Manager
* Local replay buffer from a ring file

## Milestone 6 — the vMix/Smix/CastDeckLocal layer
* Multi-view grid with 4/8/16 inputs
* Instant replay with mark-in/mark-out
* Scoreboard and ticker overlays driven by project JSON
* Playlist automation, rundown stepping, remote control over HTTP + WebSocket
* CastDeckLocal-style guest link: a browser page that publishes camera/mic over WebRTC
* Hotkey map, MIDI and Stream Deck input

## Deliberately out of scope
* Plugin ABI compatible with OBS
* 4K encode on this hardware
* GPU shaders authored by users
