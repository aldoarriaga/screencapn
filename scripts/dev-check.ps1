$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}

Get-Process -Name "screencaptn" -ErrorAction SilentlyContinue | Stop-Process -Force

npm.cmd run build:web
npm.cmd run check:web

cargo fmt --all -- --check
cargo check
cargo test -p screencaptn-core
