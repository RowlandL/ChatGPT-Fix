# ChatGPT-Fix read-only doctor.
#
# This command gathers current local evidence for the Codex Desktop mitigation
# surface. It does not modify WindowsApps, execute installers, terminate
# processes, clean CODEX_HOME, or change shortcuts.

[CmdletBinding()]
param(
    [switch]$Json,
    [string]$ShortcutPath = (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\ChatGPT.lnk'),
    [string]$CodexHome = $(if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }),
    [int]$ProcessAgeMinutes = 10
)

Set-StrictMode -Version 2.0

function ConvertTo-SizeObject {
    param([Nullable[Int64]]$Bytes)

    if ($null -eq $Bytes) {
        return [ordered]@{
            bytes = $null
            mb    = $null
            gb    = $null
        }
    }

    return [ordered]@{
        bytes = [Int64]$Bytes
        mb    = [Math]::Round($Bytes / 1MB, 2)
        gb    = [Math]::Round($Bytes / 1GB, 2)
    }
}

function New-Finding {
    param(
        [string]$Level,
        [string]$Code,
        [string]$Message,
        [object]$Evidence = $null,
        [string]$WouldDo = $null
    )

    return [pscustomobject][ordered]@{
        level    = $Level
        code     = $Code
        message  = $Message
        evidence = $Evidence
        would_do = $WouldDo
    }
}

function Get-FileHashSafe {
    param([string]$Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    try {
        return (Get-FileHash -LiteralPath $Path -Algorithm SHA256 -ErrorAction Stop).Hash
    } catch {
        return [pscustomobject][ordered]@{
            error = $_.Exception.Message
        }
    }
}

function Get-AuthenticodeSignatureSafe {
    param([string]$Path)

    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $null
    }

    try {
        $sig = Get-AuthenticodeSignature -LiteralPath $Path -ErrorAction Stop
        return [pscustomobject][ordered]@{
            status       = [string]$sig.Status
            signer       = if ($sig.SignerCertificate) { $sig.SignerCertificate.Subject } else { $null }
            timestamp_by = if ($sig.TimeStamperCertificate) { $sig.TimeStamperCertificate.Subject } else { $null }
        }
    } catch {
        return [pscustomobject][ordered]@{
            error = $_.Exception.Message
        }
    }
}

function Get-VersionOrNull {
    param([string]$Version)

    if (-not $Version) {
        return $null
    }

    try {
        return [version]$Version
    } catch {
        return $null
    }
}

function Get-LatestCodexPackage {
    $packages = @()

    try {
        $packages = @(Get-AppxPackage -Name 'OpenAI.Codex' -ErrorAction Stop)
    } catch {
        try {
            $packages = @(Get-AppxPackage -ErrorAction Stop | Where-Object { $_.Name -eq 'OpenAI.Codex' -or $_.PackageFullName -like 'OpenAI.Codex_*' })
        } catch {
            return [pscustomobject][ordered]@{
                error = $_.Exception.Message
            }
        }
    }

    if ($packages.Count -eq 0) {
        return [pscustomobject][ordered]@{
            found = $false
        }
    }

    $latest = $packages |
        Sort-Object -Property @{ Expression = { Get-VersionOrNull ([string]$_.Version) }; Descending = $true }, PackageFullName |
        Select-Object -First 1

    $exePath = $null
    if ($latest.InstallLocation) {
        $candidate = Join-Path $latest.InstallLocation 'app\ChatGPT.exe'
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $exePath = $candidate
        } else {
            $candidate = Join-Path $latest.InstallLocation 'ChatGPT.exe'
            if (Test-Path -LiteralPath $candidate -PathType Leaf) {
                $exePath = $candidate
            }
        }
    }

    return [pscustomobject][ordered]@{
        found            = $true
        name             = $latest.Name
        package_full_name = $latest.PackageFullName
        version          = [string]$latest.Version
        install_location = $latest.InstallLocation
        executable_path  = $exePath
        executable_sha256 = Get-FileHashSafe $exePath
    }
}

