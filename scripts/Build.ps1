param([ValidateSet('all','exe','msi')][string]$Target = 'all')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not (Test-Path -LiteralPath 'node_modules')) { & npm ci; if ($LASTEXITCODE -ne 0) { throw '依赖安装失败。' } }
$env:CARGO_BUILD_JOBS = '2'
$bundles = switch ($Target) { 'exe' { 'nsis' } 'msi' { 'msi' } default { 'nsis,msi' } }
& npm run tauri:build -- --bundles $bundles
if ($LASTEXITCODE -ne 0) { throw '打包失败，请查看错误日志。' }
Write-Host '安装包已生成：' -ForegroundColor Green
Get-ChildItem -LiteralPath '.\src-tauri\target\release\bundle' -Recurse -File | Where-Object { $_.Extension -in '.msi','.exe' } | Select-Object FullName,Length
