$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if (-not (Test-Path -LiteralPath 'node_modules')) {
    & npm ci
    if ($LASTEXITCODE -ne 0) { throw '依赖安装失败。' }
}
$env:CARGO_BUILD_JOBS = '2'
& npm run tauri:dev
if ($LASTEXITCODE -ne 0) { throw '启动失败，请查看上方错误。' }
