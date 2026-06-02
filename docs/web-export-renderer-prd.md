# PRD: Web-Owned Annotation Export Renderer

## Summary

Screen Captn currently has a strong WebView/Konva front-end experience for toolbar interaction, option menus, live previews, committed on-screen annotations, animation, hover states, and visual polish. Rust remains the Windows engine and owns screen capture, tray, global hotkey, window detection, file dialogs, clipboard, save, history, and the authoritative annotation model.

The next target is to make the WebView/Konva layer own final visual annotation rendering for export as well, so copy/save output matches the on-screen editor more closely. Rust should continue to own native Windows responsibilities and data authority. The current Rust/GDI annotation renderer should remain as a fallback until the Web export path proves stable.

This is a controlled renderer ownership migration, not a UI redesign.

## Goals

- Preserve the current front-end experience exactly where it is already working well.
- Make exported images match the current WebView/Konva on-screen annotation visuals.
- Reduce duplicated drawing semantics between Rust and JavaScript.
- Keep Rust as the native Windows engine for capture, tray, hotkeys, window detection, clipboard, save dialogs, and persistence.
- Keep the app lightweight and responsive on normal DPI, high DPI, and 4K displays.
- Keep rollback simple by retaining the Rust renderer as fallback during migration.

## Non-Goals

- Do not redesign the toolbar, option bars, theme drawer, hover effects, or interaction model.
- Do not replace Rust as the Windows host.
- Do not migrate to Electron, Tauri, React, or a new application shell.
- Do not change existing annotation behavior unless required to preserve export parity.
- Do not remove the Rust renderer until Web export is proven reliable.
- Do not change window detection, region selection, hotkey, tray, or app lifecycle behavior.

## Current State

### Rust Responsibilities

- Tray app and global hotkey.
- Capture overlay window.
- Screen capture and background bitmap ownership.
- Window, monitor, and client-area detection.
- Capture region selection.
- Annotation document state.
- Undo/history.
- Clipboard and save/export.
- Native file dialogs.
- Theme persistence.
- WebView2 host lifecycle.
- JSON state sync to WebView.
- Fallback/native annotation rendering.

### WebView/Konva Responsibilities

- Toolbar rendering.
- Option submenu rendering.
- Light/dark visual style.
- Theme drawer interaction.
- Toolbar hover glow and active states.
- Live annotation previews.
- Committed on-screen annotation rendering.
- Selection handles.
- Pointer routing back to Rust.
- Web UI animation.

### Current Pain

- Rust and JavaScript both know how to draw annotation-like objects.
- Rust export can drift from what the user sees in WebView.
- Visual constants are split across Rust, JavaScript, SVG assets, and renderer-specific helpers.
- Export currently depends on the Rust/GDI renderer, which is harder to polish than Konva/canvas.

## Product Principle

The editor experience that exists today is the product baseline. Migration work must be invisible to the user except for improved export fidelity.

If any phase makes drawing feel slower, changes toolbar behavior, breaks hover/selected states, or changes annotation creation/editing, that phase should not ship.

## Target Architecture

### Ownership

Rust remains the app engine and source of truth.

WebView/Konva becomes the visual renderer for:

- On-screen committed annotations.
- Live annotation previews.
- Export image composition.

Rust remains responsible for:

- Native capture.
- Native window detection.
- Native clipboard integration.
- Native save/open dialogs.
- Filesystem operations.
- App lifecycle.
- Annotation model authority.
- History/undo.
- Fallback export rendering.

### Target Export Flow

1. User clicks Copy, Save, or presses Enter.
2. Rust freezes the current authoritative document state.
3. Rust sends WebView an export request containing:
   - Capture region.
   - Captured background image.
   - Annotation snapshot.
   - Render style snapshot.
   - Export scale.
   - Output format request.
4. WebView renders the final capture into an offscreen canvas.
5. WebView returns PNG bytes or base64 PNG data to Rust.
6. Rust writes the PNG to clipboard or file.
7. If Web export fails or times out, Rust uses the existing native fallback renderer.

## User Experience Requirements

### Preserve Existing Editor Behavior

- Toolbar position, size, theme, hover effects, and active states must remain unchanged.
- Option bars must stay visually consistent and responsive.
- Region selection must remain unchanged.
- Window/fullscreen/client detection behavior must remain unchanged.
- Drawing rectangle, oval, line, arrow, pen, highlighter, text, tag, mosaic, watermark, and numbering must behave as they do today.
- Keyboard behavior must remain unchanged.
- Esc must continue to cancel current operation/overlay as currently implemented.
- Copy/save should not feel slower than today in normal use.

### Export UX

- Copy/save should feel immediate.
- If Web export takes longer than the target threshold, fallback should occur silently where possible.
- If both Web and fallback export fail, the app should fail gracefully without losing the overlay state.