function Get-ShortcutInfo {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return [pscustomobject][ordered]@{
            found = $false
            path  = $Path
        }
    }

    try {
        $shell = New-Object -ComObject WScript.Shell
        $shortcut = $shell.CreateShortcut($Path)
        return [pscustomobject][ordered]@{
            found             = $true
            path              = $Path
            target_path       = $shortcut.TargetPath
            arguments         = $shortcut.Arguments
            working_directory = $shortcut.WorkingDirectory
            icon_location     = $shortcut.IconLocation
        }
    } catch {
        return [pscustomobject][ordered]@{
            found = $false
            path  = $Path
            error = $_.Exception.Message
        }
    }
}

function Find-MitigationStatePath {
    param([string]$LauncherTarget)

    $candidates = New-Object System.Collections.Generic.List[string]

    if ($LauncherTarget -and (Test-Path -LiteralPath $LauncherTarget)) {
        $item = Get-Item -LiteralPath $LauncherTarget -ErrorAction SilentlyContinue
        if ($item) {
            $dir = if ($item.PSIsContainer) { $item.FullName } else { $item.DirectoryName }
            while ($dir) {
                $candidate = Join-Path $dir 'state.json'
                $candidates.Add($candidate)
                $parent = Split-Path -Parent $dir
                if (-not $parent -or $parent -eq $dir) {
                    break
                }
                $dir = $parent
            }
        }
    }

    $fallback = Join-Path $env:LOCALAPPDATA 'Programs\Codex-NTFS-Fix\baseline\state.json'
    $candidates.Add($fallback)

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return $candidate
        }
    }

    return $null
}

function Get-MitigationState {
    param([string]$LauncherTarget)

    $statePath = Find-MitigationStatePath $LauncherTarget
    if (-not $statePath) {
        return [pscustomobject][ordered]@{
            found = $false
        }
    }

    try {
        $state = Get-Content -LiteralPath $statePath -Raw -ErrorAction Stop | ConvertFrom-Json -ErrorAction Stop
    } catch {
        return [pscustomobject][ordered]@{
            found = $false
            path  = $statePath
            error = $_.Exception.Message
        }
    }

    $managerPath = $null
    if ($state.InstallRoot) {
        $managerCandidate = Join-Path $state.InstallRoot 'manager\Codex-NTFS-Fix-Setup.exe'
        if (Test-Path -LiteralPath $managerCandidate -PathType Leaf) {
            $managerPath = $managerCandidate
        }
    }

    $managerVersion = $null
    if ($managerPath) {
        try {
            $managerVersion = (Get-Item -LiteralPath $managerPath -ErrorAction Stop).VersionInfo.ProductVersion
        } catch {
            $managerVersion = $null
        }
    }

    return [pscustomobject][ordered]@{
        found                    = $true
        path                     = $statePath
        schema                   = $state.Schema
        experimental_mitigation  = $state.ExperimentalMitigation
        install_root             = $state.InstallRoot
        profile_path             = $state.ProfilePath
        source_app_path          = $state.SourceAppPath
        source_package_root      = $state.SourcePackageRoot
        source_package_version   = $state.SourcePackageVersion
        source_trust_status      = $state.SourceTrustStatus
        source_trust_basis       = $state.SourceTrustBasis
        installed_at_utc         = $state.InstalledAtUtc
        chatgpt_sha256_recorded  = $state.ChatGptSha256
        app_asar_sha256_recorded = $state.AppAsarSha256
        manager_path             = $managerPath
        manager_product_version  = $managerVersion
        manager_sha256           = Get-FileHashSafe $managerPath
        manager_signature        = Get-AuthenticodeSignatureSafe $managerPath
    }
}

