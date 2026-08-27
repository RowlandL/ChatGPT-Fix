[CmdletBinding()]
param([string]$SourceFile)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrEmpty($scriptRoot)) { $scriptRoot = (Get-Location).Path }
if ([string]::IsNullOrEmpty($SourceFile)) { $SourceFile = Join-Path $scriptRoot '..\setup-winforms\SetupForm.cs' }
$source = Get-Content -LiteralPath ([IO.Path]::GetFullPath($SourceFile)) -Raw
$manager = Get-Content -LiteralPath ([IO.Path]::GetFullPath((Join-Path $scriptRoot '..\src\chatgpt-fix-manager\src\main.rs'))) -Raw
$rustSetup = Get-Content -LiteralPath ([IO.Path]::GetFullPath((Join-Path $scriptRoot '..\src\chatgpt-fix-setup\src\main.rs'))) -Raw
$dq = [string][char]34
function Has {
    param([string]$needle)
    return $source.Contains($needle)
}
function Must {
    param([bool]$condition, [string]$message)
    if (-not $condition) { throw $message }
}

Must (-not (Has ('chatgpt-fix-manager' + $dq + ', ' + $dq + 'scripts' + $dq))) 'source-tree injector fallback is still present'
Must (Has 'RelativeDestination = Path.Combine') 'managed scripts payload is missing'
Must (Has 'inject-locale-i18n.js') 'locale injector payload is missing'
Must (Has 'payload') 'missing payload does not fail closed'
Must (Has 'if (key == null) throw new IOException') 'null uninstall registry key is silently accepted'
Must (Has 'Launcher') 'launcher startup failure has no warning contract'
Must (Has 'EnsureNoReparseComponents') 'path validation lacks reparse-component check'
Must (Has 'FileAttributes.ReparsePoint') 'path validation does not inspect ReparsePoint attributes'
Must (Has 'ClearExternalSetupUpdatePending') 'external rerun pending cleanup is missing'
Must (Has 'QuoteWindowsArgument') 'robocopy argument quoting helper is missing'
Must (Has ('ProcessStartInfo(' + $dq + 'robocopy.exe' + $dq)) 'robocopy is not invoked directly'
Must (-not (Has 'batContent')) 'batch-file command construction remains'
Must (Has 'FindWindowsPowerShell') 'absolute Windows PowerShell resolver is missing'
Must (Has 'throw new FileNotFoundException') 'missing Windows PowerShell does not fail closed'
Must (-not (Has 'FindPowerShell')) 'Windows PowerShell resolver still falls back to PATH'
Must ($manager.Contains('WindowsPowerShell') -and $manager.Contains('Result<PathBuf, String>')) 'Manager PowerShell resolver is not absolute/fail-closed'
Must (-not $manager.Contains('fn which(')) 'Manager still has a PATH helper resolver'
Must ($rustSetup.Contains('WindowsPowerShell') -and $rustSetup.Contains('Result<PathBuf, String>')) 'Rust Setup PowerShell resolver is not absolute/fail-closed'
Must (-not $rustSetup.Contains('fn which(')) 'Rust Setup still has a PATH helper resolver'
Must ($rustSetup.Contains("Sort-Object Version -Descending | Select-Object -First 1")) 'Rust Setup does not select the newest registered official package'
Must (-not (Has 'NeedsCopy(')) 'weak three-key baseline copy gate remains'
Must ($source.Contains('CopyTree(Path.Combine(appRoot, "app"), appDst, total);')) 'install does not always reconcile official app tree'
Must ($source.Contains('RunProcessBounded')) 'Setup process execution is not routed through bounded helper'
Must (-not (Has 'if (!EnsureNtc(installRoot, baseline))')) 'Setup still auto-enables the disabled NTC injector'
Write-Output 'SETUP HARDENING STATIC TESTS PASSED'
