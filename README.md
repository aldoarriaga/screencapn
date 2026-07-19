# Screen Cap'n

Screen Cap'n is a Rust-first native Windows screen capture app focused on fast capture, no-admin install, accessibility, and a compact annotation workflow.

## V1 Direction

- Windows-only native app.
- `Ctrl+Shift+A` global hotkey.
- Editable capture region after annotation begins.
- Object-based annotations until confirm.
- Default confirm action copies the final capture to the clipboard.
- Microsoft Store MSIX is the intended v1 release path.

## Current Implementation Slice

This repository contains the native foundation:

- `screencaptn-core`: geometry, annotation model, capture document, settings, and undo/redo history.
- `screencaptn-win`: Windows tray/hotkey shell and capture overlay.

The Windows app currently supports:

- Tray resident app.
- `Ctrl+Shift+A` capture overlay.
- Hover to hint a monitor, window, or client area; click to accept the hinted region or drag to create a manual region.
- Move/resize the capture region after selecting tools.
- Compact toolbar tool selection with a left-side drag grip.
- Basic shape/text/tag/mosaic/highlighter/pen/watermark annotation objects.
- Click annotations directly to select/move/delete them; selection is contextual, not a toolbar tool.
- Undo.
- Enter/Copy toolbar action to copy the selected region with annotations to the clipboard.
- Save toolbar action or `Ctrl+S` to write a PNG file; optional auto-save defaults to `%USERPROFILE%\Pictures\Screen Cap'n`.
- Escape/Cancel to close the overlay.

## Development

This repo is configured for the `stable-x86_64-pc-windows-gnu` Rust toolchain so development does not require admin-only Visual Studio Build Tools.

Install Rustup in user space, then run:

```powershell
cargo test
cargo run -p screencaptn-win
```

If the GNU toolchain is not installed yet:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
```

### No-Install Dev Loop

You do not need to reinstall the app while developing. Run the latest debug build directly:

```powershell
.\scripts\dev-run.ps1
```

That script stops any running Screen Cap'n process, rebuilds `screencaptn-win`, and launches `target\debug\screencaptn.exe`.

Stop the running dev app with:

```powershell
.\scripts\dev-stop.ps1
```

Run formatting, type checks, and tests with:

```powershell
.\scripts\dev-check.ps1
```

## Microsoft Store Path A

The v1 release path is Microsoft Store first:

- Store MSIX distribution first.
- Microsoft Store signing for the Store package.
- Website/GitHub direct downloads deferred until a trusted direct-download signing option is chosen.
- Minimum target: Windows 10 version 1809 / build 17763 or later.

Run the local Store-readiness check with:

```powershell
.\scripts\store-release-check.ps1
```

See [`docs/microsoft-store-path-a.md`](docs/microsoft-store-path-a.md) and [`PRIVACY.md`](PRIVACY.md) before Microsoft Store submission.
