[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$OutputDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$ReleaseVersion,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$RustArtifactDirectory,

    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$SetupArtifact,

    [string]$RepositoryRoot = (Join-Path $PSScriptRoot '..')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-Directory {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Label)

    if (-not (Test-Path -LiteralPath $Path -PathType Container)) {
        throw "$Label does not exist or is not a directory: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).ProviderPath
}

function Resolve-File {
    param([Parameter(Mandatory)][string]$Path, [Parameter(Mandatory)][string]$Label)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label does not exist or is not a file: $Path"
    }
    return (Resolve-Path -LiteralPath $Path).ProviderPath
}

function Assert-SafeCandidateDirectory {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($resolved)
    if ($resolved.TrimEnd('\') -eq $root.TrimEnd('\')) {
        throw "OutputDirectory must be a dedicated child directory: $resolved"
    }
    if ($resolved -match '(?i)\\WindowsApps(\\|$)' -or
        $resolved -match '(?i)\\Programs\\ChatGPT-Fix(\\|$)') {
        throw "OutputDirectory must not be an official-package or installed-product path: $resolved"
    }
    return $resolved
}

function Get-Hash {
    param([Parameter(Mandatory)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

$repository = Resolve-Directory -Path $RepositoryRoot -Label 'RepositoryRoot'
$rustArtifacts = Resolve-Directory -Path $RustArtifactDirectory -Label 'RustArtifactDirectory'
$setup = Resolve-File -Path $SetupArtifact -Label 'SetupArtifact'
$output = Assert-SafeCandidateDirectory -Path $OutputDirectory

if ($repository -match '(?i)\\WindowsApps(\\|$)' -or
    $repository -match '(?i)\\Programs\\ChatGPT-Fix(\\|$)') {
    throw "RepositoryRoot is not a source checkout: $repository"
}

if (Test-Path -LiteralPath $output) {
    $existing = @(Get-ChildItem -LiteralPath $output -Force)
    if ($existing.Count -gt 0) {
        throw "OutputDirectory must be empty to prevent stale release assets: $output"
    }
} else {
    New-Item -ItemType Directory -Path $output -Force | Out-Null
}

$versionFile = Join-Path $repository 'VERSION'
$actualVersion = (Get-Content -LiteralPath $versionFile -Raw).Trim()
if ($actualVersion -ne $ReleaseVersion) {
    throw "VERSION must equal $ReleaseVersion; found $actualVersion"
}
$cargo = Get-Content -LiteralPath (Join-Path $repository 'Cargo.toml') -Raw
if ($cargo -notmatch ('(?m)^version\s*=\s*"' + [regex]::Escape($ReleaseVersion) + '"\s*$')) {
    throw "Cargo workspace version is not $ReleaseVersion"
}

$safeDirectory = $repository.Replace('/', '\')
$commit = (& git -C $repository -c "safe.directory=$safeDirectory" rev-parse HEAD 2>$null).Trim()
if ($LASTEXITCODE -ne 0 -or $commit -notmatch '^[0-9a-fA-F]{40}$') {
    throw "Unable to resolve a full source commit for $repository"
}
$commit = $commit.ToLowerInvariant()

$binaryMap = [ordered]@{
    'ChatGPT-Fix-Launcher.exe' = 'chatgpt-fix-launcher.exe'
    'ChatGPT-Fix-Manager.exe'  = 'chatgpt-fix-manager.exe'
    'ChatGPT-Fix-Packer.exe'   = 'chatgpt-fix-packer.exe'
    'ChatGPT-Fix-Locale.exe'   = 'chatgpt-fix-locale.exe'
}
$injector = Resolve-File -Path (Join-Path $repository 'src\chatgpt-fix-manager\scripts\inject-native-token-cost.js') -Label 'managed injector'
$localeInjector = Resolve-File -Path (Join-Path $repository 'src\chatgpt-fix-locale\scripts\inject-locale-i18n.js') -Label 'locale injector'
$package = Join-Path $output 'package'
New-Item -ItemType Directory -Path (Join-Path $package 'scripts') -Force | Out-Null

foreach ($publishedName in $binaryMap.Keys) {
    $source = Resolve-File -Path (Join-Path $rustArtifacts $binaryMap[$publishedName]) -Label $publishedName
    Copy-Item -LiteralPath $source -Destination (Join-Path $package $publishedName)
    Copy-Item -LiteralPath $source -Destination (Join-Path $output $publishedName)
}
Copy-Item -LiteralPath $setup -Destination (Join-Path $package 'ChatGPT-Fix-Setup.exe')
Copy-Item -LiteralPath $setup -Destination (Join-Path $output 'ChatGPT-Fix-Setup.exe')
Copy-Item -LiteralPath $injector -Destination (Join-Path $package 'scripts\inject-native-token-cost.js')
Copy-Item -LiteralPath $injector -Destination (Join-Path $package 'inject-native-token-cost.js')
Copy-Item -LiteralPath $injector -Destination (Join-Path $output 'inject-native-token-cost.js')
Copy-Item -LiteralPath $localeInjector -Destination (Join-Path $package 'scripts\inject-locale-i18n.js')
Copy-Item -LiteralPath $localeInjector -Destination (Join-Path $package 'inject-locale-i18n.js')
Copy-Item -LiteralPath $localeInjector -Destination (Join-Path $output 'inject-locale-i18n.js')

$setupInfo = (Get-Item -LiteralPath (Join-Path $output 'ChatGPT-Fix-Setup.exe')).VersionInfo
if ($setupInfo.FileVersion -ne "$ReleaseVersion.0" -or $setupInfo.ProductVersion -ne $ReleaseVersion) {
    throw "Setup PE version does not match $ReleaseVersion"
}

$setupBuild = [ordered]@{
    schema = 'chatgpt_fix.setup_build.v1'
    version = $ReleaseVersion
    source_commit = $commit
    artifact = 'ChatGPT-Fix-Setup.exe'
    file_version = $setupInfo.FileVersion
    product_version = $setupInfo.ProductVersion
    sha256 = Get-Hash (Join-Path $output 'ChatGPT-Fix-Setup.exe')
    toolchain = 'csc .NET Framework 4.0.30319 (x64); WinForms (SetupForm.cs)'
    executed_installer = $false
}
$setupBuild | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'setup-build.json') -Encoding utf8

$pwsh = (Get-Command pwsh.exe -ErrorAction SilentlyContinue).Source
if ([string]::IsNullOrWhiteSpace($pwsh)) {
    throw 'PowerShell 7 (pwsh.exe) is required for the release verification scripts'
}
$verifyRelease = Join-Path $repository 'scripts\verify-release.ps1'
$verificationText = & $pwsh -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $verifyRelease `
    -StagingDirectory $package -ReleaseVersion $ReleaseVersion -RepositoryRoot $repository
if ($LASTEXITCODE -ne 0) {
    throw "verify-release.ps1 failed with exit code $LASTEXITCODE"
}
$verificationText | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding utf8

# The verifier needs both supported source layouts. The distributable zip keeps
# only the managed scripts/ layout required by the installer contract.
Remove-Item -LiteralPath (Join-Path $package 'inject-native-token-cost.js') -Force
Remove-Item -LiteralPath (Join-Path $package 'inject-locale-i18n.js') -Force
$zipName = "ChatGPT-Fix-v$ReleaseVersion-win-x64.zip"
$zipPath = Join-Path $output $zipName
Compress-Archive -Path (Join-Path $package '*') -DestinationPath $zipPath -CompressionLevel Optimal

$runtimeNames = @(
    'ChatGPT-Fix-Launcher.exe',
    'ChatGPT-Fix-Manager.exe',
    'ChatGPT-Fix-Packer.exe',
    'ChatGPT-Fix-Setup.exe',
    'ChatGPT-Fix-Locale.exe',
    'inject-native-token-cost.js',
    'inject-locale-i18n.js',
    $zipName
)
$runtimeLines = foreach ($name in $runtimeNames) {
    "$(Get-Hash (Join-Path $output $name))  $name"
}
[ordered]@{
    schema = 'chatgpt_fix.release_receipt.v1'
    version = $ReleaseVersion
    tag = "v$ReleaseVersion"
    source_commit = $commit
    toolchain = 'rustc 1.97.1; csc .NET Framework 4.0.30319 (x64)'
    target = 'x86_64-pc-windows-msvc'
    signing_status = 'unsigned'
    artifacts = @($runtimeLines)
    local_candidate = $true
    cc_switch_modified = $false
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output "ChatGPT-Fix-v$ReleaseVersion-release-receipt.json") -Encoding utf8

[ordered]@{
    spdxVersion = 'SPDX-2.3'
    dataLicense = 'CC0-1.0'
    SPDXID = 'SPDXRef-DOCUMENT'
    name = "ChatGPT-Fix-v$ReleaseVersion-win-x64"
    documentNamespace = "https://github.com/RowlandL/ChatGPT-Fix/sbom/v$ReleaseVersion/$commit"
    creationInfo = [ordered]@{
        created = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
        creators = @('Tool: Local Cargo + PowerShell')
    }
    packages = @([ordered]@{
        name = 'ChatGPT-Fix'
        SPDXID = 'SPDXRef-Package-ChatGPT-Fix'
        versionInfo = $ReleaseVersion
        downloadLocation = 'NOASSERTION'
        filesAnalyzed = $false
        licenseConcluded = 'MIT'
        licenseDeclared = 'MIT'
        copyrightText = 'NOASSERTION'
    })
    relationships = @([ordered]@{
        spdxElementId = 'SPDXRef-DOCUMENT'
        relationshipType = 'DESCRIBES'
        relatedSpdxElement = 'SPDXRef-Package-ChatGPT-Fix'
    })
} | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output "ChatGPT-Fix-v$ReleaseVersion-sbom.spdx.json") -Encoding utf8

$hashedNames = @($runtimeNames) + @(
    "ChatGPT-Fix-v$ReleaseVersion-release-receipt.json",
    "ChatGPT-Fix-v$ReleaseVersion-sbom.spdx.json",
    'setup-build.json',
    'verification.json',
    'RELEASE-NOTES.md'
)
@"
# ChatGPT-Fix v$ReleaseVersion release candidate

Source commit: $commit
Target: x86_64-pc-windows-msvc
Signing: unsigned
Scope: generated from a verified source checkout; artifacts are unsigned and
publication is controlled by the release workflow. No CC Switch mutation.
"@ | Set-Content -LiteralPath (Join-Path $output 'RELEASE-NOTES.md') -Encoding utf8
$sumLines = foreach ($name in $hashedNames) {
    "$(Get-Hash (Join-Path $output $name))  $name"
}
$sumLines | Set-Content -LiteralPath (Join-Path $output 'SHA256SUMS.txt') -Encoding ascii

$verifyDownloaded = Join-Path $repository 'scripts\verify-downloaded-release.ps1'
$downloadText = & $pwsh -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $verifyDownloaded `
    -DownloadDirectory $output -ExpectedVersion $ReleaseVersion -ExpectedSourceCommit $commit
if ($LASTEXITCODE -ne 0) {
    throw "verify-downloaded-release.ps1 failed with exit code $LASTEXITCODE"
}

Remove-Item -LiteralPath $package -Recurse -Force
[ordered]@{
    schema = 'chatgpt_fix.local_release_candidate.v1'
    version = $ReleaseVersion
    source_commit = $commit
    output_directory = $output
    downloaded_release_verification = ($downloadText | ConvertFrom-Json)
    asset_count = @($hashedNames).Count + 1
} | ConvertTo-Json -Depth 6
