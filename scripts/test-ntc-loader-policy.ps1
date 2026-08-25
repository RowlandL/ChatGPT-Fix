[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$scriptPath = Join-Path $PSScriptRoot '..\src\chatgpt-fix-manager\scripts\inject-native-token-cost.js'
$source = Get-Content -LiteralPath $scriptPath -Raw

foreach ($marker in @(
    "function attachNtcToContents",
    "webContents.getAllWebContents()",
    "contents.__chatgptFixNtcAttached",
    "contents.once('dom-ready'",
    "app.on('web-contents-created'"
)) {
    if ($source -notmatch [regex]::Escape($marker)) {
        throw "inject-native-token-cost.js is missing loader hardening marker: $marker"
    }
}

Write-Output 'ntc loader policy passed'
