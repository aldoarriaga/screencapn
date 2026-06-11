# Screen Cap'n v1.0.0.0 Handover

Date prepared: 2026-06-11

Repository: `C:\my-apps\screencapn`

Release candidate package: `C:\my-apps\screencapn\target\store-msix\ScreenCapn-1.0.0.0-x64.msix`

This document is intended for a senior engineer taking over Screen Cap'n from a cold start. It records the product behavior, architecture, important implementation decisions, current release state, and the details that are easiest to lose if only reading the code piecemeal.

## 1. Product Summary

Screen Cap'n is a Windows screenshot capture and annotation utility. It is tray-resident, invoked by a global hotkey, opens a native full-screen overlay, lets the user select or reuse a capture region, annotate it, then copy or save the final image.

The v1 product principles are:

- Fast by design: the app should open, capture, annotate, copy, and exit with minimal friction.
- Local first: screenshots are processed locally. No cloud telemetry, OCR, sync, or backend service is part of v1.
- Object-based annotation: drawings remain editable until the user confirms.
- Lightweight native shell: Rust owns capture, windowing, export, settings, tray, hotkey, and packaging.
- Rich but compact editing UI: WebView2/Konva/Lexical owns the annotation toolbar, visual editing affordances, and text editing.
- Microsoft Store first: v1 distribution is intended as a Microsoft Store MSIX package signed by Microsoft Store.

The user-facing name is `Screen Cap'n`.

## 2. Current Release State

The current public release target is `1.0.0.0`.

Important release files:

- MSIX candidate: `target\store-msix\ScreenCapn-1.0.0.0-x64.msix`
- WACK result XML: `target\store-msix\1.0.0.0 result.xml`
- WACK report HTML: `target\store-msix\wack-report.htm`
- MSIX staging folder: `target\store-msix\stage`
- Store manifest: `target\store-msix\stage\AppxManifest.xml`

The staged MSIX currently contains only the runtime payload:

- `screencaptn.exe`
- `WebView2Loader.dll`
- `AppxManifest.xml`
- generated AppX metadata
- logo assets under `Assets\`

The MSIX intentionally does not include `docs`, `README.md`, `LICENSE`, or `THIRD_PARTY_NOTICES.md`. Those files exist for repository and Store/listing compliance, not runtime behavior.

The MSIX is a Store path candidate and is not locally trusted-signed. Partner Center / Microsoft Store signing is expected to provide the trusted distribution signature.

Important metadata:

- Store identity in staged manifest: `AldoArriaga.ScreenCapn`
- Publisher in staged manifest: `CN=Screen Capn`
- Package version: `1.0.0.0`
- Target device family: `Windows.Desktop`
- Minimum Windows version: `10.0.17763.0` / Windows 10 1809
- Architecture: `x64`
- Capability: `runFullTrust`

Known release metadata debt:

- Workspace crate version in `Cargo.toml` is still `0.1.0`.
- `crates\screencaptn-win\app.manifest` assembly identity is still `0.1.0.0`.
- This does not stop the current MSIX package from using `1.0.0.0`, but it should be aligned before a final tagged release.

## 3. Repository Layout

Top-level:

- `Cargo.toml`: Rust workspace.
- `Cargo.lock`: Rust dependency lockfile.
- `package.json`: Web UI bundling pipeline.
- `package-lock.json`: JavaScript dependency lockfile.
- `README.md`: public project summary and development commands.
- `PRIVACY.md`: privacy policy source text for Store listing.
- `LICENSE`: project MIT license.
- `THIRD_PARTY_NOTICES.md`: open-source notices.
- `docs\microsoft-store-path-a.md`: Microsoft Store submission checklist.
- `scripts\store-release-check.ps1`: local release validation script.
- `target\store-msix\`: local MSIX/WACK artifacts.

Rust crates:

- `crates\screencaptn-core`: shared model crate.
- `crates\screencaptn-win`: Windows native app crate.

Web UI assets:

- `crates\screencaptn-win\assets\web-ui\index.html`: WebView host page.
- `crates\screencaptn-win\assets\web-ui\app.js`: main Web/Konva/Lexical UI implementation.
- `crates\screencaptn-win\assets\web-ui\src\app-entry.js`: bundling entrypoint that exposes Lexical APIs and imports `app.js`.
- `crates\screencaptn-win\assets\web-ui\app.bundle.js`: generated/minified bundle embedded at compile time.
- `crates\screencaptn-win\assets\web-ui\vendor\konva.js`: bundled Konva runtime.
- `crates\screencaptn-win\assets\toolbar\`: toolbar SVGs.
- `crates\screencaptn-win\assets\region-controls\`: lock/unlock and aspect-ratio SVGs.

## 4. Build Pipeline

JavaScript build:

```powershell
npm.cmd run build:web
npm.cmd run check:web
```

`build:web` uses esbuild to bundle `assets\web-ui\src\app-entry.js` into `assets\web-ui\app.bundle.js` as an IIFE targeting Chrome 120. This bundle includes Lexical and the app UI.

Rust build/check:

```powershell
cargo fmt --all -- --check
cargo check
cargo test -p screencaptn-core
cargo build --release -p screencaptn-win
```

Release validation script:

```powershell
.\scripts\store-release-check.ps1
```

The release script:

- stops any running `screencaptn` process
- builds/checks the Web bundle
- runs Rust format/check/core tests/release build
- embeds `crates\screencaptn-win\app.manifest` into the release EXE when `mt.exe` is available
- runs `npm audit --omit=dev --audit-level=high`
- runs `cargo audit` when installed

The release script does not create the MSIX because package identity, Store reservation, signing, and WACK execution are account/machine specific.

## 5. Core Rust Model

The shared model is in `crates\screencaptn-core`.

### Geometry

`geometry.rs` defines:

- `Point`
- `Size`
- `Rect`
- `ResizeHandle`

`Rect` supports:

- `right()`, `bottom()`, `center()`
- `contains(point)`
- `translate(dx, dy)`
- `normalize()`
- `from_points(a, b)`
- `resize_from_handle(handle, to, min_size)`
- `is_visible()`
- `hit_resize_handle(point, radius)`

This geometry is used by both capture regions and annotations.

### Style

`style.rs` defines:

- `Color`
- `StrokeStyle`
- `HighlightShape`
- `ToolKind`

Default stroke:

- width: `8.0`
- color: red `#FF3B30`
- opacity: `1.0`