function Get-CodexStateSize {
    param([string]$Root)

    $sessionsRoot = Join-Path $Root 'sessions'
    $sessionFiles = @()
    if (Test-Path -LiteralPath $sessionsRoot -PathType Container) {
        $sessionFiles = @(Get-ChildItem -LiteralPath $sessionsRoot -Recurse -File -Filter '*.jsonl' -ErrorAction SilentlyContinue)
    }

    $sessionTotal = [Int64]0
    if ($sessionFiles.Count -gt 0) {
        $measure = $sessionFiles | Measure-Object -Property Length -Sum
        if ($measure.Sum) {
            $sessionTotal = [Int64]$measure.Sum
        }
    }

    $largestSessions = @(
        $sessionFiles |
            Sort-Object -Property Length -Descending |
            Select-Object -First 5 |
            ForEach-Object {
                [pscustomobject][ordered]@{
                    path = $_.FullName
                    size = ConvertTo-SizeObject $_.Length
                    last_write_time = $_.LastWriteTime.ToString('o')
                }
            }
    )

    $logs2 = Join-Path $Root 'logs_2.sqlite'
    $logs2Item = Get-Item -LiteralPath $logs2 -ErrorAction SilentlyContinue

    return [pscustomobject][ordered]@{
        codex_home          = $Root
        sessions_root       = $sessionsRoot
        sessions_found      = (Test-Path -LiteralPath $sessionsRoot -PathType Container)
        session_jsonl_count = $sessionFiles.Count
        sessions_total      = ConvertTo-SizeObject $sessionTotal
        largest_sessions    = $largestSessions
        logs_2_sqlite       = if ($logs2Item) {
            [pscustomobject][ordered]@{
                path            = $logs2Item.FullName
                size            = ConvertTo-SizeObject $logs2Item.Length
                last_write_time = $logs2Item.LastWriteTime.ToString('o')
            }
        } else {
            $null
        }
    }
}

function Get-ProcessStartTime {
    param([object]$CimProcess)

    if (-not $CimProcess.CreationDate) {
        return $null
    }

    if ($CimProcess.CreationDate -is [datetime]) {
        return $CimProcess.CreationDate
    }

    try {
        return [Management.ManagementDateTimeConverter]::ToDateTime($CimProcess.CreationDate)
    } catch {
        return $null
    }
}

function Get-CommandKind {
    param([object]$CimProcess)

    $name = [string]$CimProcess.Name
    $commandLine = [string]$CimProcess.CommandLine
    $exePath = [string]$CimProcess.ExecutablePath
    $combined = ($name + ' ' + $exePath + ' ' + $commandLine).ToLowerInvariant()

    if ($combined -match 'bocha') { return 'bocha-mcp' }
    if ($combined -match 'node_repl') { return 'node-repl' }
    if ($combined -match 'open design|open-design') { return 'open-design-mcp' }
    if ($combined -match 'hermes') { return 'hermes-mcp' }
    if ($combined -match 'mcp') { return 'mcp-runtime' }
    if ($name -ieq 'uv.exe') { return 'uv-runtime' }
    if ($name -ieq 'python.exe') { return 'python-runtime' }
    if ($name -ieq 'node.exe') { return 'node-runtime' }
    if ($name -ieq 'pwsh.exe' -or $name -ieq 'powershell.exe') { return 'powershell-runtime' }
    if ($name -ieq 'git.exe') { return 'git-runtime' }
    if ($name -ieq 'codex.exe') { return 'codex-backend' }
    if ($name -ieq 'ChatGPT.exe') { return 'chatgpt-desktop' }

    return 'unknown'
}

function Get-AncestorChain {
    param(
        [int]$ProcessId,
        [hashtable]$ProcessById
    )

    $chain = @()
    $seen = @{}
    $current = $ProcessId

    for ($i = 0; $i -lt 24; $i++) {
        if (-not $ProcessById.ContainsKey($current)) {
            break
        }

        $proc = $ProcessById[$current]
        $parentId = [int]$proc.ParentProcessId
        if ($parentId -le 0 -or $seen.ContainsKey($parentId)) {
            break
        }

        $seen[$parentId] = $true
        if ($ProcessById.ContainsKey($parentId)) {
            $parent = $ProcessById[$parentId]
            $chain += [pscustomobject][ordered]@{
                pid  = [int]$parent.ProcessId
                name = [string]$parent.Name
            }
            $current = $parentId
        } else {
            $chain += [pscustomobject][ordered]@{
                pid  = $parentId
                name = $null
            }
            break
        }
    }

    return $chain
}

