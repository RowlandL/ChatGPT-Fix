[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$StagingDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$ReleaseVersion,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$RepositoryRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-RequiredDirectory {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Label)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "$Label does not exist or is not a directory: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).Path
}

function Test-ReleaseText {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Expected, [Parameter(Mandatory)][string]$Label)

    $actual = (Get-Content -LiteralPath $Path -Raw).Trim()
    if ($actual -ne $Expected) {
        throw "$Label must equal $Expected; found $actual"
    }
}

function Assert-ManagerHasNoBuildPath {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$WorkspaceRoot)

    $bytes = [System.IO.File]::ReadAllBytes($Path)
    $text = [System.Text.Encoding]::UTF8.GetString($bytes) + [System.Text.Encoding]::Unicode.GetString($bytes)
    $workspace = $WorkspaceRoot.Replace('/', '\').TrimEnd('\')
    foreach ($value in @('D:\project', 'C:\Users', $workspace)) {
        if ($value -and $text.IndexOf($value, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
            throw "Manager binary contains forbidden build/source path marker: $value"
        }
    }
}

$staging = Resolve-RequiredDirectory -Path $StagingDirectory -Label 'StagingDirectory'
$repository = Resolve-RequiredDirectory -Path $RepositoryRoot -Label 'RepositoryRoot'
$stagingRoot = [System.IO.Path]::GetPathRoot($staging)
if ($staging.TrimEnd('\') -eq $stagingRoot.TrimEnd('\')) {
    throw "StagingDirectory must be a dedicated staging directory, not a volume root: $staging"
}
if ($staging -match '(?i)\\WindowsApps(\\|$)' -or $staging -match '(?i)\\Programs\\ChatGPT-Fix(\\|$)') {
    throw "Release verification refuses an official-package or installed-product path: $staging"
}

Test-ReleaseText -Path (Join-Path $repository 'VERSION') -Expected $ReleaseVersion -Label 'VERSION'
$cargo = Get-Content -LiteralPath (Join-Path $repository 'Cargo.toml') -Raw
if ($cargo -notmatch ('(?m)^version\s*=\s*"' + [regex]::Escape($ReleaseVersion) + '"\s*$')) {
    throw "Cargo workspace version is not $ReleaseVersion"
}
$setupSource = Get-Content -LiteralPath (Join-Path $repository 'setup-winforms\SetupForm.cs') -Raw
foreach ($declaration in @(
    ('AssemblyVersion("' + $ReleaseVersion + '.0")'),
    ('AssemblyFileVersion("' + $ReleaseVersion + '.0")'),
    ('AssemblyInformationalVersion("' + $ReleaseVersion + '")')
)) {
    if (-not $setupSource.Contains($declaration)) {
        throw "Setup version fanout is missing $declaration"
    }
}
$guiSource = Get-Content -LiteralPath (Join-Path $repository 'src\chatgpt-fix-setup\src\gui.rs') -Raw
if ($guiSource -notmatch ('ChatGPT-Fix\s+' + [regex]::Escape($ReleaseVersion))) {
    throw "Setup GUI label does not contain release version $ReleaseVersion"
}

$requiredExecutables = @(
    'ChatGPT-Fix-Launcher.exe',
    'ChatGPT-Fix-Manager.exe',
    'ChatGPT-Fix-Packer.exe',
    'ChatGPT-Fix-Setup.exe'
)
$nestedScript = Join-Path $staging 'scripts\inject-native-token-cost.js'
$flatScript = Join-Path $staging 'inject-native-token-cost.js'
foreach ($path in @($nestedScript, $flatScript)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Release staging is missing required injector layout: $path"
    }
}
$nestedScriptHash = (Get-FileHash -LiteralPath $nestedScript -Algorithm SHA256).Hash.ToLowerInvariant()
$flatScriptHash = (Get-FileHash -LiteralPath $flatScript -Algorithm SHA256).Hash.ToLowerInvariant()
if ($nestedScriptHash -ne $flatScriptHash) {
    throw 'Nested and flat injector payloads do not match'
}

$artifacts = @()
foreach ($name in $requiredExecutables) {
    $path = Join-Path $staging $name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Release payload is missing $name"
    }
    $item = Get-Item -LiteralPath $path
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    $artifacts += [pscustomobject]@{
        name = $name
        path = $path
        sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
        file_version = $item.VersionInfo.FileVersion
        product_version = $item.VersionInfo.ProductVersion
        signature_status = [string]$signature.Status
        signer = if ($signature.SignerCertificate) { $signature.SignerCertificate.Subject } else { $null }
    }
}

$setupVersion = (Get-Item -LiteralPath (Join-Path $staging 'ChatGPT-Fix-Setup.exe')).VersionInfo
$expectedSetupFileVersion = $ReleaseVersion + '.0'
if ($setupVersion.FileVersion -ne $expectedSetupFileVersion -or $setupVersion.ProductVersion -ne $ReleaseVersion) {
    throw "Compiled Setup PE version does not match $ReleaseVersion (expected file=$expectedSetupFileVersion, product=$ReleaseVersion; file=$($setupVersion.FileVersion), product=$($setupVersion.ProductVersion))"
}

foreach ($entry in @(
    @{ name = 'ChatGPT-Fix-Launcher.exe'; prefix = 'ChatGPT-Fix-Launcher' },
    @{ name = 'ChatGPT-Fix-Manager.exe'; prefix = 'ChatGPT-Fix-Manager' },
    @{ name = 'ChatGPT-Fix-Packer.exe'; prefix = 'ChatGPT-Fix-Packer' }
)) {
    $actual = (& (Join-Path $staging $entry.name) --version 2>$null | Out-String)
    $expected = $entry.prefix + ' ' + $ReleaseVersion + [Environment]::NewLine
    if ($actual -ne $expected) {
        throw "$($entry.name) --version output mismatch: expected [$expected], got [$actual]"
    }
}

Assert-ManagerHasNoBuildPath -Path (Join-Path $staging 'ChatGPT-Fix-Manager.exe') -WorkspaceRoot $repository

[pscustomobject]@{
    release_version = $ReleaseVersion
    staging_directory = $staging
    payload_verified = $true
    installer_executed = $false
    artifacts = $artifacts
    required_script = [System.IO.Path]::GetRelativePath($staging, $nestedScript)
    flat_script = [System.IO.Path]::GetRelativePath($staging, $flatScript)
    required_script_sha256 = $nestedScriptHash
} | ConvertTo-Json -Depth 4