Supported tools:

- Step number
- Rectangle
- Oval
- Line
- Arrow
- Pen
- Text
- Tag
- Mosaic
- Highlighter
- Watermark

### Annotations

`annotation.rs` defines `AnnotationKind`:

- `Rectangle`
- `Oval`
- `Line { start, end }`
- `Arrow { start, end }`
- `StepNumber { number }`
- `Text { text, font_size, framed, filled }`
- `Tag { label, anchor, font_size }`
- `Mosaic { mode, brush_size }`
- `Highlighter { shape, opacity, start, end }`
- `Pen { points }`
- `PenArrow { points }`
- `Watermark { text, opacity }`

`MosaicMode`:

- `Area`
- `Brush`

`Annotation` fields:

- `id`
- `bounds`
- `stroke`
- `fill`
- `step_number`
- `kind`

Important annotation behavior:

- `display_step_number()` returns attached auto numbering or the standalone step number.
- `accepts_auto_numbering()` excludes standalone step numbers and watermarks.
- `translated(dx, dy)` correctly moves points/anchors for lines, arrows, highlighters, tags, and pens.

### Document and History

`document.rs` owns:

- selected capture region
- annotation list
- selected annotation id
- next annotation id

`history.rs` provides bounded undo/redo checkpoints. The overlay creates `History::new(100)`.

## 6. Native Windows Architecture

The Windows app is in `crates\screencaptn-win`.

### Entry Point

`main.rs`:

- uses `windows_subsystem = "windows"` outside debug builds
- installs diagnostics
- creates and runs `NativeApp`

### Native App Shell

`native.rs` owns:

- hidden native window
- DPI awareness
- global hotkey registration
- tray menu command handling
- theme/settings reload
- capture overlay lifecycle
- shortcut editor launch
- auto-save folder picker
- donation URL launch

Startup behavior:

- Calls `SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)`.
- Creates hidden top-level window class `ScreenCaptnHiddenWindow`.
- Loads theme/settings.
- Registers configured hotkey.
- Adds tray icon.
- Logs hotkey/tray registration failures instead of killing the app.

Hotkey behavior:

- Default is `Ctrl+Shift+A`.
- Configured hotkey is loaded from settings.
- If registration fails, the app falls back to default `Ctrl+Shift+A`, saves that setting, and continues.
- The app prevents multiple overlays from opening at once with `overlay_open`.

Tray behavior:

