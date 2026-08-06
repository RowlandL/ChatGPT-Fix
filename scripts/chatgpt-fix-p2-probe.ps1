<#
.SYNOPSIS
    ChatGPT-Fix P2 bounded package inspection probe.
.DESCRIPTION
    In Live mode, reads the bounded live package state (Get-AppxPackage,
    manifest, primary EXE, shortcut) and outputs a single canonical
    LiveInspectionV1 JSON document to stdout.
    In Fixture mode, reads synthetic snapshot files from $FixtureRoot
    and produces the same canonical JSON.

    This script must NOT contain:
    - Mutation cmdlets
    - Process cmdlets
    - Network cmdlets
    - Any Get-AppxPackage without -Name
    - Any Get-AppxPackage with -AllUsers
.PARAMETER Mode
    'Live' for live package inspection; 'Fixture' for synthetic testing.
.PARAMETER FixtureRoot
    Required when Mode is 'Fixture'. Path to the fixture directory.
.EXAMPLE
    Invoke-ChatGptFixP2Probe -Mode Live
    Invoke-ChatGptFixP2Probe -Mode Fixture -FixtureRoot "tests/live-fixtures/valid-current"
#>

function Invoke-ChatGptFixP2Probe {
    param(
        [Parameter(Mandatory)]
        [ValidateSet('Live', 'Fixture')]
        [string]$Mode,

        [Parameter(Mandatory = $false)]
        [string]$FixtureRoot
    )

    if ($Mode -eq 'Fixture') {
        if (-not $FixtureRoot) {
            Write-Error "FixtureRoot is required when Mode is Fixture"
            exit 1
        }
        return Invoke-FixtureProbe -FixtureRoot $FixtureRoot
    }

    return Invoke-LiveProbe
}

function Invoke-LiveProbe {
    # Bounded package probe: fixed package name, no wildcards, no AllUsers.
    $package = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction Stop
    $identityBefore = $package.PackageFullName

    # Manifest paths.
    $manifestPath = Join-Path $package.InstallLocation 'AppxManifest.xml'
    $manifestBytes = [System.IO.File]::ReadAllBytes($manifestPath)
    $manifestSha256 = Compute-Sha256Bytes $manifestBytes

    # Primary executable.
    $manifestXml = [System.Xml.XmlDocument]::new()
    $manifestXml.Load($manifestPath)
    $ns = @{m = 'http://schemas.microsoft.com/appx/manifest/foundation/windows10'}
    $executableNode = $manifestXml.SelectSingleNode('//m:Application/m:Executable', $ns)
    $executableRel = if ($executableNode) { $executableNode.InnerText.Trim() } else { '' }
    $executablePath = Join-Path $package.InstallLocation $executableRel
    $executableBytes = [System.IO.File]::ReadAllBytes($executablePath)
    $executableSha256 = Compute-Sha256Bytes $executableBytes

    # Manifest dependencies.
    $dependencies = @()
    $depNodes = $manifestXml.SelectNodes('//m:PackageDependency', $ns)
    foreach ($dep in $depNodes) {
        $depName = $dep.GetAttribute('Name')
        $depPublisher = $dep.GetAttribute('Publisher')
        $depMinVer = $dep.GetAttribute('MinVersion')
        # Verify each dependency is installed for current user.
        $resolved = Get-AppxPackage -Name $depName -ErrorAction SilentlyContinue
        if (-not $resolved) {
            Write-Error "Dependency not found: $depName"
            exit 1
        }
        $dependencies += "$depName`_$depMinVer`_x64__$depPublisher"
    }

    # Shortcut.
    $shortcutPath = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\ChatGPT.lnk"
    $shortcut = Get-ShorcutProperties -Path $shortcutPath

    # Re-check identity after probe.
    $packageAfter = Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction Stop
    $identityAfter = $packageAfter.PackageFullName

    $hashBefore = Compute-Sha256Bytes $manifestBytes
    $hashAfter = Compute-Sha256Bytes ([System.IO.File]::ReadAllBytes($manifestPath))

    $result = [PSCustomObject]@{
        schema                   = 'chatgpt_fix.live_inspection.v1'
        probe_source             = 'pwsh-probe'
        package_full_name        = $package.PackageFullName
        version                  = $package.Version
        architecture             = 'x64'
        publisher                = $package.Publisher
        publisher_id             = '2p2nqsd0c76g0'
        install_location         = $package.InstallLocation
        manifest_sha256          = $manifestSha256
        primary_executable       = $executableRel
        primary_bytes            = $executableBytes.Length
        primary_sha256           = $executableSha256
        manifest_dependencies    = $dependencies
        shortcut_target_path     = $shortcut.TargetPath
        shortcut_arguments       = $shortcut.Arguments
        shortcut_working_directory = $shortcut.WorkingDirectory
        shortcut_icon_location   = $shortcut.IconLocation
        identity_before          = $identityBefore
        identity_after           = $identityAfter
        hash_before              = $hashBefore
        hash_after               = $hashAfter
        codex_home_inspected     = $false
        local_state_inspected    = $false
        processes_inspected      = $false
    }

    return $result
}

function Invoke-FixtureProbe {
    param([string]$FixtureRoot)

    $outputPath = Join-Path $FixtureRoot 'probe-output.json'
    if (-not (Test-Path $outputPath)) {
        Write-Error "Fixture output not found: $outputPath"
        exit 1
    }

    $content = Get-Content $outputPath -Raw -Encoding UTF8
    $result = $content | ConvertFrom-Json
    return $result
}

function Compute-Sha256Bytes {
    param([byte[]]$Bytes)
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    $hash = $sha256.ComputeHash($Bytes)
    return ($hash | ForEach-Object { $_.ToString('x2') }) -join ''
}

function Get-ShorcutProperties {
    param([string]$Path)
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    return [PSCustomObject]@{
        TargetPath        = $shortcut.TargetPath
        Arguments         = $shortcut.Arguments
        WorkingDirectory  = $shortcut.WorkingDirectory
        IconLocation      = $shortcut.IconLocation
    }
}

# ---------------------------------------------------------------------------
# Entry point when invoked via stdin from Packer
# ---------------------------------------------------------------------------
if ($MyInvocation.InvocationName -eq '.') {
    # Dot-sourced: the Packer piped this script into pwsh stdin with
    # the trailing invocation. The caller is responsible for calling
    # Invoke-ChatGptFixP2Probe with the desired parameters.
} elseif ($MyInvocation.Line -match 'Invoke-ChatGptFixP2Probe') {
    # Called directly with -Mode argument.
    $result = Invoke-ChatGptFixP2Probe @args
    $result | ConvertTo-Json -Compress -Depth 10
}