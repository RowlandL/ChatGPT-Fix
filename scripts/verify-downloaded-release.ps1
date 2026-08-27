[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateNotNullOrEmpty()]
    [string]$DownloadDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$ExpectedVersion,

    [Parameter(Mandatory)]
    [ValidatePattern('^[0-9a-fA-F]{40}$')]
    [string]$ExpectedSourceCommit
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = [IO.Path]::GetFullPath($DownloadDirectory)
if (-not (Test-Path -LiteralPath $root -PathType Container)) {
    throw "DownloadDirectory does not exist: $root"
}

$runtimeAssets = @(
    'ChatGPT-Fix-Launcher.exe',
    'ChatGPT-Fix-Manager.exe',
    'ChatGPT-Fix-Packer.exe',
    'ChatGPT-Fix-Setup.exe',
    'ChatGPT-Fix-Locale.exe',
    'inject-native-token-cost.js',
    'inject-locale-i18n.js',
    "ChatGPT-Fix-v$ExpectedVersion-win-x64.zip"
)
$hashedAssets = $runtimeAssets + @(
    "ChatGPT-Fix-v$ExpectedVersion-release-receipt.json",
    "ChatGPT-Fix-v$ExpectedVersion-sbom.spdx.json",
    'setup-build.json',
    'verification.json',
    'RELEASE-NOTES.md'
)
$requiredAssets = $hashedAssets + @('SHA256SUMS.txt')
foreach ($name in $requiredAssets) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $name) -PathType Leaf)) {
        throw "Downloaded release is missing $name"
    }
}

$actualNames = @(Get-ChildItem -LiteralPath $root -File | ForEach-Object Name | Sort-Object)
$expectedNames = @($requiredAssets | Sort-Object)
if (($actualNames -join "`n") -ne ($expectedNames -join "`n")) {
    throw "Downloaded release asset set is not exact. Expected [$($expectedNames -join ', ')], found [$($actualNames -join ', ')]"
}

$sumPath = Join-Path $root 'SHA256SUMS.txt'
$hashes = @{}
foreach ($line in Get-Content -LiteralPath $sumPath) {
    if ($line -notmatch '^([0-9a-f]{64})  ([^\\/]+)$') {
        throw "Invalid SHA256SUMS line: $line"
    }
    $hash = $Matches[1]
    $name = $Matches[2]
    if ($hashes.ContainsKey($name)) { throw "Duplicate SHA256SUMS entry: $name" }
    $hashes[$name] = $hash
}
$sumNames = @($hashes.Keys | Sort-Object)
$expectedHashNames = @($hashedAssets | Sort-Object)
if (($sumNames -join "`n") -ne ($expectedHashNames -join "`n")) {
    throw 'SHA256SUMS.txt does not cover the exact hashed asset set'
}
foreach ($name in $hashedAssets) {
    $actual = (Get-FileHash -LiteralPath (Join-Path $root $name) -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $hashes[$name]) { throw "SHA-256 mismatch: $name" }
}

$receiptName = "ChatGPT-Fix-v$ExpectedVersion-release-receipt.json"
$receipt = Get-Content -LiteralPath (Join-Path $root $receiptName) -Raw | ConvertFrom-Json
if ($receipt.schema -ne 'chatgpt_fix.release_receipt.v1' -or
    $receipt.version -ne $ExpectedVersion -or
    $receipt.tag -ne "v$ExpectedVersion" -or
    $receipt.source_commit -ne $ExpectedSourceCommit.ToLowerInvariant() -or
    $receipt.target -ne 'x86_64-pc-windows-msvc' -or
    $receipt.signing_status -ne 'unsigned') {
    throw 'Release receipt identity or provenance is invalid'
}
$expectedRuntimeLines = @($runtimeAssets | ForEach-Object { $hashes[$_] + '  ' + $_ })
if ((@($receipt.artifacts) -join "`n") -ne ($expectedRuntimeLines -join "`n")) {
    throw 'Release receipt runtime artifact hashes do not match SHA256SUMS.txt'
}

$sbomName = "ChatGPT-Fix-v$ExpectedVersion-sbom.spdx.json"
$sbom = Get-Content -LiteralPath (Join-Path $root $sbomName) -Raw | ConvertFrom-Json
if ($sbom.spdxVersion -ne 'SPDX-2.3' -or
    $sbom.name -ne "ChatGPT-Fix-v$ExpectedVersion-win-x64" -or
    $sbom.documentNamespace -ne "https://github.com/RowlandL/ChatGPT-Fix/sbom/v$ExpectedVersion/$($ExpectedSourceCommit.ToLowerInvariant())" -or
    @($sbom.packages).Count -ne 1 -or
    $sbom.packages[0].versionInfo -ne $ExpectedVersion) {
    throw 'SBOM identity or source commit is invalid'
}