- Left or right tray click opens menu.
- Menu commands:
  - quick access shortcut editor
  - auto-save toggle
  - choose auto-save folder
  - light/dark theme toggle
  - donate link
  - exit

Donation command:

- Menu text: `Enjoying Screen Cap'n? Consider donating`
- Opens: `https://screencapn.com/donate`

### Tray

`tray.rs` owns Shell notification icon and native context menu.

Command ids:

- `9000`: set shortcut
- `9001`: toggle auto-save
- `9002`: set auto-save folder
- `9003`: toggle theme
- `9004`: donate
- `9005`: exit

Tray tooltip:

- `Screen Cap'n - {hotkey display label}`

Current caveat:

- The checkbox glyph strings in `tray.rs` show mojibake in the source (`â˜‘` / `â˜`). Functionally they are intended as checked/unchecked checkbox labels. This should be cleaned up to proper UTF-8 or replaced with ASCII-safe labels before final polish.

### Settings

`settings.rs` persists to:

```text
%APPDATA%\ScreenCaptn\settings.json
```

Settings:

- `hotkey`
- `autoSave`
- `aspectRatio`
- `lockedRegions`

Default hotkey:

- Ctrl: true
- Shift: true
- Alt: false
- Win: false
- key: `A`

Default auto-save:

- disabled
- folder: `%USERPROFILE%\Pictures\Screen Cap'n`

Aspect ratios:

- `custom`
- `9x16`
- `16x9`
- `1x1`
- `4x5`

Important v1 behavior:

- `OverlayState::new` resets `settings.aspect_ratio` to `Custom` on each capture launch. This prevents a ratio preset from leaking into the next screenshot call.
- Locked regions persist per monitor, relative to the monitor rectangle.

### Theme

`theme.rs` persists theme to:

```text
%APPDATA%\ScreenCaptn\theme.txt
```

Theme values:

- light
- dark

### Diagnostics

`diagnostics.rs` is opt-in.

Diagnostics are enabled only in debug builds or when:

```text
SCREENCAPTN_DIAGNOSTICS=1
```

Log path:

```text
%LOCALAPPDATA%\ScreenCaptn\logs\screencaptn.log
```

Logs include breadcrumbs, startup errors, WebView2 process failure events, and JS diagnostics when enabled. Production should not log sensitive window titles or screenshot contents.

### App Icon

`app_icon.rs` loads the application icon used by the tray/window. The current icon work used the supplied Screen Cap'n SVG/logo assets and generated store assets in the MSIX staging folder.

## 7. Capture Overlay Architecture

The main overlay is `crates\screencaptn-win\src\overlay.rs`.

Rust owns:

- screenshot capture
- native overlay window
- smart region detection
- region selection/move/resize
- persistent locked region behavior
- aspect-ratio constrained region behavior
- native red border and dimming
- native export/copy/save
- native watermark export
- settings updates
- authoritative annotation model

WebView2/Konva owns:

- toolbar UI
- submenu UI
- SVG icons
- smooth toolbar animation
- region control visual overlay
- committed annotation visual layer
- selection handles
- drawing previews
- Lexical text editor overlay
- exact-hit color swatches
- watermark preview

The overlay window:

- class: `ScreenCaptnCaptureOverlay`
- topmost tool window
- covers the full virtual desktop
- captures a frozen screenshot bitmap on launch
- initializes WebView2 as transparent child UI

Important constants:

- `HANDLE_RADIUS = 6.0`
- `REGION_RESIZE_HIT_RADIUS = 26.0`
- `MIN_REGION_SIZE = 24.0`
- `FRAME_HIT_WIDTH = 22.0`
- `CLICK_DRAG_THRESHOLD = 5.0`
- `PEN_POINT_SPACING = 5.5`
- `DEFAULT_TEXT_FONT_SIZE = 27.0`
- `TAG_DEFAULT_WIDTH = 146.0`
- `TAG_DEFAULT_HEIGHT = 55.0`
- `WATERMARK_OPACITY = 0.5`
- `WATERMARK_ROTATION_DEGREES = -20.0`
- `WEB_EXPORT_ENABLED = false`
- `MAGNIFIER_SIZE = 104.0`
- `MAGNIFIER_SAMPLE_SIZE = 18.0`

Because `WEB_EXPORT_ENABLED` is false, native Rust export is authoritative. The Web renderer is still critical for interactive preview/editing.

## 8. Capture Region Selection

### Current Smart Region V1 Behavior

