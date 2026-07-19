$ErrorActionPreference = "Stop"

Get-Process -Name "screencaptn" -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Host "Stopped any running Screen Captn dev process."