function Get-ProcessSnapshot {
    param([int]$AgeMinutes)

    try {
        $cimProcesses = @(Get-CimInstance Win32_Process -ErrorAction Stop)
    } catch {
        return [pscustomobject][ordered]@{
            error = $_.Exception.Message
        }
    }

    $byPid = @{}
    foreach ($proc in $cimProcesses) {
        $byPid[[int]$proc.ProcessId] = $proc
    }

    $perfByPid = @{}
    foreach ($proc in @(Get-Process -ErrorAction SilentlyContinue)) {
        $perfByPid[[int]$proc.Id] = $proc
    }

    $knownChildNames = @(
        'node.exe',
        'node_repl.exe',
        'uv.exe',
        'python.exe',
        'pwsh.exe',
        'powershell.exe',
        'bocha-search-mcp.exe',
        'Open Design.exe',
        'git.exe'
    )

    $rootNames = @('ChatGPT.exe', 'codex.exe')
    $now = Get-Date
    $relevant = @()
    $candidates = @()

    foreach ($proc in $cimProcesses) {
        $kind = Get-CommandKind $proc
        $name = [string]$proc.Name
        $isKnown = $knownChildNames -contains $name -or $rootNames -contains $name -or $kind -ne 'unknown'
        if (-not $isKnown) {
            continue
        }

        $startTime = Get-ProcessStartTime $proc
        $ageMinutes = $null
        if ($startTime) {
            $ageMinutes = [Math]::Round(($now - $startTime).TotalMinutes, 1)
        }

        $ancestors = Get-AncestorChain ([int]$proc.ProcessId) $byPid
        $ancestorNames = @($ancestors | ForEach-Object { $_.name } | Where-Object { $_ })
        $hasCodexAncestor = @($ancestorNames | Where-Object { $_ -in $rootNames }).Count -gt 0

        $perf = $null
        if ($perfByPid.ContainsKey([int]$proc.ProcessId)) {
            $perf = $perfByPid[[int]$proc.ProcessId]
        }

        $summary = [pscustomobject][ordered]@{
            pid                = [int]$proc.ProcessId
            name               = $name
            parent_pid         = [int]$proc.ParentProcessId
            command_kind       = $kind
            start_time         = if ($startTime) { $startTime.ToString('o') } else { $null }
            age_minutes        = $ageMinutes
            has_codex_ancestor = $hasCodexAncestor
            ancestor_chain     = $ancestors
            private_mb         = if ($perf) { [Math]::Round($perf.PrivateMemorySize64 / 1MB, 1) } else { $null }
            working_set_mb     = if ($perf) { [Math]::Round($perf.WorkingSet64 / 1MB, 1) } else { $null }
            handles            = if ($perf) { $perf.Handles } else { $null }
            threads            = if ($perf) { $perf.Threads.Count } else { $null }
        }
        $relevant += $summary

        $oldEnough = $false
        if ($ageMinutes -ne $null -and $ageMinutes -ge $AgeMinutes) {
            $oldEnough = $true
        }

        if (($knownChildNames -contains $name) -and ($hasCodexAncestor -or $oldEnough)) {
            $reason = if ($hasCodexAncestor) {
                'known tool/runtime process currently below ChatGPT.exe or codex.exe'
            } else {
                'known tool/runtime process older than threshold without proving active ownership'
            }

            $candidates += [pscustomobject][ordered]@{
                pid             = $summary.pid
                name            = $summary.name
                parent_pid      = $summary.parent_pid
                command_kind    = $summary.command_kind
                age_minutes     = $summary.age_minutes
                private_mb      = $summary.private_mb
                working_set_mb  = $summary.working_set_mb
                lifecycle_policy_reason = $reason
            }
        }
    }

    $grouped = @(
        $relevant |
            Group-Object -Property name |
            Sort-Object -Property Count -Descending |
            ForEach-Object {
                [pscustomobject][ordered]@{
                    name = $_.Name
                    count = $_.Count
                    private_mb = [Math]::Round((($_.Group | Measure-Object -Property private_mb -Sum).Sum), 1)
                    working_set_mb = [Math]::Round((($_.Group | Measure-Object -Property working_set_mb -Sum).Sum), 1)
                }
            }
    )

    return [pscustomobject][ordered]@{
        age_threshold_minutes = $AgeMinutes
        relevant_process_count = $relevant.Count
        grouped = $grouped
        lifecycle_candidates = @($candidates | Sort-Object -Property name, pid)
    }
}