Earlier UIA/app-specific content detection was intentionally removed because it was slow and too unpredictable across Chrome, Illustrator, Electron, and complex apps.

Current deterministic rules:

1. Cursor at absolute virtual desktop top edge (`<= 4px`) selects all screens.
2. Cursor near top of current monitor (`<= 32px`) selects current screen.
3. Cursor over a detected top-level app window selects that full window.
4. If no useful window exists, fallback is current screen.
5. Manual drag always overrides smart detection.
6. Locked region on launch overrides smart detection for that monitor.

Supported smart region kinds:

- `AllScreens`
- `CurrentScreen`
- `Window`
- `Manual`

Supported smart region sources:

- `Monitor`
- `Win32`
- `Manual`

Important implementation details:

- Initial candidate uses the actual cursor position from `GetCursorPos`, not virtual desktop origin.
- Window detection uses `EnumWindows`, root/top-level checks, `DwmGetWindowAttribute(DWMWA_EXTENDED_FRAME_BOUNDS)`, and fallback `GetWindowRect`.
- Tool windows, invisible windows, iconic windows, shell window, owned windows, and tiny windows are rejected.
- Basic anti-flicker state still exists: `pending_smart_region`, `HOVER_STABILITY_MS`, `MIN_RECT_CHANGE_PX`, `CONFIDENCE_SWITCH_DELTA`.
- Immediate switches happen for all-screens/current-screen/manual and from current-screen to window.

### Manual Region Selection

Before a capture region is committed:

- click accepts the current smart region
- click-drag creates a manual region
- Enter accepts the current smart region
- Escape cancels the capture overlay

After a capture region is committed:

- the region can be moved by dragging its frame
- the region can be resized using handles
- resize handle hit area is intentionally larger than the visible squares
- full-screen/current-screen regions still expose resize handles

### Dimming and Red Border

The current native visual behavior is intentional:

- animated red border stays native
- inactive screen area is dimmed
- when a region is active, non-active areas are darker
- region controls are rendered above a dark top gradient
- the gradient is behind the red frame

The user explicitly wanted to preserve the current animated red border and native dimming.

### Top Region Controls

The capture region has two top controls:

- lock/unlock
- aspect ratio

They are centered along the top of the region, styled consistently in selection preview and committed region mode, and use SVG assets in `assets\region-controls`.

Visual rules:

- controls are 90% white for contrast
- hover glow is laser red, consistent with toolbar hover language
- a dark top gradient improves visibility
- controls and gradient disappear once annotation starts / user annotations exist

Lock behavior:

- toggles persistence of the active region for the current monitor
- locked regions are stored relative to monitor bounds
- when capture opens on that monitor later, the locked region is applied immediately

Aspect ratio behavior:

- clicking ratio cycles: custom -> 9:16 -> 16:9 -> 1:1 -> 4:5 -> custom
- choosing a non-custom ratio disables smart auto-detection until the user returns to custom
- while in ratio placement mode, the selected ratio follows the mouse after the pointer leaves the top controls
- mouse wheel scales the ratio region up/down
- when a ratio is selected after a region is already active, the same placement/scroll behavior applies
- custom returns to full current screen
- corner dragging preserves the selected ratio
- non-corner resize while ratio is active switches back to custom
- ratio mode resets to custom on the next screenshot call

### Pixel Magnifier

While the user is finding a point to draw/select, the overlay has native magnifier support:

- square magnifier size: `104px`
- sample size: `18px`
- intended to help precise start/end placement

## 9. Annotation Tools

The annotation model is Rust-authoritative. Web/Konva renders live visuals and editing affordances from Rust state.

### Shared Tool Behavior

- Tool switching deselects any selected/edited annotation without deleting it.
- First Escape deselects or ends active editing.
- Second Escape cancels the capture overlay.
- Delete removes the selected annotation.
- Ctrl+Z undoes.
- Ctrl+S saves and closes the overlay.
- Enter copies to clipboard and closes, unless editing text/watermark where it commits according to text rules.
- Right-click deselects all and clears text editing feedback.
- Objects should be selected/draggable primarily through their line/stroke or explicit editable handles, not through interior empty area, except for text/tag editing surfaces.
- Toolbar selection is contextual: clicking an object syncs the active tool/submenu to that object.

### Rectangle

Behavior:

