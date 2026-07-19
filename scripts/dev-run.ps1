$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

$cargoBin = Join-Path $env:USERPROFILE ".cargo\bin"
if (Test-Path $cargoBin) {
    $env:Path = "$cargoBin;$env:Path"
}

Get-Process -Name "screencaptn" -ErrorAction SilentlyContinue | Stop-Process -Force

cargo build -p screencaptn-win

$exe = Join-Path $repoRoot "target\debug\screencaptn.exe"
Start-Process -FilePath $exe -WorkingDirectory $repoRoot
Write-Host "Screen Captn is running from $exe"
Write-Host "Use Ctrl+Shift+A to open capture. Run scripts\dev-stop.ps1 to stop it."

