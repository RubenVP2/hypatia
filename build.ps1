# ==============================================================================
# Unified build script for Hypatia (Rust + WebAssembly)
# ==============================================================================
# 1. Compiles the WebAssembly frontend with Trunk in release mode
# 2. Compiles the Windows standalone server binary with Cargo in release mode
# 3. Verifies size criteria and outputs a summary
# ==============================================================================

$ErrorActionPreference = "Stop"

$sw = [System.Diagnostics.Stopwatch]::StartNew()

Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "  HYPATIA - Standalone Release Build" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan

# ------------------------------------------------------------------------------
# Step 1: WebAssembly frontend compilation (Trunk)
# ------------------------------------------------------------------------------
Write-Host "`n[1/3] Compiling WASM frontend (crates/web)..." -ForegroundColor Yellow
if (Get-Command trunk -ErrorAction SilentlyContinue) {
    Push-Location (Join-Path $PSScriptRoot "crates/web")
    trunk build --release
    $trunkExit = $LASTEXITCODE
    Pop-Location
    if ($trunkExit -ne 0) {
        Write-Host "`n[ERROR] Trunk build failed with exit code $trunkExit." -ForegroundColor Red
        exit $trunkExit
    }
} else {
    $normalized = $PSScriptRoot.Replace('\', '/') + '/crates/web'
    $wslPath = (wsl wslpath -u $normalized).Trim()
    $trunkCommand = "export PATH=`"`$HOME/.cargo/bin:`$PATH`" && cd '$wslPath' && trunk build --release"
    wsl -e bash -lc $trunkCommand
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`n[ERROR] Trunk build (WSL) failed with exit code $LASTEXITCODE." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

$distDir = "crates/web/dist"
$distIndex = Join-Path $distDir "index.html"
if (-not (Test-Path $distIndex)) {
    Write-Host "`n[ERROR] $distIndex was not found after Trunk build!" -ForegroundColor Red
    exit 1
}

$wasmFiles = Get-ChildItem -Path $distDir -Filter "*.wasm"
$wasmSizeKb = 0
if ($wasmFiles.Count -gt 0) {
    $wasmSizeKb = [math]::Round($wasmFiles[0].Length / 1KB, 2)
}
Write-Host "  -> WASM frontend successfully generated ($wasmSizeKb KB)." -ForegroundColor Green

# ------------------------------------------------------------------------------
# Step 2: Windows native server binary compilation (Cargo release)
# ------------------------------------------------------------------------------
Write-Host "`n[2/3] Compiling Windows native server binary (hypatia-server)..." -ForegroundColor Yellow
cargo build --release -p hypatia-server

if ($LASTEXITCODE -ne 0) {
    Write-Host "`n[ERROR] cargo build --release failed with exit code $LASTEXITCODE." -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "  -> Server successfully compiled." -ForegroundColor Green

# ------------------------------------------------------------------------------
# Step 3: Binary verification and summary
# ------------------------------------------------------------------------------
Write-Host "`n[3/3] Verifying final binary..." -ForegroundColor Yellow
$binaryPath = "target/release/hypatia-server.exe"

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

if ($binaryMb -le 25.0) {
    Write-Host "  Size budget   : PASSED (<= 25 MB)" -ForegroundColor Green
} else {
    Write-Host "  Size budget   : EXCEEDED (> 25 MB)" -ForegroundColor Red
}

Write-Host "`nTo run Hypatia:" -ForegroundColor Cyan
Write-Host "  .\target\release\hypatia-server.exe              # Standard mode (opens browser)" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia-server.exe --demo       # Demo mode with synthetic stream" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia-server.exe --no-open    # Without auto-opening browser" -ForegroundColor Gray
Write-Host "====================================================================" -ForegroundColor Green