- Drawn by dragging inside the region.
- Stored as `AnnotationKind::Rectangle`.
- Uses current stroke color and stroke width.
- Selection hit test is stroke-based, not full interior.
- Editable via four box resize corners/handles.
- Movable by dragging the selected object hit line.

### Oval

Behavior:

- Drawn by dragging inside the region.
- Stored as `AnnotationKind::Oval`.
- Uses current stroke color and width.
- Selection hit test is elliptical stroke-based.
- Editable via box resize handles.

### Line

Behavior:

- Drawn from start to end.
- Stored as `AnnotationKind::Line { start, end }`.
- Uses round cap in Web preview.
- Selection hit test uses distance-to-segment with a tolerant radius.
- Endpoints are editable.

### Arrow

Behavior:

- Drawn from start to end.
- Stored as `AnnotationKind::Arrow { start, end }`.
- Uses stroke color for both line and head.
- Arrow head is scaled from stroke width with minimum head dimensions.
- Endpoints are editable.

### Pen

Behavior:

- Freehand drawing collects points every `5.5px`.
- Stored as `AnnotationKind::Pen { points }`.
- Web preview uses Konva line tension around `0.35`.
- Uses current stroke color and width.
- Selection hit test follows the polyline path.

Pen submenu:

- free pen
- pen arrow

### Pen Arrow

Behavior:

- Same freehand input as pen.
- Stored as `AnnotationKind::PenArrow { points }`.
- Renders a freehand path with an arrow tip at the end.

### Highlighter

Behavior:

- Default highlighter mode is line, not area.
- Line highlighter draws a semi-transparent rounded line from start to end.
- Area highlighter draws a semi-transparent rounded rectangle.
- Opacity defaults to `0.30`.
- Shift constrains highlighter line drawing horizontally/vertically.
- Highlighter temporarily uses yellow as active tool color and restores previous normal stroke color when switching away.

Highlighter shape mapping:

- Web name `line` maps to `HighlightShape::Rectangle` for line mode.
- Web name `area` maps to `HighlightShape::RoundedRectangle`.

### Text

Behavior:

- Stored as `AnnotationKind::Text { text, font_size, framed, filled }`.
- Inline click text creates non-framed text.
- Click-drag text creates framed textbox.
- Empty text annotations are removed on commit/cancel.
- Text uses Lexical in the Web overlay for editing.
- Caret supports visible/blinking state, click placement, text insertion, deletion, and left/right arrows.
- Shift+Enter inserts line breaks for editable text/tag.
- Framed text grows vertically when wrapped text exceeds the textbox height.
- Right-click outside commits/deselects rather than deleting text.

Text style:

- no-background and solid-background modes are controlled by the text submenu
- filled inline text renders one rounded background per row, with width based on each row's text
- filled textbox behaves like a whole textbox background
- no-background text uses the selected color directly
- filled text uses contrast text color over the selected background

Known limitation:

- Up/down arrow behavior in native fallback is minimal. Lexical handles DOM editing behavior where active.

### Tag

Behavior:

- Stored as `AnnotationKind::Tag { label, anchor, font_size }`.
- Tag is a speech-bubble/callout shape with a pointer anchor.
- Tag body defaults around `146x55`.
- Tag frame/taper uses `TAG_FRAME = 14`.
- Empty tags are preserved so their box corners and pointer remain editable before typing.
- Clicking inside the white tag area resumes text editing.
- Text is always black in both edit and committed states.
- Tag body resize changes only the box.
- Pointer/anchor drag changes only pointer location and keeps the box fixed.
- Final export is intended to match drawing mode: same tag body, taper, pointer, and frame geometry.

Tag selection/editing:

- body/corners expose box resize
- anchor handle exposes pointer relocation
- tag pointer line/body are hit-testable

### Step Number

Behavior:

- Standalone step number tool creates `AnnotationKind::StepNumber { number }`.
- Numbering toggle can attach auto-number badges to normal annotations.
- Watermark and standalone step number annotations do not receive auto-numbering.
- Modes:
  - restart numbering
  - continue numbering from next available number
- Step numbers can be edited by selecting/editing the badge.

### Mosaic

Behavior:

- Stored as `AnnotationKind::Mosaic { mode, brush_size }`.
- Modes:
  - area
  - brush
- Mosaic/pixelate samples the underlying screenshot image only.
- It should not pixelate or blur annotations drawn by Screen Cap'n.
- Web preview uses cached mosaic canvases.
- Native export should match this principle: mosaic affects sampled capture content, not overlay elements.

### Watermark

