[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptPath = Join-Path $PSScriptRoot 'verify-release.ps1'
$source = Get-Content -LiteralPath $scriptPath -Raw

foreach ($marker in @('D:\\project', 'C:\\Users')) {
    if ($source -match [regex]::Escape($marker)) {
        throw "verify-release.ps1 contains machine-specific path marker: $marker"
    }
}

if ($source -notmatch 'function\s+Get-BuildPathMarkers') {
    throw 'verify-release.ps1 must derive forbidden build paths from the current build environment'
}

if ($source -notmatch 'function\s+Assert-ReleaseBinariesHaveNoBuildPath') {
    throw 'verify-release.ps1 must scan every release executable for build paths'
}

if ($source -notmatch 'Assert-ReleaseBinariesHaveNoBuildPath\s+-Paths\s+\$artifactPaths') {
    throw 'verify-release.ps1 must pass the complete release executable set to the path scanner'
}
if ($source -notmatch '\[System\.IO\.Path\]::GetRelativePath\(\$staging,\s*\$path\)') {
    throw 'verify-release.ps1 must keep artifact paths relative for publishable evidence'
}
if ($source -notmatch 'staging_directory\s*=\s*''\.''') {
    throw 'verify-release.ps1 must not publish the local staging directory'
}

$downloadSource = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'verify-downloaded-release.ps1') -Raw
if ($downloadSource -notmatch '\[IO\.Path\]::IsPathRooted\(\[string\]\$artifact\.path\)') {
    throw 'verify-downloaded-release.ps1 must reject absolute artifact paths'
}

$ciSource = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\.github\workflows\ci.yml') -Raw
if ($ciSource -notmatch 'chatgpt-fix-locale\.exe') {
    throw 'CI release staging must include ChatGPT-Fix-Locale.exe'
}

$releaseWorkflow = Get-Content -LiteralPath (Join-Path $PSScriptRoot '..\.github\workflows\release-v1.0.5.yml') -Raw
if ($releaseWorkflow -notmatch 'prepare-local-release\.ps1') {
    throw 'v1.0.5 release workflow must use the current local release candidate generator'
}

Write-Output 'release path policy passed'
