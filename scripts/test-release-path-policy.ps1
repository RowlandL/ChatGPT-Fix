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

Write-Output 'release path policy passed'