Watermark is a global overlay-style annotation/tool rather than a normal local drawing object.

State:

- `watermark_text`
- `watermark_color`
- `watermark_mode`
- `watermark_date_enabled`
- `watermark_image_path`
- `watermark_image_bitmap`
- `watermark_image_data_url`

Modes:

- text
- date toggle
- image picker

Important current behavior:

- The old separate "text" option in the watermark submenu was removed because it was redundant.
- Date is toggled by the calendar option.
- Image is selected through a native file picker.
- Enter while watermark text is active confirms/copies to clipboard and closes.
- Clear watermark resets text/date/image state.

Watermark visual requirements from the final tuning:

- Preview and final export should match.
- Text wrapping must match between preview and final export.
- Image transparency must match between preview and final export.
- Image stays upright; only text/date are rotated.
- Date is centered below text before rotation.
- Text rotation uses `-20deg`.
- Watermark opacity is `0.5`.
- When text and image are both present, pattern alternates between text/date tile and image tile where applicable.
- Honeycomb spacing should grow with text/font size to avoid overlap.

Current implementation notes:

- Native export is authoritative.
- Web preview receives `watermarkImageDataUrl` generated from native-decoded/faded bitmap data so browser transparency is closer to native export.
- `web_message_needs_static_redraw` marks watermark changes as requiring static redraw.
- `WEB_EXPORT_ENABLED` remains false, so do not rely on Web export for final file output.

## 10. Toolbar and Options UI

The toolbar is rendered by Web/Konva and positioned by Rust.

Toolbar tools/actions:

- drag grip
- numbering toggle
- rectangle
- oval
- line
- arrow
- pen
- highlighter
- text
- tag
- watermark
- mosaic
- undo
- copy
- save
- cancel

Toolbar behavior:

- appears centered near the bottom of the selected region/window
- is moved high enough so submenus/options remain visible and leave space below
- has a 1100ms total appear animation:
  - S-curve opacity
  - upward movement
  - subtle settling/bounce
- can be dragged by its grip
- hover effect scales icons and adds glow

Submenus:

- color swatches
- stroke width slider
- font size slider
- pen mode
- highlighter line/area mode
- text background mode
- watermark date/image/clear
- numbering restart/continue

Important color fix:

- Color swatches use exact-hit hotzones.
- Clicking between swatches or anywhere in the menu that is not a swatch does not change color.
- This fix is in the Web UI, not Rust.

## 11. WebView2 / Konva / Lexical UI

`web_ui.rs` embeds the web UI into the native overlay.

It:

- initializes WebView2 with apartment-threaded COM
- creates a transparent child controller
- disables default context menus, devtools, and status bar
- registers postMessage callback
- logs WebView2 process failures through diagnostics
- embeds `index.html`, `konva.js`, `app.bundle.js`, and toolbar/region SVGs with `include_str!`

`app-entry.js`:

- imports Lexical APIs
- exposes them on `window.SCREEN_CAPTN_LEXICAL`
- imports `app.js`

`app.js`:

- creates a Konva stage
- layers:
  - committed annotations
  - selection handles
  - active drawing preview
  - watermark preview
  - UI toolbar/submenus
- schedules dirty renders with `requestAnimationFrame`
- maintains annotation node caches and mosaic canvas caches
- receives full state and render diffs from Rust
- sends pointer, keyboard, toolbar, submenu, text, watermark, copy/save/cancel messages to Rust
- creates Lexical editor overlay for text/tag editing
- handles toolbar animation and exact-hit color swatches

Web-to-Rust message examples:

- `ready`
- `pointerDown`
- `pointerMove`
- `pointerUp`
- `selectTool`
- `toggleNumbering`
- `setColor`
- `setStrokeWidth`
- `setFontSize`
- `setPenMode`
- `setHighlighterShape`
- `setTextFilled`
- `setTextDraft`
- `commitText`
- `cancelText`
- `setTextCaret`
- `setWatermarkMode`
- `setWatermarkText`
- `clearWatermark`
- `copy`
- `save`
- `cancel`
- `deselect`
- `diagnostic`

Rust-to-Web messages:

- `state`: full snapshot
- `renderDiff`: incremental state/annotation update
- `exportRequest`: legacy/disabled path when `WEB_EXPORT_ENABLED` is true

## 12. Rendering and Export

Native rendering/export is the source of truth.

Copy:

- toolbar copy or Enter copies final capture to clipboard
- closes overlay after successful action