## Functional Requirements

### Export Request

Rust must be able to request a WebView export with:

- Export request id.
- Capture region dimensions.
- Background image data.
- Annotation list.
- Selected annotation should not render selection handles in final export.
- Watermark settings.
- Numbering badge state.
- Render style contract.
- Output format: PNG.

### Export Response

WebView must respond with:

- Request id.
- Success/failure state.
- PNG payload on success.
- Error reason on failure.
- Render dimensions.

### Fallback

Rust must keep the current native export renderer available until Web export is stable.

Fallback triggers:

- WebView unavailable.
- Export request timeout.
- Invalid response.
- PNG decode/write failure.
- Web export feature flag disabled.

### Export Parity

Web export must match on-screen committed annotation rendering for:

- Rectangle.
- Oval.
- Line.
- Arrow.
- Pen.
- Pen arrow.
- Highlighter line.
- Highlighter area.
- Text line.
- Text box.
- Solid text.
- Tag.
- Mosaic.
- Watermark text/date/image.
- Numbering badges.

## Technical Requirements

### Render Style Contract

Create a Rust-owned `RenderStyle` structure that is serialized to WebView and used by Rust fallback export.

The contract should include:

- Annotation palette.
- Stroke min/max/default.
- Font min/max/default.
- Arrow head sizing.
- Pen smoothing/tension settings.
- Highlighter opacity.
- Highlighter line width formula.
- Highlighter area radius.
- Text padding.
- Text solid radius.
- Tag frame width.
- Tag radius.
- Tag pointer geometry constants.
- Mosaic cell size and sampling rules.
- Step badge size.
- Step badge font size.
- Step badge placement offsets.
- Selection handle sizes, for on-screen use only.

Rules:

- JS should not hardcode visual constants that belong to exported annotation appearance.
- JS may keep layout-only constants for toolbar/menu positioning.
- Rust fallback renderer and JS renderer should consume the same style values.

### Typed Web Protocol

Replace stringly typed export messages with a typed protocol.

Recommended request:

```json
{
  "type": "exportRequest",
  "requestId": 123,
  "format": "png",
  "region": { "x": 0, "y": 0, "width": 1200, "height": 800 },
  "background": {
    "encoding": "png",
    "data": "base64..."
  },
  "annotations": [],
  "renderStyle": {}
}
```

Recommended response:

```json
{
  "type": "exportReady",
  "requestId": 123,
  "format": "png",
  "width": 1200,
  "height": 800,
  "data": "base64..."
}
```

Failure response:

```json
{
  "type": "exportFailed",
  "requestId": 123,
  "reason": "timeout | render-error | invalid-state"
}
```

### Background Image Transfer

Preferred phased approach:

1. Start with PNG/base64 background transfer from Rust to WebView for export only.
2. Optimize later if needed using shared files, memory buffers, or WebView resource interception.

Constraints:

- Avoid sending background data every frame.
- Send only on export request.
- Cache background image in WebView for the current capture session if possible.
- Invalidate cache when a new capture session starts.

### Canvas Export Renderer

WebView should render export into an offscreen canvas, not the visible editor canvas.

The export canvas must:

- Match capture region pixel size.
- Not include toolbar, option menus, selection handles, hover states, caret, or region preview.
- Render annotations in document order.
- Respect annotation opacity.
- Render watermark behind annotations unless current behavior says otherwise.
- Produce PNG data.

### Performance Requirements

Targets:

- Normal 1080p capture export: under 100 ms after request.
- 4K capture export: under 250 ms target, under 500 ms worst-case before fallback.
- Web export timeout: configurable, initial recommendation 750 ms.
- No impact on live drawing responsiveness.
- No Web export work during live pointer drag.

Memory:

- Avoid holding multiple full-resolution export canvases.
- Release export canvas references after response.
- Avoid duplicating background image more than necessary.

## Migration Plan

### Phase 1: Shared Render Style Contract

Purpose:
Make Rust and JS consume one style contract without changing rendering behavior.

Tasks:

- Add `RenderStyle` struct in Rust.
- Serialize it through existing state sync.
- Replace JS export-relevant magic numbers with `renderStyle` values.
- Keep current JS defaults as fallback only.
- Make Rust renderer consume the same `RenderStyle`.

Acceptance:

- No visible UI change.
- Existing WebView editor looks unchanged.
- Rust export output unchanged or closer to WebView visuals.
- `cargo test`, `cargo check`, and `node --check` pass.

### Phase 2: Export Protocol Skeleton

Purpose:
Add request/response plumbing without changing default export behavior.

Tasks:

- Add `exportRequest`, `exportReady`, and `exportFailed` messages.
- Add Rust request id tracking.
- Add timeout handling.
- Keep native Rust export as default.
- Log Web export output for diagnostics only.

