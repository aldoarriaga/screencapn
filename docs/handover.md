# Screen Captn Handover

Date: 2026-06-03
Project path: `C:\my-apps\screencapn`
Branch: `main`
Remote: `origin/main`

## Product Summary
Screen Captn is a Rust-native Windows screenshot and annotation app with a WebView2/Konva front layer.

Rust remains authoritative for:
- tray app, global hotkey, overlay lifecycle
- capture/window detection/region selection
- annotation model, undo/history, clipboard/save/export
- native fallback/export rendering

WebView2 + Konva owns the current front-end experience:
- toolbar and option bars
- hover/pressed/selection visuals
- live previews
- committed on-screen annotation rendering
- theme drawer and UI animation

## Current Git State
Latest pushed commits:
- `705f89c Add dirty-layer WebView rendering`
- `0495e72 Improve web annotation renderer and performance path`
- `7097161 Add WebView annotation UI migration`
- `e2b40eb Baseline Screen Captn implementation`

Current uncommitted files:
- `crates/screencaptn-core/src/history.rs`
- `crates/screencaptn-win/assets/web-ui/app.js`
- `crates/screencaptn-win/src/overlay.rs`
- `scripts/dev-check.ps1`
- `docs/continuation.md`
- `docs/handover.md`

## Latest Completed Work
### Performance Phase 3: Render Diff Protocol
Implemented but not yet committed.

Rust now sends:
- full snapshot on overlay open
- full snapshot when WebView sends `ready`
- full snapshot after undo recovery
- compact `renderDiff` after normal add/update/remove/style/selection changes

WebView now handles:
- full `state` snapshots as before
- `renderDiff` with state patch plus `added`, `updated`, and `removed` annotations
- direct committed-node cache updates for changed annotations

### Watermark Ordering
Watermark now renders on top of all annotations regardless of creation order:
- WebView committed annotation z-order puts watermark annotations last
- native overlay fallback paints watermark after annotations
- export/copy/save paints watermark after annotations

### Performance Slice After Phase 3
Implemented low-risk improvements:
- render diffs update only changed committed Konva nodes instead of rebuilding all committed annotations
- consecutive diffs merge against pending state to avoid stale state during same-frame updates
- undo history skips duplicate top checkpoints while preserving redo-branch clearing

### Safety/Workflow
`scripts/dev-check.ps1` now stops any running `screencaptn.exe` before checks.

A smaller continuation note also exists at:
- `docs/continuation.md`

## Validation Already Run
These passed after the latest changes:

```powershell
Stop-Process -Name screencaptn -Force -ErrorAction SilentlyContinue
node --check "C:\my-apps\screencapn\crates\screencaptn-win\assets\web-ui\app.js"
& "$env:USERPROFILE\.cargo\bin\cargo.exe" fmt --all
& "$env:USERPROFILE\.cargo\bin\cargo.exe" check
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test -p screencaptn-core
```

Core tests currently pass: `8 passed`.

The app was intentionally not launched during the latest implementation to avoid Windows/Defender/WebView stalls.

## Manual Tests Still Needed
Before committing/pushing, manually verify:
- launch app and open capture overlay
- select region/window/fullscreen
- draw rectangle, oval, line, arrow, pen, highlighter, mosaic, text, tag
- edit/delete/undo annotations
- add watermark before and after other annotations
- confirm watermark stays visually on top on-screen
- copy/save and confirm watermark is also on top in exported output
- confirm no duplicate shapes, stale selection, broken toolbar, or delayed committed rendering

## Known Operational Risk
The system has previously stalled when commands launched or interacted with the app while Defender/WebView/build artifacts were active.

Safer rules:
- do not auto-launch `screencaptn.exe` while editing code
- stop `screencaptn.exe` before build/check/test commands
- avoid Computer Use unless explicitly requested
- prefer direct `cargo` commands over `dev-check.ps1` because local PowerShell execution policy may block scripts
- if Defender continues stalling builds, consider a Defender exclusion for `C:\my-apps\screencapn\target` after explicit user approval

## Next Natural Step
1. Run the manual smoke test above.
2. If it passes, commit and push the current uncommitted work.
3. Next PRD performance target: profile whether Rust-side diff baseline creation is still too expensive for large annotation lists and long pen paths.
4. Larger future slice: command-based undo/history for high-volume operations, but only after snapshot/diff behavior is stable.

## Important Architecture Reminder
Do not rewrite the current UI experience. The user likes the current front-end feel. Future work should preserve the toolbar, option panels, theme drawer, animation style, and annotation visuals unless the user asks for a design change.