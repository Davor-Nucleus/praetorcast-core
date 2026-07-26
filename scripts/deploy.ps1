<#
.SYNOPSIS
    Compile praetorcast-core et déploie le binaire ET ses ressources vers le dossier
    d'exécution.

.DESCRIPTION
    Le code source et le dossier d'exécution sont deux arbres distincts. Askama compile
    les templates dans le binaire, mais `public/` et `data/` sont lus depuis le disque,
    au chemin relatif au dossier de lancement. Recopier le seul .exe laisse donc des
    ressources périmées ou absentes — d'où des overlays en 404 ou une fonctionnalité
    qui démarre vide.

    Ce script fait les deux. Il ne SUPPRIME jamais rien côté exécution, et il ne touche
    pas aux fichiers d'état déjà présents dans `data/` sauf demande explicite.

.PARAMETER Runtime
    Dossier d'exécution. Par défaut le dossier PraetorCast voisin.

.PARAMETER OverwriteData
    Écrase aussi les `data/*.json` du dossier d'exécution. À utiliser en connaissance
    de cause : ce sont les configurations en cours (bannières, planning, récompenses).

.PARAMETER SkipBuild
    Déploie le binaire déjà compilé sans relancer cargo.

.EXAMPLE
    .\scripts\deploy.ps1
.EXAMPLE
    .\scripts\deploy.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    [string] $Runtime = (Join-Path (Split-Path $PSScriptRoot -Parent) '..\PraetorCast'),
    [switch] $OverwriteData,
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'

$source = Split-Path $PSScriptRoot -Parent
if (-not (Test-Path $Runtime)) { throw "Dossier d'exécution introuvable : $Runtime" }
$Runtime = (Resolve-Path $Runtime).Path

Write-Host "Source  : $source"
Write-Host "Runtime : $Runtime"

# ── 1. Compilation ───────────────────────────────────────────────────────────
if (-not $SkipBuild) {
    Write-Host "`n[1/4] cargo build --release" -ForegroundColor Cyan
    Push-Location $source
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build a échoué (code $LASTEXITCODE)" }
    } finally { Pop-Location }
} else {
    Write-Host "`n[1/4] compilation ignorée (-SkipBuild)" -ForegroundColor DarkGray
}

# ── 2. Binaire ───────────────────────────────────────────────────────────────
# Un exe en cours d'exécution ne peut pas être écrasé sous Windows.
Write-Host "`n[2/4] binaire" -ForegroundColor Cyan
$running = Get-Process -Name 'praetorcast-core' -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "  praetorcast-core tourne (pid $($running.Id -join ', ')) — arrêt" -ForegroundColor Yellow
    $running | Stop-Process -Force
    Start-Sleep -Milliseconds 500
}
Copy-Item (Join-Path $source 'target\release\praetorcast-core.exe') (Join-Path $Runtime 'praetorcast-core.exe') -Force
Write-Host "  praetorcast-core.exe copié"

# ── 3. Ressources publiques ──────────────────────────────────────────────────
# `public/js` est un asset de code : on l'aligne systématiquement.
# Les dossiers d'uploads sont du contenu utilisateur : on complète sans écraser.
Write-Host "`n[3/4] public/" -ForegroundColor Cyan
$assetDirs  = @('js')
$uploadDirs = @('banner', 'scheduler', 'channelpoint', 'font')

foreach ($dir in $assetDirs) {
    $from = Join-Path $source "public\$dir"
    if (-not (Test-Path $from)) { continue }
    $to = Join-Path $Runtime "public\$dir"
    New-Item -ItemType Directory -Force $to | Out-Null
    Copy-Item "$from\*" $to -Recurse -Force
    Write-Host "  public/$dir aligné"
}

foreach ($dir in $uploadDirs) {
    $from = Join-Path $source "public\$dir"
    if (-not (Test-Path $from)) { continue }
    $to = Join-Path $Runtime "public\$dir"
    New-Item -ItemType Directory -Force $to | Out-Null
    $added = 0
    foreach ($file in Get-ChildItem $from -File) {
        $target = Join-Path $to $file.Name
        if (-not (Test-Path $target)) { Copy-Item $file.FullName $target; $added++ }
    }
    Write-Host "  public/$dir : $added fichier(s) ajouté(s)"
}

# ── 4. Ponts Node et état ────────────────────────────────────────────────────
Write-Host "`n[4/4] ws/ et data/" -ForegroundColor Cyan
foreach ($bridge in Get-ChildItem $source -File -Filter 'ws_*.cjs' -ErrorAction SilentlyContinue) {
    $to = Join-Path $Runtime "ws\$($bridge.Name)"
    if (Test-Path (Split-Path $to -Parent)) {
        Copy-Item $bridge.FullName $to -Force
        Write-Host "  ws/$($bridge.Name) aligné"
    }
}

$dataDir = Join-Path $Runtime 'data'
New-Item -ItemType Directory -Force $dataDir | Out-Null
foreach ($file in Get-ChildItem (Join-Path $source 'data') -File -Filter '*.json') {
    $target = Join-Path $dataDir $file.Name
    if (Test-Path $target) {
        if ($OverwriteData) {
            Copy-Item $file.FullName $target -Force
            Write-Host "  data/$($file.Name) ÉCRASÉ (-OverwriteData)" -ForegroundColor Yellow
        } else {
            Write-Host "  data/$($file.Name) conservé (état en cours)" -ForegroundColor DarkGray
        }
    } else {
        Copy-Item $file.FullName $target
        Write-Host "  data/$($file.Name) créé"
    }
}

Write-Host "`nDéploiement terminé." -ForegroundColor Green
Write-Host "Relancer avec : $Runtime\start\start.bat"
