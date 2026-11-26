param(
    [Parameter(Mandatory = $true)]
    [string]$Tag,
    [string]$SumsFile = ""
)

$ErrorActionPreference = 'Stop'
$ver = $Tag.TrimStart('v')
$repoRoot = Split-Path -Parent $PSScriptRoot
$utf8NoBom = [System.Text.UTF8Encoding]::new($false)

if (-not $SumsFile) {
    $SumsFile = Join-Path $env:TEMP "batt-tray-SHA256SUMS-$Tag.txt"
    gh release download $Tag -R fawk-qp/batt-tray -p SHA256SUMS.txt -O $SumsFile --clobber
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

$hashes = @{}
foreach ($line in Get-Content -LiteralPath $SumsFile) {
    if ($line -match '^\s*([0-9a-fA-F]{64})\s+\*?(.+?)\s*$') {
        $hashes[$matches[2]] = $matches[1].ToLowerInvariant()
    }
}

$winHash = $hashes['batt-tray-win-rust.exe']
$linuxHash = $hashes['batt-tray-linux-rust']
if (-not $winHash -or -not $linuxHash) {
    throw "Missing expected hashes in $SumsFile"
}

$scoopPath = Join-Path $repoRoot 'bucket\batt-tray.json'
$scoop = Get-Content -LiteralPath $scoopPath -Raw | ConvertFrom-Json
$scoop.version = $ver
$scoop.architecture.'64bit'.url = "https://github.com/fawk-qp/batt-tray/releases/download/$Tag/batt-tray-win-rust.exe"
$scoop.architecture.'64bit'.hash = $winHash
[System.IO.File]::WriteAllText($scoopPath, (($scoop | ConvertTo-Json -Depth 10) + "`n"), $utf8NoBom)

$aurPath = Join-Path $repoRoot 'packaging\aur\PKGBUILD'
$aur = Get-Content -LiteralPath $aurPath -Raw
$aur = $aur -replace '(?m)^pkgver=.*$', "pkgver=$ver"
$aur = $aur -replace "(?m)^sha256sums=\('[0-9a-fA-F]+' 'SKIP'\)$", "sha256sums=('$linuxHash' 'SKIP')"
[System.IO.File]::WriteAllText($aurPath, $aur, $utf8NoBom)

$winUpper = $winHash.ToUpperInvariant()
$wingetDir = Join-Path $repoRoot 'packaging\winget'

$verYaml = Join-Path $wingetDir 'fawk-qp.BattTray.yaml'
$verText = (Get-Content -LiteralPath $verYaml -Raw) -replace '(?m)^PackageVersion: .*$', "PackageVersion: $ver"
[System.IO.File]::WriteAllText($verYaml, $verText, $utf8NoBom)

$locYaml = Join-Path $wingetDir 'fawk-qp.BattTray.locale.en-US.yaml'
$locText = (Get-Content -LiteralPath $locYaml -Raw) -replace '(?m)^PackageVersion: .*$', "PackageVersion: $ver"
[System.IO.File]::WriteAllText($locYaml, $locText, $utf8NoBom)

$instYaml = Join-Path $wingetDir 'fawk-qp.BattTray.installer.yaml'
$instText = Get-Content -LiteralPath $instYaml -Raw
$instText = $instText -replace '(?m)^PackageVersion: .*$', "PackageVersion: $ver"
$instText = $instText -replace '(?m)^(\s*InstallerUrl: ).*$', "`${1}https://github.com/fawk-qp/batt-tray/releases/download/$Tag/batt-tray-win-rust.exe"
$instText = $instText -replace '(?m)^(\s*InstallerSha256: ).*$', "`${1}$winUpper"
[System.IO.File]::WriteAllText($instYaml, $instText, $utf8NoBom)
