[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$OutputDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$ReleaseVersion,

    [string]$SourceFile = (Join-Path $PSScriptRoot '..\setup-winforms\SetupForm.cs'),
    [string]$ManifestFile = (Join-Path $PSScriptRoot '..\setup-winforms\app.manifest')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-StagingDirectory {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = [System.IO.Path]::GetFullPath($Path)
    $root = [System.IO.Path]::GetPathRoot($resolved)
    if ($resolved.TrimEnd('\') -eq $root.TrimEnd('\')) {
        throw "OutputDirectory must be a child staging directory, not a volume root: $resolved"
    }
    if ($resolved -match '(?i)\\WindowsApps(\\|$)' -or $resolved -match '(?i)\\Programs\\ChatGPT-Fix(\\|$)') {
        throw "OutputDirectory must not be an official-package or installed-product path: $resolved"
    }
    return $resolved
}

$source = [System.IO.Path]::GetFullPath($SourceFile)
$manifest = [System.IO.Path]::GetFullPath($ManifestFile)
$output = Resolve-StagingDirectory -Path $OutputDirectory

foreach ($path in @($source, $manifest)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Required WinForms build input is missing: $path"
    }
}

$sourceText = Get-Content -LiteralPath $source -Raw
$expectedAssemblyVersion = $ReleaseVersion + '.0'
foreach ($requiredDeclaration in @(
    ('AssemblyVersion("' + $expectedAssemblyVersion + '")'),
    ('AssemblyFileVersion("' + $expectedAssemblyVersion + '")'),
    ('AssemblyInformationalVersion("' + $ReleaseVersion + '")')
)) {
    if (-not $sourceText.Contains($requiredDeclaration)) {
        throw ("SetupForm.cs does not fan out release version {0}: missing {1}" -f $ReleaseVersion, $requiredDeclaration)
    }
}

$csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $csc -PathType Leaf)) {
    throw "Required .NET Framework compiler was not found: $csc"
}

New-Item -ItemType Directory -Path $output -Force | Out-Null
$setupExe = Join-Path $output 'ChatGPT-Fix-Setup.exe'
& $csc /nologo /target:winexe "/out:$setupExe" "/win32manifest:$manifest" $source
if ($LASTEXITCODE -ne 0) {
    throw "Framework64 csc failed with exit code $LASTEXITCODE"
}
if (-not (Test-Path -LiteralPath $setupExe -PathType Leaf)) {
    throw "Framework64 csc completed without producing $setupExe"
}

$version = (Get-Item -LiteralPath $setupExe).VersionInfo
$expectedFileVersion = $ReleaseVersion + '.0'
if ($version.FileVersion -ne $expectedFileVersion -or $version.ProductVersion -ne $ReleaseVersion) {
    throw "Compiled Setup PE version does not match $ReleaseVersion (expected file=$expectedFileVersion, product=$ReleaseVersion; file=$($version.FileVersion), product=$($version.ProductVersion))"
}

[pscustomobject]@{
    artifact = $setupExe
    file_version = $version.FileVersion
    product_version = $version.ProductVersion
    sha256 = (Get-FileHash -LiteralPath $setupExe -Algorithm SHA256).Hash.ToLowerInvariant()
    executed_installer = $false
} | ConvertTo-Json -Compress
