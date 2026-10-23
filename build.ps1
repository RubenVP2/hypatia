# ==============================================================================
# Script de build unifié pour Hypatia (Rust + WebAssembly)
# ==============================================================================
# 1. Compile le frontend WebAssembly sous WSL avec Trunk en mode release
# 2. Compile le binaire serveur autonome Windows avec Cargo en mode release
# 3. Vérifie les critères de taille et présente un récapitulatif
# ==============================================================================

$ErrorActionPreference = "Stop"

$sw = [System.Diagnostics.Stopwatch]::StartNew()

Write-Host "====================================================================" -ForegroundColor Cyan
Write-Host "  HYPATIA - Build de release autonome" -ForegroundColor Cyan
Write-Host "====================================================================" -ForegroundColor Cyan

# ------------------------------------------------------------------------------
# Étape 1 : Compilation du frontend WebAssembly (Trunk)
# ------------------------------------------------------------------------------
Write-Host "`n[1/3] Compilation du frontend WASM (crates/web)..." -ForegroundColor Yellow
if (Get-Command trunk -ErrorAction SilentlyContinue) {
    Push-Location (Join-Path $PSScriptRoot "crates/web")
    trunk build --release
    $trunkExit = $LASTEXITCODE
    Pop-Location
    if ($trunkExit -ne 0) {
        Write-Host "`n[ERREUR] La compilation Trunk a échoué avec le code $trunkExit." -ForegroundColor Red
        exit $trunkExit
    }
} else {
    $normalized = $PSScriptRoot.Replace('\', '/') + '/crates/web'
    $wslPath = (wsl wslpath -u $normalized).Trim()
    $trunkCommand = "export PATH=`"`$HOME/.cargo/bin:`$PATH`" && cd '$wslPath' && trunk build --release"
    wsl -e bash -lc $trunkCommand
    if ($LASTEXITCODE -ne 0) {
        Write-Host "`n[ERREUR] La compilation Trunk (WSL) a échoué avec le code $LASTEXITCODE." -ForegroundColor Red
        exit $LASTEXITCODE
    }
}

$distDir = "crates/web/dist"
$distIndex = Join-Path $distDir "index.html"
if (-not (Test-Path $distIndex)) {
    Write-Host "`n[ERREUR] $distIndex est introuvable après la compilation Trunk !" -ForegroundColor Red
    exit 1
}

$wasmFiles = Get-ChildItem -Path $distDir -Filter "*.wasm"
$wasmSizeKb = 0
if ($wasmFiles.Count -gt 0) {
    $wasmSizeKb = [math]::Round($wasmFiles[0].Length / 1KB, 2)
}
Write-Host "  -> Frontend WASM généré avec succès ($wasmSizeKb Ko)." -ForegroundColor Green

# ------------------------------------------------------------------------------
# Étape 2 : Compilation du binaire serveur Windows (Cargo release)
# ------------------------------------------------------------------------------
Write-Host "`n[2/3] Compilation du binaire serveur natif Windows (hypatia-server)..." -ForegroundColor Yellow
cargo build --release -p hypatia-server

if ($LASTEXITCODE -ne 0) {
    Write-Host "`n[ERREUR] La compilation cargo build --release a échoué avec le code $LASTEXITCODE." -ForegroundColor Red
    exit $LASTEXITCODE
}
Write-Host "  -> Serveur compilé avec succès." -ForegroundColor Green

# ------------------------------------------------------------------------------
# Étape 3 : Vérification du binaire et résumé
# ------------------------------------------------------------------------------
Write-Host "`n[3/3] Vérification du binaire final..." -ForegroundColor Yellow
$binaryPath = "target/release/hypatia-server.exe"

if (-not (Test-Path $binaryPath)) {
    Write-Host "`n[ERREUR] Le binaire $binaryPath est introuvable !" -ForegroundColor Red
    exit 1
}

$binaryItem = Get-Item $binaryPath
$binaryBytes = $binaryItem.Length
$binaryMb = [math]::Round($binaryBytes / 1MB, 2)
$sw.Stop()
$totalSec = [math]::Round($sw.Elapsed.TotalSeconds, 1)

Write-Host "`n====================================================================" -ForegroundColor Green
Write-Host "  BUILD RÉUSSI AVEC SUCCÈS en $totalSec s" -ForegroundColor Green
Write-Host "====================================================================" -ForegroundColor Green
Write-Host "  Binaire produit   : $binaryPath" -ForegroundColor White
Write-Host "  Taille du binaire : $binaryMb Mo ($binaryBytes octets)" -ForegroundColor White

if ($binaryMb -le 25.0) {
    Write-Host "  Budget de taille  : RESPECTÉ (<= 25 Mo)" -ForegroundColor Green
} else {
    Write-Host "  Budget de taille  : DÉPASSÉ (> 25 Mo)" -ForegroundColor Red
}

Write-Host "`nPour exécuter Hypatia :" -ForegroundColor Cyan
Write-Host "  .\target\release\hypatia-server.exe              # Mode standard (ouvre le navigateur)" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia-server.exe --demo       # Mode démo avec flux synthétique" -ForegroundColor Gray
Write-Host "  .\target\release\hypatia-server.exe --no-open    # Sans ouverture auto du navigateur" -ForegroundColor Gray
Write-Host "====================================================================" -ForegroundColor Green
