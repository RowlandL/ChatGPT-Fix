[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repo = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$fixedPackage = 'C:\Users\32893\Downloads\ChatGPT-Fix-1.0.5-fixed'
$temp = Join-Path ([IO.Path]::GetTempPath()) ('ChatGPT-Fix-Framework-LongPath-' + [Guid]::NewGuid().ToString('N'))
$package = Join-Path $temp 'OpenAI.Codex_26.820.9563.0_x64__fixture'
$install = Join-Path $temp 'install'
$shell = Join-Path $temp 'shell'
$setup = Join-Path $temp 'ChatGPT-Fix-Setup.exe'
$subkey = 'Software\Microsoft\Windows\CurrentVersion\Uninstall\ChatGPT-Fix-Framework-' + [Guid]::NewGuid().ToString('N')

try {
    foreach ($dir in @($temp, $package, $install, $shell)) {
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
    }
    $source = Join-Path $repo 'setup-winforms\SetupForm.cs'
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    & $compiler /nologo /target:winexe "/out:$setup" $source
    if ($LASTEXITCODE -ne 0) { throw "csc failed: $LASTEXITCODE" }

    $app = Join-Path $package 'app'
    $resources = Join-Path $app 'resources'
    New-Item -ItemType Directory -Path $resources -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $app 'ChatGPT.exe'), 'fake-chatgpt')
    [IO.File]::WriteAllText((Join-Path $resources 'app.asar'), 'fake-asar')
    [IO.File]::WriteAllText((Join-Path $resources 'icon-chatgpt.ico'), 'fake-icon')
    $deep = $app
    for ($index = 0; $index -lt 9; $index++) {
        $deep = Join-Path $deep ('segment-' + ('z' * 24))
    }
    New-Item -ItemType Directory -Path $deep -Force | Out-Null
    $deepFile = Join-Path $deep 'deep-fixture.txt'
    [IO.File]::WriteAllText($deepFile, 'long-path-fixture')
    if ($deepFile.Length -le 260) { throw 'fixture did not exceed MAX_PATH' }

    foreach ($name in @('ChatGPT-Fix-Launcher.exe','ChatGPT-Fix-Manager.exe','ChatGPT-Fix-Packer.exe','ChatGPT-Fix-Locale.exe')) {
        Copy-Item -LiteralPath (Join-Path $fixedPackage $name) -Destination (Join-Path $temp $name) -Force
    }
    New-Item -ItemType Directory -Path (Join-Path $temp 'scripts') -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $fixedPackage 'scripts\inject-native-token-cost.js') -Destination (Join-Path $temp 'scripts\inject-native-token-cost.js') -Force
    $config = [ordered]@{
        install_root = $install
        shell_root = $shell
        uninstall_subkey = $subkey
        package_root = $package
        skip_launch = '1'
    } | ConvertTo-Json -Compress
    [IO.File]::WriteAllText((Join-Path $temp 'test-config.json'), $config, (New-Object Text.UTF8Encoding($false)))

    $process = Start-Process -FilePath $setup -ArgumentList '--test-install' -WorkingDirectory $temp -Wait -PassThru -WindowStyle Hidden
    $log = Join-Path $temp 'test-install.log'
    $logText = if (Test-Path -LiteralPath $log) { [string](Get-Content -Raw -LiteralPath $log) } else { '' }
    $baseline = Join-Path $install ('baselines\' + (Get-Item -LiteralPath $package).Name)
    $relativeDeep = $deep.Substring($package.Length + 1)
    $copiedDeep = Join-Path $baseline $relativeDeep
    $state = if (Test-Path -LiteralPath (Join-Path $baseline 'state.json')) {
        Get-Content -Raw -LiteralPath (Join-Path $baseline 'state.json') | ConvertFrom-Json
    } else { $null }
    [pscustomobject]@{
        ExitCode = $process.ExitCode
        DeepPathLength = $deepFile.Length
        InstallLog = $logText
        CurrentExists = Test-Path -LiteralPath (Join-Path $install 'current.json')
        StateExists = $null -ne $state
        SourceVersion = if ($null -ne $state) { [string]$state.source_version } else { $null }
        ManifestCount = if ($null -ne $state) { @($state.source_hash_manifest).Count } else { 0 }
        DeepFileCopied = Test-Path -LiteralPath $copiedDeep
    } | ConvertTo-Json -Depth 6
    if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $install 'current.json')) -or
        -not (Test-Path -LiteralPath (Join-Path $baseline 'state.json')) -or -not (Test-Path -LiteralPath $copiedDeep)) {
        throw 'real .NET Framework Setup long-path integration failed'
    }
} finally {
    try { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree($subkey, $false) } catch { }
    if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Output 'FRAMEWORK LONG-PATH INSTALL TEST PASSED'