function Get-RiskLevelForBytes {
    param(
        [Nullable[Int64]]$Bytes,
        [Int64]$Attention,
        [Int64]$Warning,
        [Int64]$High
    )

    if ($null -eq $Bytes) { return 'unknown' }
    if ($Bytes -ge $High) { return 'high' }
    if ($Bytes -ge $Warning) { return 'warning' }
    if ($Bytes -ge $Attention) { return 'attention' }
    return 'ok'
}

$official = Get-LatestCodexPackage
$launcher = Get-ShortcutInfo $ShortcutPath
$mitigation = Get-MitigationState $launcher.target_path
$storage = Get-CodexStateSize $CodexHome
$processes = Get-ProcessSnapshot $ProcessAgeMinutes

$findings = @()

if ($official.found -ne $true) {
    $findings += (New-Finding 'warning' 'official-package-not-found' 'OpenAI.Codex Appx package was not found for the current user.' $official 'verify package registration before copying or launching anything')
}

if ($launcher.found -ne $true) {
    $findings += (New-Finding 'warning' 'shortcut-not-found' 'ChatGPT shortcut could not be read.' $launcher 'locate launcher target before changing shortcuts')
}

if ($official.found -eq $true -and $mitigation.found -eq $true) {
    if ([string]$official.version -ne [string]$mitigation.source_package_version) {
        $findings += (New-Finding 'warning' 'mitigation-source-version-drift' 'The local mitigation baseline was created from a different official package version.' ([pscustomobject][ordered]@{
            official_version = $official.version
            mitigation_source_version = $mitigation.source_package_version
        }) 'would prepare or refresh an isolated side-by-side runtime from the latest official package after review')
    }
}

if ($mitigation.found -eq $true -and $mitigation.manager_signature -and $mitigation.manager_signature.status -ne 'Valid') {
    $findings += (New-Finding 'attention' 'local-manager-not-validly-signed' 'The local mitigation manager is not validly signed.' $mitigation.manager_signature 'would require explicit trust review before executing installer or manager binaries')
}

$candidateCount = 0
if ($processes.lifecycle_candidates) {
    $candidateCount = @($processes.lifecycle_candidates).Count
}
if ($candidateCount -gt 0) {
    $findings += (New-Finding 'attention' 'lifecycle-candidates-present' 'Known Codex tool/runtime processes match local lifecycle ownership rules.' ([pscustomobject][ordered]@{
        count = $candidateCount
        threshold_minutes = $ProcessAgeMinutes
    }) 'would cover owned descendants through the isolated launcher/baseline lifecycle model after review')
}

$sessionsRisk = Get-RiskLevelForBytes $storage.sessions_total.bytes 1GB 10GB 100GB
$logsRisk = 'unknown'
if ($storage.logs_2_sqlite) {
    $logsRisk = Get-RiskLevelForBytes $storage.logs_2_sqlite.size.bytes 1GB 5GB 10GB
}

if ($sessionsRisk -in @('attention', 'warning', 'high')) {
    $findings += (New-Finding $sessionsRisk 'codex-sessions-size-risk' 'CODEX_HOME sessions JSONL storage is large enough to merit review.' $storage.sessions_total 'would externalize or cap image-heavy history only after explicit review')
}

if ($logsRisk -in @('attention', 'warning', 'high')) {
    $findings += (New-Finding $logsRisk 'codex-logs2-size-risk' 'CODEX_HOME logs_2.sqlite is large enough to merit review.' $storage.logs_2_sqlite.size 'would rotate/vacuum/archive logs only after explicit review')
}

$status = 'ok'
if (@($findings | Where-Object { $_.level -eq 'high' }).Count -gt 0) {
    $status = 'high-risk'
} elseif (@($findings | Where-Object { $_.level -eq 'warning' }).Count -gt 0) {
    $status = 'warning'
} elseif (@($findings | Where-Object { $_.level -eq 'attention' }).Count -gt 0) {
    $status = 'attention'
}

$officialVersionForReport = $null
$mitigationVersionForReport = $null
$isDriftedForReport = $null
if ($official.found -eq $true) {
    $officialVersionForReport = $official.version
}
if ($mitigation.found -eq $true) {
    $mitigationVersionForReport = $mitigation.source_package_version
}
if ($official.found -eq $true -and $mitigation.found -eq $true) {
    $isDriftedForReport = ([string]$official.version -ne [string]$mitigation.source_package_version)
}