$setupBuild = Get-Content -LiteralPath (Join-Path $root 'setup-build.json') -Raw | ConvertFrom-Json
if ($setupBuild.file_version -ne "$ExpectedVersion.0" -or
    $setupBuild.product_version -ne $ExpectedVersion -or
    $setupBuild.executed_installer -ne $false -or
    $setupBuild.sha256 -ne $hashes['ChatGPT-Fix-Setup.exe']) {
    throw 'Setup build evidence does not match the downloaded Setup executable'
}

$verification = Get-Content -LiteralPath (Join-Path $root 'verification.json') -Raw | ConvertFrom-Json
if ($verification.release_version -ne $ExpectedVersion -or
    $verification.payload_verified -ne $true -or
    $verification.installer_executed -ne $false -or
    $verification.required_script_sha256 -ne $hashes['inject-native-token-cost.js'] -or
    $verification.required_locale_script_sha256 -ne $hashes['inject-locale-i18n.js']) {
    throw 'Release verification evidence is invalid'
}
if ([IO.Path]::IsPathRooted([string]$verification.staging_directory) -or
    [string]$verification.staging_directory -match '(^|[\\/])\.\.([\\/]|$)') {
    throw 'Release verification evidence contains an absolute or escaping staging path'
}
foreach ($artifact in @($verification.artifacts)) {
    if (-not $hashes.ContainsKey($artifact.name) -or $hashes[$artifact.name] -ne $artifact.sha256) {
        throw "Verification evidence hash mismatch: $($artifact.name)"
    }
    if ([IO.Path]::IsPathRooted([string]$artifact.path) -or
        [string]$artifact.path -match '(^|[\\/])\.\.([\\/]|$)') {
        throw "Release verification evidence contains an absolute or escaping artifact path: $($artifact.name)"
    }
}

$extractRoot = Join-Path ([IO.Path]::GetTempPath()) ('ChatGPT-Fix-Verify-' + [guid]::NewGuid().ToString('N'))
try {
    Expand-Archive -LiteralPath (Join-Path $root "ChatGPT-Fix-v$ExpectedVersion-win-x64.zip") -DestinationPath $extractRoot
    $expectedZipFiles = @(
        'ChatGPT-Fix-Launcher.exe',
        'ChatGPT-Fix-Manager.exe',
        'ChatGPT-Fix-Packer.exe',
        'ChatGPT-Fix-Setup.exe',
        'ChatGPT-Fix-Locale.exe',
        'scripts\inject-native-token-cost.js',
        'scripts\inject-locale-i18n.js'
    )
    $actualZipFiles = @(Get-ChildItem -LiteralPath $extractRoot -Recurse -File | ForEach-Object {
        [IO.Path]::GetRelativePath($extractRoot, $_.FullName)
    } | Sort-Object)
    if (($actualZipFiles -join "`n") -ne (($expectedZipFiles | Sort-Object) -join "`n")) {
        throw 'Zip payload file set is not exact'
    }
    foreach ($name in $runtimeAssets[0..4]) {
        $zipHash = (Get-FileHash -LiteralPath (Join-Path $extractRoot $name) -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($zipHash -ne $hashes[$name]) { throw "Zip executable hash mismatch: $name" }
    }
    $zipInjectorHash = (Get-FileHash -LiteralPath (Join-Path $extractRoot 'scripts\inject-native-token-cost.js') -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($zipInjectorHash -ne $hashes['inject-native-token-cost.js']) { throw 'Zip injector hash mismatch' }
    $zipLocaleInjectorHash = (Get-FileHash -LiteralPath (Join-Path $extractRoot 'scripts\inject-locale-i18n.js') -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($zipLocaleInjectorHash -ne $hashes['inject-locale-i18n.js']) { throw 'Zip locale injector hash mismatch' }
}
finally {
    if (Test-Path -LiteralPath $extractRoot) { Remove-Item -LiteralPath $extractRoot -Recurse -Force }
}

[ordered]@{
    schema = 'chatgpt_fix.download_verification.v1'
    version = $ExpectedVersion
    source_commit = $ExpectedSourceCommit.ToLowerInvariant()
    verified = $true
    hashed_assets = $hashedAssets.Count
    zip_payload_files = 7
} | ConvertTo-Json -Depth 3
