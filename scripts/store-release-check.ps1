param(
    [switch]$SkipAudit
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}

Get-Process -Name "screencaptn" -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host "== Web bundle =="
npm.cmd run build:web
npm.cmd run check:web

Write-Host "== Rust checks =="
cargo fmt --all -- --check
cargo check
cargo test -p screencaptn-core
cargo build --release -p screencaptn-win

Write-Host "== Release manifest =="
$manifest = Join-Path $repoRoot "crates\screencaptn-win\app.manifest"
$releaseExe = Join-Path $repoRoot "target\release\screencaptn.exe"
$mt = Get-ChildItem "C:\Program Files (x86)\Windows Kits\10\bin", "$env:TEMP\screencaptn-winsdk-buildtools\bin" `
    -Recurse `
    -Filter "mt.exe" `
    -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match "\\x64\\mt\.exe$" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if ($mt) {
    & $mt.FullName -manifest $manifest "-outputresource:$releaseExe;#1"
} else {
    Write-Warning "mt.exe was not found. The release EXE may fail WACK DPI-awareness validation until app.manifest is embedded."
}

if (-not $SkipAudit) {
    Write-Host "== Dependency audits =="
    npm.cmd audit --omit=dev --audit-level=high

    if (Get-Command cargo-audit -ErrorAction SilentlyContinue) {
        cargo audit
    } else {
        Write-Warning "cargo-audit is not installed. Install it with: cargo install cargo-audit"
    }
} else {
    Write-Warning "Skipping dependency audits."
}

Write-Host ""
Write-Host "Local release checks completed."
Write-Host "Next manual Path A steps: build MSIX, run WACK, and submit through Partner Center."
