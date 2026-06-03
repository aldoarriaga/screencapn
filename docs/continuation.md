# Screen Captn Continuation Notes

## Current Architecture
- Rust remains the app engine: tray, hotkey, capture overlay, window detection, annotation model/history, clipboard/save/export, and fallback/native rendering.
- WebView2 + Konva owns the overlay front layer: toolbar, option bars, hover/selection visuals, live previews, and committed on-screen annotation rendering.
- Rust is still the source of truth for annotations. WebView renders snapshots/diffs and sends pointer/UI commands back to Rust.

## Latest Performance Work
- Phase 1 added render/event coalescing and preview node reuse for common live-drawing paths.
- Phase 2 added dirty-layer rendering and annotation node caching in the WebView renderer.
- Phase 3 added a Render Diff Protocol: Rust sends initial/forced full snapshots, then normal changes send compact render diffs with state patch plus added/updated/removed annotations.
- The first post-Phase-3 slice makes WebView apply render diffs directly to the committed Konva node cache and keeps watermark annotations visually above all other annotations.
- History now skips duplicate top checkpoints while preserving redo-branch clearing semantics.

## Operational Risks
- Do not auto-launch `screencaptn.exe` during code edits. Launch only for explicit manual smoke tests.
- Stop any running `screencaptn.exe` before build/check commands to avoid locked binaries and Defender stalls.
- Prefer direct `cargo` commands over PowerShell script execution because local execution policy may block scripts.
- If Defender continues stalling builds, consider excluding `C:\my-apps\screencapn\target` after user approval.

## Next Performance Steps
- Profile whether Rust-side diff baseline creation is still too expensive for large annotation lists, especially long pen paths.
- Consider command-based undo/history for high-volume operations after the current snapshot model is stable.
- Keep export/native renderer parity work separate from responsiveness work.