$report = [pscustomobject][ordered]@{
    schema = 'chatgpt_fix.doctor.v1'
    generated_at = (Get-Date).ToString('o')
    read_only = $true
    status = $status
    official_package = $official
    launcher = $launcher
    local_mitigation = $mitigation
    version_drift = [pscustomobject][ordered]@{
        official_version = $officialVersionForReport
        mitigation_source_version = $mitigationVersionForReport
        is_drifted = $isDriftedForReport
    }
    processes = $processes
    codex_state = $storage
    findings = @($findings)
    dry_run_actions = [pscustomobject][ordered]@{
        would_modify_windowsapps = $false
        would_execute_installer = $false
        would_terminate_processes = $false
        would_clean_codex_home = $false
        would_push_or_publish = $false
    }
}

if ($Json) {
    $report | ConvertTo-Json -Depth 12
    return
}

Write-Output 'ChatGPT-Fix doctor (read-only)'
Write-Output ('Generated: {0}' -f $report.generated_at)
Write-Output ('Status: {0}' -f $report.status)
Write-Output ''

Write-Output 'Official package'
if ($official.found -eq $true) {
    Write-Output ('- Package: {0}' -f $official.package_full_name)
    Write-Output ('- Version: {0}' -f $official.version)
    Write-Output ('- Install location: {0}' -f $official.install_location)
    Write-Output ('- ChatGPT.exe: {0}' -f $official.executable_path)
    Write-Output ('- ChatGPT.exe SHA256: {0}' -f $official.executable_sha256)
} else {
    Write-Output '- Not found'
}
Write-Output ''

Write-Output 'Launcher and local mitigation'
Write-Output ('- Shortcut: {0}' -f $launcher.path)
Write-Output ('- Target: {0}' -f $launcher.target_path)
Write-Output ('- Working directory: {0}' -f $launcher.working_directory)
if ($mitigation.found -eq $true) {
    Write-Output ('- Mitigation state: {0}' -f $mitigation.path)
    Write-Output ('- Source package version: {0}' -f $mitigation.source_package_version)
    Write-Output ('- Experimental mitigation: {0}' -f $mitigation.experimental_mitigation)
    Write-Output ('- Manager: {0}' -f $mitigation.manager_path)
    Write-Output ('- Manager signature: {0}' -f $mitigation.manager_signature.status)
} else {
    Write-Output '- Mitigation state: not found'
}
Write-Output ('- Version drift: {0}' -f $report.version_drift.is_drifted)
Write-Output ''

Write-Output 'Process lifecycle candidates'
Write-Output ('- Candidate count: {0}' -f $candidateCount)
foreach ($candidate in @($processes.lifecycle_candidates | Select-Object -First 20)) {
    Write-Output ('  - PID {0} {1} kind={2} age_min={3} private_mb={4} ws_mb={5}' -f $candidate.pid, $candidate.name, $candidate.command_kind, $candidate.age_minutes, $candidate.private_mb, $candidate.working_set_mb)
}
if ($candidateCount -gt 20) {
    Write-Output ('  - ... {0} more omitted from text output; use -Json for full data' -f ($candidateCount - 20))
}
Write-Output ''

Write-Output 'CODEX_HOME storage'
Write-Output ('- CODEX_HOME: {0}' -f $storage.codex_home)
Write-Output ('- sessions jsonl count: {0}' -f $storage.session_jsonl_count)
Write-Output ('- sessions total: {0} GB' -f $storage.sessions_total.gb)
if ($storage.logs_2_sqlite) {
    Write-Output ('- logs_2.sqlite: {0} GB' -f $storage.logs_2_sqlite.size.gb)
} else {
    Write-Output '- logs_2.sqlite: not found'
}
Write-Output ''

Write-Output 'Findings'
if (@($findings).Count -eq 0) {
    Write-Output '- none'
} else {
    foreach ($finding in $findings) {
        Write-Output ('- [{0}] {1}: {2}' -f $finding.level, $finding.code, $finding.message)
        if ($finding.would_do) {
            Write-Output ('  would_do: {0}' -f $finding.would_do)
        }
    }
}
Write-Output ''
Write-Output 'Dry-run guarantees: no WindowsApps modification, no installer execution, no process termination, no CODEX_HOME cleanup, no publication.'
