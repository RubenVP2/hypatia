# ==============================================================================
# Unified build script for Hypatia (Rust + Slint Desktop + Embedded Mobile UI)
# ==============================================================================
$ErrorActionPreference = "Stop"

$sw = [System.Diagnostics.Stopwatch]::StartNew()

Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "  HYPATIA - Native Desktop Release Build" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan

Write-Host "`n[1/3] Running workspace unit tests..." -ForegroundColor Yellow
cargo test --workspace
if ($LASTEXITCODE -ne 0) {
    Write-Host "`n[ERROR] cargo test --workspace failed with exit code $LASTEXITCODE." -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "  -> Workspace tests passed." -ForegroundColor Green

Write-Host "`n[2/3] Compiling Windows native binary (hypatia)..." -ForegroundColor Yellow
cargo build --release -p hypatia
if ($LASTEXITCODE -ne 0) {
    Write-Host "`n[ERROR] cargo build --release -p hypatia failed with exit code $LASTEXITCODE." -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "  -> Binary compiled." -ForegroundColor Green

Write-Host "`n[3/3] Verifying output binary..." -ForegroundColor Yellow
$binaryPath = "target/release/hypatia.exe"
if (-not (Test-Path $binaryPath)) {
    Write-Host "`n[ERROR] Binary $binaryPath was not found!" -ForegroundColor Red
    exit 1
}

$binaryItem = Get-Item $binaryPath
$binaryBytes = $binaryItem.Length
$binaryMb = [math]::Round($binaryBytes / 1MB, 2)
$sw.Stop()
$totalSec = [math]::Round($sw.Elapsed.TotalSeconds, 1)

Write-Host "`n====================================================================" -ForegroundColor Green
Write-Host "  BUILD SUCCEEDED in $totalSec s" -ForegroundColor Green
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "  Output binary : $binaryPath" -ForegroundColor White
Write-Host "  Binary size   : $binaryMb MB ($binaryBytes bytes)" -ForegroundColor White
Write-Host "`nTo run Hypatia:" -ForegroundColor Cyan
Write-Host "  .\target\release\hypatia.exe                         # Live companion + Mobile Remote" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia.exe --telemetry             # With Telemetry Lab open" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia.exe --demo --telemetry      # Demo mode with synthetic events" -ForegroundColor Gray
Write-Host "====================================================================" -ForegroundColor Green