Save:

- toolbar save or Ctrl+S opens save flow
- closes annotation mode after successful save
- auto-save can bypass manual destination depending on tray setting

Auto-save:

- setting lives in `%APPDATA%\ScreenCaptn\settings.json`
- default folder: `%USERPROFILE%\Pictures\Screen Cap'n`
- filenames use `ScreenCapn-{timestamp}.png` with overflow handling

Clipboard format:

- native clipboard path uses bitmap data for clipboard interoperability

File format:

- current save/auto-save target is PNG.

Render parity requirements:

- line widths in drawing mode and final screenshot must match
- tag geometry in drawing mode and final screenshot must match
- watermark preview and final screenshot must match
- mosaic must affect screenshot pixels, not tool annotations

Static rendering cache:

- `RenderCache` keeps back/static device contexts and bitmaps.
- `static_layer_dirty` marks when static repaint is needed.
- Web UI receives diffs to avoid unnecessary full sync.

## 13. Keyboard and Mouse Behavior

Global:

- hotkey opens capture overlay
- Escape:
  - first press deselects/ends active annotation/editing
  - second press cancels overlay
- Enter:
  - before region: accepts current smart region
  - after region: copies final image and closes
  - while watermark text active: confirms/copies/closes
  - while normal text active: commits unless Shift+Enter inserts line break
- Ctrl+S: save and close
- Ctrl+Z: undo
- Delete: delete selected annotation
- Right-click: deselect all

Text/tag:

- clicking text/tag sets caret position
- right-click outside deselects and leaves object
- Shift+Enter inserts line break
- text/tag caret should be visible and blinking when editing

Region:

- drag inside no-region state creates region
- click no-region state accepts smart region
- frame drag moves active region
- handles resize active region
- wheel scales ratio-locked region during ratio placement

## 14. Shortcut Editor

`shortcut_window.rs` implements the compact native hotkey editor.

Product intent:

- compact version of a gaming/macro key editor
- specific to key presses, not a full macro recorder
- allows user to change default `Ctrl+Shift+A`

Validation:

- hotkey must have a nonzero key code
- at least one modifier must be present

The saved hotkey is immediately re-registered. If registration fails, app falls back to default.

## 15. Privacy and Security Posture

Privacy position:

- screenshots are processed locally
- screenshot content is not uploaded by the app
- clipboard copy is user-triggered
- auto-save is local and user-configurable
- no backend analytics in v1
- diagnostics are opt-in/debug only
- no telemetry or network service was added just for release

Network behavior:

- only explicit donation menu opens `https://screencapn.com/donate` in browser
- WebView2 UI is loaded from embedded strings, not a remote page

Security/release checks already used in this cycle:

- static review of project code
- gitleaks/secret scan
- Windows App Certification Kit
- MSIX package scan/review
- Microsoft Defender scan
- dependency audit path via `store-release-check.ps1`

Third-party notices:

- `THIRD_PARTY_NOTICES.md` lists bundled/runtime/build dependencies.
- Notable runtime/bundled components:
  - Konva 10.3.0, MIT
  - Lexical 0.28.0, MIT
  - WebView2Loader.dll, Microsoft terms
  - Rust standard library, MIT OR Apache-2.0

## 16. Microsoft Store Submission Notes

Path A is Microsoft Store MSIX first.

Use:

- `docs\microsoft-store-path-a.md`
- `PRIVACY.md`
- `THIRD_PARTY_NOTICES.md`
- WACK report from `target\store-msix`

Recommended Store declarations:

- Do not mark in-app purchases unless real purchases exist.
- Do not mark accessibility-tested unless a real accessibility pass was completed.
- Do not mark game recording/broadcast; this is not a game.
- Do not mark generative AI; there are no generative AI features in the app.
- Pen and ink input can be left unchecked unless proper pen-specific QA is completed.
- Hardware requirements can generally be left unspecified; although keyboard/mouse are required in practice, declaring them as minimum may unnecessarily block/flag devices.

Store certification notes to include if needed:

- Screen Cap'n captures screenshots only after user action.
- Screen Cap'n uses a configurable global hotkey.
- Clipboard copy is user-triggered.
- Screenshot content is processed locally and is not uploaded by the app.
- The app is a full-trust desktop MSIX package.

## 17. WACK Context

WACK means Windows App Certification Kit.

It validates Store package requirements such as:

- manifest correctness
- API usage checks
- file/package structure
- app launch behavior
- signing/package metadata
- supported platform declarations