Acceptance:

- Copy/save behavior remains current.
- WebView can respond to export requests.
- Fallback remains primary.
- No UI behavior change.

### Phase 3: Web Export Behind Feature Flag

Purpose:
Generate actual WebView PNG exports but do not make them default.

Tasks:

- Rust sends background image plus annotation snapshot to WebView.
- WebView renders into offscreen canvas.
- WebView returns PNG.
- Rust writes PNG to temporary debug location or optional test path.
- Compare output manually and with scripted smoke checks.

Acceptance:

- Export output visually matches WebView committed annotations.
- No visible editor regressions.
- Rust fallback still used for normal Copy/Save unless flag enabled.

### Phase 4: Make Web Export Default With Fallback

Purpose:
Use Web export for Copy/Save by default while keeping fallback.

Tasks:

- Enable Web export by default.
- Fallback to Rust export on failure/timeout.
- Keep diagnostics for fallback rate.
- Validate on normal DPI and 4K/high DPI.

Acceptance:

- Copy/save output matches on-screen WebView visuals.
- Copy/save remains responsive.
- Fallback works if WebView is unavailable.

### Phase 5: Retire Most Rust Annotation Rendering

Purpose:
Reduce long-term duplication after Web export is proven stable.

Tasks:

- Keep Rust renderer only for fallback/debug.
- Remove unused native toolbar/submenu drawing if no longer needed.
- Keep minimal renderer paths needed for safe fallback.
- Split renderer module out of `overlay.rs`.

Acceptance:

- Smaller `overlay.rs`.
- No loss of fallback safety.
- Reduced duplicated visual constants.

## Testing Plan

### Unit Tests

- Annotation serialization.
- Render style serialization.
- Numbering badge policy.
- Tag geometry policy where possible.
- Export request/response parsing.

### Integration Tests

- WebView export request returns PNG.
- Timeout falls back to Rust renderer.
- Invalid WebView response falls back to Rust renderer.
- Copy still writes image to clipboard.
- Save still writes PNG.

### Visual Regression Tests

Capture test scenes for:

- Rectangle.
- Oval.
- Line.
- Arrow.
- Pen.
- Pen arrow.
- Highlighter line.
- Highlighter area.
- Text line.
- Text box.
- Solid text.
- Tag.
- Mosaic.
- Watermark text/date/image.
- Numbering badges.

Compare:

- On-screen WebView committed rendering.
- Web export PNG.
- Rust fallback PNG.

Initial visual comparison can be manual. Later it should become automated with pixel tolerance.

### Manual Smoke Tests

- Launch app.
- Trigger capture with hotkey.
- Select full screen.
- Select full window.
- Draw each annotation.
- Edit text/tag.
- Toggle numbering.
- Use Copy.
- Use Save.
- Test light/dark mode.
- Test 4K monitor.
- Test normal DPI monitor.
- Test multi-monitor.

## Risks

### Risk: Export Becomes Slower

Mitigation:

- Keep fallback.
- Add timeout.
- Cache background image for current capture session.
- Avoid export canvas during live drawing.

### Risk: WebView Export Fails On Some Machines

Mitigation:

- Keep Rust fallback.
- Log failure reasons.
- Avoid deleting Rust renderer until enough confidence exists.

### Risk: UI Regressions

Mitigation:

- Do not change toolbar/option code during export migration unless necessary.
- Gate export changes behind feature flag.
- Keep existing WebView editor as baseline.

### Risk: Large Capture Memory Usage

Mitigation:

- Release export canvas after use.
- Avoid repeated background transfers.
- Consider PNG transfer optimization later.

### Risk: Protocol Drift

Mitigation:

- Define typed messages.
- Keep schema in one place.
- Add tests for serialization/deserialization.

## Success Metrics

- Copy/save output visually matches on-screen annotations.
- No visible changes to current editor UX.
- Copy/save succeeds with fallback when Web export is unavailable.
- Export remains fast on 4K captures.
- Rust annotation renderer duplication decreases.
- `overlay.rs` becomes easier to split after renderer responsibilities are clarified.

## Open Questions

- Should Web export use PNG base64 or binary bridge payload first?
- Should background image be cached in WebView per capture session?
- What timeout should ship by default: 500 ms, 750 ms, or 1000 ms?
- How much visual difference from Rust fallback is acceptable before Web export becomes default?
- Should export include future selection/caret states for special workflows, or always final clean image only?

## Decision Recommendation

Proceed with the target architecture, but only in phases.

Do not rewrite the UI.
Do not remove Rust export yet.
Do not make Web export default until it is proven stable.

The safest immediate next step is Phase 1: centralize the render style contract. That improves quality without touching the current front-end experience.
