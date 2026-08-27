[CmdletBinding()]
param([string]$RepositoryRoot)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrEmpty($RepositoryRoot)) {
    $scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
    if ([string]::IsNullOrEmpty($scriptRoot)) { $scriptRoot = (Get-Location).Path }
    $repo = [IO.Path]::GetFullPath((Join-Path $scriptRoot '..'))
} else {
    $repo = [IO.Path]::GetFullPath($RepositoryRoot)
}
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('ChatGPT-Fix-Setup-Isolation-' + [Guid]::NewGuid().ToString('N'))
$payload = Join-Path $tempRoot 'payload'
$install = Join-Path $tempRoot 'install'
$bin = Join-Path $payload 'bin'
New-Item -ItemType Directory -Path $bin -Force | Out-Null
New-Item -ItemType Directory -Path $install -Force | Out-Null

try {
    $source = Join-Path $repo 'setup-winforms\SetupForm.cs'
    $manifest = Join-Path $repo 'setup-winforms\app.manifest'
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $compiler -PathType Leaf)) { throw "csc.exe unavailable: $compiler" }
    $setupExe = Join-Path $tempRoot 'Setup.exe'
    & $compiler /nologo /target:winexe "/out:$setupExe" "/win32manifest:$manifest" $source
    if ($LASTEXITCODE -ne 0) { throw "csc failed: $LASTEXITCODE" }
    $assembly = [Reflection.Assembly]::LoadFrom($setupExe)
    $setupType = $assembly.GetType('ChatGPTFixSetup.SetupForm', $true)
    $flags = [Reflection.BindingFlags] 'NonPublic,Static'

    # Repair is deliberately limited to shortcut and uninstall-entry repair.
    # Inspect its compiled call sites without invoking any registry/shortcut
    # operation: the EnsureNtc metadata token must not occur in its IL.
    $repair = $setupType.GetMethod('RepairShortcutAndRegistry', [Reflection.BindingFlags]'Public,Instance')
    $ensureNtc = $setupType.GetMethod('EnsureNtc', [Reflection.BindingFlags]'NonPublic,Instance')
    if (-not $repair -or -not $ensureNtc) { throw 'repair isolation methods are missing' }
    $repairIl = $repair.GetMethodBody().GetILAsByteArray()
    $ensureNtcToken = [BitConverter]::GetBytes($ensureNtc.MetadataToken)
    for ($offset = 0; $offset -le $repairIl.Length - $ensureNtcToken.Length; $offset++) {
        $matches = $true
        for ($index = 0; $index -lt $ensureNtcToken.Length; $index++) {
            if ($repairIl[$offset + $index] -ne $ensureNtcToken[$index]) { $matches = $false; break }
        }
        if ($matches) { throw 'repair still calls EnsureNtc and can modify the current baseline' }
    }
    Write-Output 'REPAIR BASELINE ISOLATION TEST PASSED'

    # All Setup child processes share one bounded wait primitive. A deliberate
    # short timeout must return promptly and must not depend on ReadToEnd.
    $runProcess = $setupType.GetMethod('RunProcessBounded', $flags)
    if (-not $runProcess) { throw 'bounded Setup process helper is missing' }
    $sleepInfo = [Diagnostics.ProcessStartInfo]::new()
    $sleepInfo.FileName = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $sleepInfo.Arguments = '-NoProfile -NonInteractive -Command "Start-Sleep -Seconds 5"'
    $sleepInfo.UseShellExecute = $false
    $sleepInfo.CreateNoWindow = $true
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $timedOut = $false
    try {
        $runProcess.Invoke($null, [object[]]@($sleepInfo, [int]100, [string]'bounded helper test')) | Out-Null
    } catch {
        $timedOut = $_.Exception.ToString().Contains('timeout') -or $_.Exception.ToString().Contains('超时')
    }
    $clock.Stop()
    if (-not $timedOut) { throw 'bounded Setup process helper did not report timeout' }
    if ($clock.ElapsedMilliseconds -ge 5000) { throw 'bounded Setup process helper exceeded its cleanup bound' }
    Write-Output 'BOUNDED SETUP PROCESS TEST PASSED'

    # Missing injector: all five executable payload entries exist, but the
    # colocated script does not. The transaction must fail before scripts/ is written.
    foreach ($name in @('ChatGPT-Fix-Launcher.exe','ChatGPT-Fix-Manager.exe','ChatGPT-Fix-Packer.exe','ChatGPT-Fix-Setup.exe','ChatGPT-Fix-Locale.exe')) {
        [IO.File]::WriteAllText((Join-Path $payload $name), 'test')
    }
    $deploy = $setupType.GetMethod('DeployPayloadTransactional', $flags)
    try {
        $deploy.Invoke($null, [object[]]@([string]$install, [string]$payload)) | Out-Null
        throw 'missing injector unexpectedly succeeded'
    } catch {
        if (-not $_.Exception.ToString().Contains('payload')) { throw }
    }
    if (Test-Path -LiteralPath (Join-Path $install 'scripts\inject-native-token-cost.js')) {
        throw 'missing injector wrote managed script'
    }
    Write-Output 'MISSING INJECTOR FAIL-CLOSED TEST PASSED'

    # A deployed payload keeps its backup receipt until current.json is
    # published. Simulate a pre-current failure and prove all five old files
    # are restored from that receipt.
    $payloadInstall = Join-Path $tempRoot 'payload-transaction'
    New-Item -ItemType Directory -Path $payloadInstall -Force | Out-Null
    foreach ($name in @('ChatGPT-Fix-Launcher.exe','ChatGPT-Fix-Manager.exe','ChatGPT-Fix-Packer.exe','ChatGPT-Fix-Setup.exe','ChatGPT-Fix-Locale.exe')) {
        [IO.File]::WriteAllText((Join-Path $payload $name), ('new-' + $name))
        $destination = Join-Path $payloadInstall ('bin\' + $name)
        New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
        [IO.File]::WriteAllText($destination, ('old-' + $name))
    }
    [IO.File]::WriteAllText((Join-Path $payload 'inject-native-token-cost.js'), 'new-injector')
    $injectDestination = Join-Path $payloadInstall 'scripts\inject-native-token-cost.js'
    New-Item -ItemType Directory -Path (Split-Path -Parent $injectDestination) -Force | Out-Null
    [IO.File]::WriteAllText($injectDestination, 'old-injector')
    $deployment = $deploy.Invoke($null, [object[]]@([string]$payloadInstall, [string]$payload))
    $rollbackPayload = $setupType.GetMethod('RollbackPayloadDeployment', $flags)
    if (-not $rollbackPayload) { throw 'payload rollback helper is missing' }
    $rollbackPayload.Invoke($null, [object[]]@($deployment)) | Out-Null
    foreach ($name in @('ChatGPT-Fix-Launcher.exe','ChatGPT-Fix-Manager.exe','ChatGPT-Fix-Packer.exe','ChatGPT-Fix-Setup.exe','ChatGPT-Fix-Locale.exe')) {
        $destination = Join-Path $payloadInstall ('bin\' + $name)
        if ((Get-Content -LiteralPath $destination -Raw) -ne ('old-' + $name)) { throw 'payload rollback did not restore ' + $name }
    }
    if ((Get-Content -LiteralPath $injectDestination -Raw) -ne 'old-injector') { throw 'payload rollback did not restore injector' }
    Write-Output 'PAYLOAD PRE-CURRENT ROLLBACK TEST PASSED'

    $dirStats = $setupType.GetMethod('DirStats', $flags)
    if (-not $dirStats) { throw 'DirStats helper is missing' }
    $missingStats = Join-Path $tempRoot 'missing-stats-tree'
    $statsFailed = $false
    try { $dirStats.Invoke($null, [object[]]@([string]$missingStats)) | Out-Null }
    catch { $statsFailed = $_.Exception.ToString().Contains('Directory') -or $_.Exception.ToString().Contains('目录') }
    if (-not $statsFailed) { throw 'DirStats swallowed an enumeration error' }
    Write-Output 'DIR STATS ERROR PROPAGATION TEST PASSED'

    # A repeat install reconciles an existing injected baseline against the
    # official source before the next NTC ensure. The old receipt is removed
    # only after source equality is proven, and the old backup is preserved.
    $ntcRoot = Join-Path $tempRoot 'ntc-reconcile'
    $ntcResources = Join-Path $ntcRoot 'baseline\app\resources'
    $ntcSource = Join-Path $ntcRoot 'source\app\resources'
    New-Item -ItemType Directory -Path $ntcResources, $ntcSource -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $ntcResources 'app.asar'), 'source-asar')
    [IO.File]::WriteAllText((Join-Path $ntcSource 'app.asar'), 'source-asar')
    [IO.File]::WriteAllText((Join-Path $ntcResources 'app.asar.pre-ntc'), 'source-asar')
    $beforeHash = (Get-FileHash (Join-Path $ntcResources 'app.asar.pre-ntc') -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $ntcResources 'ntc-commit.json'),
        '{"schema":"chatgpt_fix.ntc_commit.v1","artifact":"app.asar","before_sha256":"' +
        $beforeHash + '","after_sha256":"' + ('a' * 64) + '"}')
    $reconcile = $setupType.GetMethod('ReconcileNtcAfterBaselineCopy', $flags)
    if (-not $reconcile) { throw 'NTC reconcile helper is missing' }
    $reconcile.Invoke($null, [object[]]@([string]$ntcRoot, [string]$ntcResources, [string]$ntcSource)) | Out-Null
    if (Test-Path (Join-Path $ntcResources 'ntc-commit.json')) { throw 'stale NTC receipt remains after reconcile' }
    if (Test-Path (Join-Path $ntcResources 'app.asar.pre-ntc')) { throw 'old NTC backup was not moved' }
    if (-not (Get-ChildItem $ntcResources -File -Filter 'app.asar.pre-ntc.stale-*')) { throw 'stale NTC backup was not preserved' }
    Write-Output 'NTC STALE TRANSACTION RECONCILIATION TEST PASSED'

    # External rerun cleanup removes only a verified pending Setup and marker.
    $installedSetup = Join-Path $install 'bin\ChatGPT-Fix-Setup.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $installedSetup) -Force | Out-Null
    $pending = $installedSetup + '.pending-v1.0.2'
    [IO.File]::WriteAllText($pending, 'pending')
    $hash = (Get-FileHash -LiteralPath $pending -Algorithm SHA256).Hash.ToLowerInvariant()
    $marker = Join-Path $install 'state\setup-update-pending-v1.0.2.json'
    New-Item -ItemType Directory -Path (Split-Path -Parent $marker) -Force | Out-Null
    $markerText = '{"pending_path":"' + $pending.Replace('\','/') + '","sha256":"' + $hash + '"}'
    [IO.File]::WriteAllText($marker, $markerText)
    $clear = $setupType.GetMethod('ClearExternalSetupUpdatePending', $flags)
    $clear.Invoke($null, [object[]]@([string]$install, [string]$installedSetup)) | Out-Null
    if ((Test-Path -LiteralPath $pending) -or (Test-Path -LiteralPath $marker)) { throw 'verified pending state was not cleaned' }
    Write-Output 'EXTERNAL PENDING CLEANUP TEST PASSED'

    [IO.File]::WriteAllText($pending, 'tampered')
    [IO.File]::WriteAllText($marker, '{"pending_path":"' + $pending.Replace('\','/') + '","sha256":"' + ('0' * 64) + '"}')
    $mismatchFailed = $false
    try { $clear.Invoke($null, [object[]]@([string]$install, [string]$installedSetup)) | Out-Null }
    catch { $mismatchFailed = $true }
    if (-not $mismatchFailed -or -not (Test-Path -LiteralPath $pending) -or -not (Test-Path -LiteralPath $marker)) {
        throw 'mismatched pending state was not rejected and retained for diagnosis'
    }
    Write-Output 'EXTERNAL PENDING MISMATCH FAIL-CLOSED TEST PASSED'

    # Exercise Windows argument quoting with metacharacters that would be
    # interpreted by cmd.exe if a batch file were used.
    $quote = $setupType.GetMethod('QuoteWindowsArgument', $flags)
    foreach ($value in @("C:\path with spaces\a'b&c^d%e!f\", 'C:\plain')) {
        $quoted = [string]$quote.Invoke($null, [object[]]@([string]$value))
        if (-not ($quoted.StartsWith('"') -and $quoted.EndsWith('"'))) { throw "argument not quoted: $value" }
    }
    Write-Output 'ROBOCOPY ARGUMENT QUOTING TEST PASSED'

    # v1.0.1 created ChatGPT.lnk as a product-owned baseline entry. The
    # v1.0.2 migration must recognize that exact legacy shape so it can be
    # upgraded in place without leaving a duplicate official shortcut.
    $legacyOfficial = Join-Path $tempRoot 'ChatGPT.lnk'
    $legacyTarget = Join-Path $install 'baselines\fixture\app\ChatGPT.exe'
    New-Item -ItemType Directory -Path (Split-Path -Parent $legacyTarget) -Force | Out-Null
    [IO.File]::WriteAllText($legacyTarget, 'fixture')
    $legacyShell = New-Object -ComObject WScript.Shell
    $legacyShortcut = $legacyShell.CreateShortcut($legacyOfficial)
    $legacyShortcut.TargetPath = $legacyTarget
    $legacyShortcut.Arguments = ''
    $legacyShortcut.WorkingDirectory = Split-Path -Parent $legacyTarget
    $legacyShortcut.Description = 'ChatGPT'
    $legacyShortcut.Save()
    $legacyOfficialMethod = $setupType.GetMethod('IsLegacyV101OfficialShortcut', $flags)
    if (-not $legacyOfficialMethod) { throw 'legacy official shortcut ownership helper is missing' }
    $legacyOwned = [bool]$legacyOfficialMethod.Invoke($null, [object[]]@([string]$legacyOfficial, [string]$install))
    if (-not $legacyOwned) { throw 'legacy product-owned official shortcut was not recognized' }

    foreach ($invalid in @(
        @{ Path = (Join-Path $tempRoot 'ChatGPT (official).lnk'); Target = $legacyTarget; Arguments = ''; WorkingDirectory = (Split-Path -Parent $legacyTarget); Description = 'ChatGPT'; Label = 'fallback name' },
        @{ Path = $legacyOfficial; Target = 'C:\Program Files\WindowsApps\OpenAI.Codex_fixture\app\ChatGPT.exe'; Arguments = ''; WorkingDirectory = 'C:\Program Files\WindowsApps\OpenAI.Codex_fixture\app'; Description = 'ChatGPT'; Label = 'WindowsApps target' },
        @{ Path = $legacyOfficial; Target = $legacyTarget; Arguments = '--user-data-dir=C:\user'; WorkingDirectory = (Split-Path -Parent $legacyTarget); Description = 'ChatGPT'; Label = 'arguments' },
        @{ Path = $legacyOfficial; Target = $legacyTarget; Arguments = ''; WorkingDirectory = $install; Description = 'ChatGPT'; Label = 'working directory' },
        @{ Path = $legacyOfficial; Target = $legacyTarget; Arguments = ''; WorkingDirectory = (Split-Path -Parent $legacyTarget); Description = 'ChatGPT official entry'; Label = 'description' }
    )) {
        $candidate = $legacyShell.CreateShortcut($invalid.Path)
        $candidate.TargetPath = $invalid.Target
        $candidate.Arguments = $invalid.Arguments
        $candidate.WorkingDirectory = $invalid.WorkingDirectory
        $candidate.Description = $invalid.Description
        $candidate.Save()
        if ([bool]$legacyOfficialMethod.Invoke($null, [object[]]@([string]$invalid.Path, [string]$install))) {
            throw ('legacy official ownership accepted invalid ' + $invalid.Label)
        }
    }
    Write-Output 'LEGACY OFFICIAL SHORTCUT MIGRATION TEST PASSED'

    # A v1.0.2 ownership marker can still point at the fallback name from an
    # earlier collision. A valid legacy standard name takes priority, is
    # upgraded in place, and only the known v1.0.2 fallback is removed.
    $shellRoot = Join-Path $tempRoot 'shell'
    New-Item -ItemType Directory -Path $shellRoot -Force | Out-Null
    $migrationLegacy = Join-Path $shellRoot 'ChatGPT.lnk'
    $migrationLegacyShortcut = $legacyShell.CreateShortcut($migrationLegacy)
    $migrationLegacyShortcut.TargetPath = $legacyTarget
    $migrationLegacyShortcut.Arguments = ''
    $migrationLegacyShortcut.WorkingDirectory = Split-Path -Parent $legacyTarget
    $migrationLegacyShortcut.Description = 'ChatGPT'
    $migrationLegacyShortcut.Save()
    $migrationFallback = Join-Path $shellRoot 'ChatGPT (official).lnk'
    $windows = $env:SystemRoot
    $migrationFallbackShortcut = $legacyShell.CreateShortcut($migrationFallback)
    $migrationFallbackShortcut.TargetPath = (Join-Path $windows 'explorer.exe')
    $migrationFallbackShortcut.Arguments = 'shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App'
    $migrationFallbackShortcut.WorkingDirectory = $windows
    $migrationFallbackShortcut.Description = 'ChatGPT official entry (managed by ChatGPT-Fix v1.0.2)'
    $migrationFallbackShortcut.Save()
    New-Item -ItemType Directory -Path (Join-Path $install 'bin') -Force | Out-Null
    $installedLauncher = Join-Path $install 'bin\ChatGPT-Fix-Launcher.exe'
    [IO.File]::WriteAllText($installedLauncher, 'fixture')
    $migrationLegacyWrapper = Join-Path $shellRoot 'ChatGPT-Fix-Launcher.lnk'
    $migrationLegacyWrapperShortcut = $legacyShell.CreateShortcut($migrationLegacyWrapper)
    $migrationLegacyWrapperShortcut.TargetPath = $installedLauncher
    $migrationLegacyWrapperShortcut.Arguments = ''
    $migrationLegacyWrapperShortcut.WorkingDirectory = Split-Path -Parent $installedLauncher
    $migrationLegacyWrapperShortcut.Description = ''
    $migrationLegacyWrapperShortcut.Save()
    $migrationWrapperFallback = Join-Path $shellRoot 'ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk'
    $migrationWrapperFallbackShortcut = $legacyShell.CreateShortcut($migrationWrapperFallback)
    $migrationWrapperFallbackShortcut.TargetPath = $installedLauncher
    $migrationWrapperFallbackShortcut.Arguments = ''
    $migrationWrapperFallbackShortcut.WorkingDirectory = Split-Path -Parent $installedLauncher
    $migrationWrapperFallbackShortcut.Description = 'ChatGPT-Fix Launcher wrapper (managed by ChatGPT-Fix v1.0.2)'
    $migrationWrapperFallbackShortcut.Save()
    New-Item -ItemType Directory -Path (Join-Path $install 'state') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $install 'state\shortcut-ownership.json'), '{"schema":"chatgpt_fix.shortcut_ownership.v1","official_name":"ChatGPT (official).lnk","wrapper_name":"ChatGPT-Fix-Launcher (ChatGPT-Fix).lnk"}')
    $createShortcuts = $setupType.GetMethod('CreateShortcuts', $flags)
    if (-not $createShortcuts) { throw 'shortcut creation helper is missing' }
    $savedTesting = [Environment]::GetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING')
    $savedInstallRoot = [Environment]::GetEnvironmentVariable('TEST_INSTALL_ROOT')
    $savedShellRoot = [Environment]::GetEnvironmentVariable('TEST_SHELL_ROOT')
    $savedUninstallSubkey = [Environment]::GetEnvironmentVariable('TEST_UNINSTALL_SUBKEY')
    try {
        [Environment]::SetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING', '1')
        [Environment]::SetEnvironmentVariable('TEST_INSTALL_ROOT', $install)
        [Environment]::SetEnvironmentVariable('TEST_SHELL_ROOT', $shellRoot)
        [Environment]::SetEnvironmentVariable('TEST_UNINSTALL_SUBKEY', 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix-Test')
        $createShortcuts.Invoke($null, [object[]]@([string]$install)) | Out-Null
    } finally {
        [Environment]::SetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING', $savedTesting)
        [Environment]::SetEnvironmentVariable('TEST_INSTALL_ROOT', $savedInstallRoot)
        [Environment]::SetEnvironmentVariable('TEST_SHELL_ROOT', $savedShellRoot)
        [Environment]::SetEnvironmentVariable('TEST_UNINSTALL_SUBKEY', $savedUninstallSubkey)
    }
    $migratedShortcut = $legacyShell.CreateShortcut($migrationLegacy)
    if ($migratedShortcut.TargetPath -ne (Join-Path $windows 'explorer.exe') -or
        $migratedShortcut.Arguments -ne 'shell:AppsFolder\OpenAI.Codex_2p2nqsd0c76g0!App' -or
        $migratedShortcut.WorkingDirectory -ne $windows -or
        $migratedShortcut.Description -ne 'ChatGPT official entry (managed by ChatGPT-Fix v1.0.5)') {
        throw 'legacy standard shortcut was not upgraded to the v1.0.5 official entry'
    }
    if (Test-Path -LiteralPath $migrationFallback) { throw 'verified v1.0.2 fallback shortcut was not removed after standard-name migration' }
    $migratedWrapper = $legacyShell.CreateShortcut($migrationLegacyWrapper)
    if ($migratedWrapper.TargetPath -ne $installedLauncher -or
        $migratedWrapper.Arguments -ne '' -or
        $migratedWrapper.WorkingDirectory -ne (Split-Path -Parent $installedLauncher) -or
        $migratedWrapper.Description -ne 'ChatGPT-Fix Launcher wrapper (managed by ChatGPT-Fix v1.0.5)') {
        throw 'legacy standard wrapper was not upgraded to the v1.0.5 managed entry'
    }
    if (Test-Path -LiteralPath $migrationWrapperFallback) { throw 'verified v1.0.2 wrapper fallback was not removed after standard-name migration' }
    if ((Get-Content -LiteralPath (Join-Path $install 'state\shortcut-ownership.json') -Raw) -notmatch '"official_name":"ChatGPT\.lnk"') {
        throw 'shortcut ownership marker did not move back to ChatGPT.lnk'
    }
    if ((Get-Content -LiteralPath (Join-Path $install 'state\shortcut-ownership.json') -Raw) -notmatch '"wrapper_name":"ChatGPT-Fix-Launcher\.lnk"') {
        throw 'shortcut ownership marker did not move back to ChatGPT-Fix-Launcher.lnk'
    }
    $userFallback = $legacyShell.CreateShortcut($migrationFallback)
    $userFallback.TargetPath = (Join-Path $windows 'notepad.exe')
    $userFallback.Arguments = ''
    $userFallback.WorkingDirectory = $windows
    $userFallback.Description = 'User shortcut'
    $userFallback.Save()
    $userWrapperFallback = $legacyShell.CreateShortcut($migrationWrapperFallback)
    $userWrapperFallback.TargetPath = (Join-Path $windows 'notepad.exe')
    $userWrapperFallback.Arguments = '--user-wrapper'
    $userWrapperFallback.WorkingDirectory = $windows
    $userWrapperFallback.Description = 'User wrapper shortcut'
    $userWrapperFallback.Save()
    $savedTesting = [Environment]::GetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING')
    $savedInstallRoot = [Environment]::GetEnvironmentVariable('TEST_INSTALL_ROOT')
    $savedShellRoot = [Environment]::GetEnvironmentVariable('TEST_SHELL_ROOT')
    $savedUninstallSubkey = [Environment]::GetEnvironmentVariable('TEST_UNINSTALL_SUBKEY')
    try {
        [Environment]::SetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING', '1')
        [Environment]::SetEnvironmentVariable('TEST_INSTALL_ROOT', $install)
        [Environment]::SetEnvironmentVariable('TEST_SHELL_ROOT', $shellRoot)
        [Environment]::SetEnvironmentVariable('TEST_UNINSTALL_SUBKEY', 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix-Test')
        $createShortcuts.Invoke($null, [object[]]@([string]$install)) | Out-Null
    } finally {
        [Environment]::SetEnvironmentVariable('CODEX_NTFS_FIX_SETUP_TESTING', $savedTesting)
        [Environment]::SetEnvironmentVariable('TEST_INSTALL_ROOT', $savedInstallRoot)
        [Environment]::SetEnvironmentVariable('TEST_SHELL_ROOT', $savedShellRoot)
        [Environment]::SetEnvironmentVariable('TEST_UNINSTALL_SUBKEY', $savedUninstallSubkey)
    }
    if (-not (Test-Path -LiteralPath $migrationFallback)) { throw 'unowned fallback shortcut was deleted during migration' }
    if (-not (Test-Path -LiteralPath $migrationWrapperFallback)) { throw 'unowned wrapper fallback shortcut was deleted during migration' }
    Write-Output 'LEGACY OFFICIAL SHORTCUT SELECTION TEST PASSED'

    # Missing fixed System32 Windows PowerShell must fail closed; PATH is not
    # consulted as an alternate executable source.
    $oldSystemRoot = [Environment]::GetEnvironmentVariable('SystemRoot')
    [Environment]::SetEnvironmentVariable('SystemRoot', $tempRoot)
    $findPs = $setupType.GetMethod('FindWindowsPowerShell', $flags)
    $psFailed = $false
    try {
        $findPs.Invoke($null, $null) | Out-Null
    } catch {
        $psFailed = $true
        if (-not $_.Exception.ToString().Contains('Windows PowerShell')) { throw }
    } finally {
        [Environment]::SetEnvironmentVariable('SystemRoot', $oldSystemRoot)
    }
    if (-not $psFailed) { throw 'missing Windows PowerShell unexpectedly resolved' }
    Write-Output 'POWERSHELL ABSOLUTE PATH FAIL-CLOSED TEST PASSED'

    # Real reparse isolation test where the host permits symlink creation.
    $target = Join-Path $tempRoot 'real-target'
    $link = Join-Path $tempRoot 'ChatGPT-Fix-Test-link'
    New-Item -ItemType Directory -Path $target -Force | Out-Null
    try {
        New-Item -ItemType SymbolicLink -Path $link -Target $target -ErrorAction Stop | Out-Null
        $cfgType = $setupType.GetNestedType('TestConfig', $flags)
        $cfg = [Activator]::CreateInstance($cfgType, $true)
        $cfg.install_root = Join-Path $link 'install'
        $cfg.shell_root = Join-Path $tempRoot 'shell'
        $cfg.uninstall_subkey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix-Test'
        $validate = $setupType.GetMethod('ValidateTestConfig', $flags)
        $reparseFailed = $false
        try {
            $validate.Invoke($null, [object[]]@($cfg, [string]'symlink-test')) | Out-Null
        } catch {
            $reparseFailed = $true
            if (-not $_.Exception.ToString().Contains('reparse')) { throw }
        }
        if (-not $reparseFailed) { throw 'reparse path unexpectedly accepted' }
        Write-Output 'REPARSE COMPONENT REJECTION TEST PASSED'
    } catch {
        if ($_.Exception.Message -like '*privilege*' -or $_.Exception.Message -like '*access*' -or $_.Exception.Message -like '*symbolic*') {
            Write-Output 'REPARSE COMPONENT TEST SKIPPED (symlink unavailable)'
        } else { throw }
    }
} finally {
    if (Test-Path -LiteralPath $tempRoot) { Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue }
}