Observed WACK context from this cycle:

- Package was generated and tested locally.
- Optional warnings may appear because Rust standard library strings/API references can look like blocked executable references in static scans.
- Review `target\store-msix\1.0.0.0 result.xml` and `target\store-msix\wack-report.htm` before final submission.

## 18. Known Technical Caveats

These are not necessarily blockers, but a new engineer should know them.

1. Rust crate version and embedded Win32 manifest are still `0.1.0`/`0.1.0.0`, while MSIX is `1.0.0.0`.
2. The tray checkbox labels show source encoding corruption and should be made UTF-8 clean or ASCII-safe.
3. UIA/content-region detection was deliberately removed. Do not re-add it casually; it caused lag and bad sidebar/app-panel selections.
4. `WEB_EXPORT_ENABLED` is false. Native export is the path that matters.
5. Web UI is embedded with `include_str!`, so after JS changes always run `npm run build:web` before Rust build/package.
6. WebView2 runtime is assumed available through Microsoft Edge WebView2.
7. Test MSIX is not trusted-signed until Store signing.
8. Some README text may still mention older details such as BMP/default folder spelling; align README before final public release.
9. The current git working tree may contain release-candidate changes not yet committed. Always inspect `git status` before editing or tagging.

## 19. Manual QA Checklist Before Final Store Submission

Core capture:

- Launch app from packaged MSIX.
- Confirm tray icon appears.
- Confirm default hotkey opens overlay.
- Confirm custom hotkey can be saved and used.
- Confirm Escape closes overlay when no active object exists.
- Confirm first Escape deselects active object, second Escape cancels.
- Confirm current screen, all-screens top edge, and full-window detection.
- Confirm manual drag overrides detection.
- Confirm locked region persists per monitor.
- Confirm ratio cycling, ratio placement, scroll scaling, and custom full-screen reset.

Toolbar:

- Toolbar appears smoothly and submenus are fully visible.
- Toolbar can be dragged.
- Tool switching deselects selected object without deleting it.
- Color swatches change only on exact swatch hits.
- Clicking between swatches does nothing.

Annotations:

- Rectangle draw/select/move/resize/export line width.
- Oval draw/select/move/resize/export line width.
- Line endpoint edit and export parity.
- Arrow endpoint edit/head scale/export parity.
- Pen freehand draw/move/export parity.
- Pen arrow draw/move/export parity.
- Highlighter line default, area mode, opacity/export parity.
- Mosaic area/brush affects screenshot pixels only.
- Text inline creation, click-drag textbox creation, line breaks, caret, right-click deselect.
- Filled inline text has per-line background.
- Filled textbox background covers whole textbox.
- Framed textbox grows to fit wrapped text.
- Tag empty resize/pointer edit.
- Tag with text resize/pointer edit.
- Tag white area re-enters text editing.
- Tag final export matches drawing mode.
- Step number standalone edit.
- Auto-number restart/continue and attached badges.

Watermark:

- Text-only preview/export match.
- Image-only preview/export transparency match.
- Text+image preview/export match.
- Date centered below text and rotated as a group.
- Image remains upright.
- Long text wraps identically in preview and final output.
- Font size changes do not freeze.
- Enter while watermark text active copies/closes.

Output:

- Copy to clipboard works.
- Save As writes PNG and closes annotation mode.
- Auto-save toggle works.
- Auto-save folder picker works.
- Auto-save writes to `%USERPROFILE%\Pictures\Screen Cap'n` by default.

Environments:

- single monitor
- multi-monitor
- mixed DPI if available
- maximized and non-maximized apps
- Windows Explorer
- Chrome / browser
- Electron apps
- creative apps such as Illustrator

Release:

- `npm run build:web`
- `npm run check:web`
- `cargo fmt --all -- --check`
- `cargo check`
- `cargo test -p screencaptn-core`
- `cargo build --release -p screencaptn-win`
- package MSIX
- run WACK
- scan package
- upload through Partner Center

## 20. Suggested First Tasks for the Next Engineer

1. Run `git status` and identify all uncommitted release-candidate files.
2. Align crate/app manifest versions with `1.0.0.0`.
3. Fix tray checkbox label encoding.
4. Update README if it still says BMP or old save folder paths.
5. Re-run full release validation and WACK from a clean tree.
6. Create a signed Store submission package through Partner Center.
7. Tag the final release commit once the submitted package hash/version is known.

