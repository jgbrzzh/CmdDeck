param([switch]$InstallPrerequisites)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
if ($InstallPrerequisites) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) { throw '请先从 Microsoft Store 安装 App Installer，再运行安装脚本。' }
    foreach ($id in @('OpenJS.NodeJS.LTS','Rustlang.Rustup','Microsoft.EdgeWebView2Runtime')) {
        & winget install --id $id --exact --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -notin @(0,-1978335189)) { Write-Warning "请检查 $id 的安装结果。" }
    }
    & winget install --id Microsoft.VisualStudio.2022.BuildTools --exact --accept-package-agreements --accept-source-agreements --override '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    Write-Host '安装结束。请重新打开 PowerShell，再运行 Setup.ps1。' -ForegroundColor Green
    exit
}
foreach ($command in @('node','npm','cargo','rustc')) { if (-not (Get-Command $command -ErrorAction SilentlyContinue)) { throw "缺少 $command。请运行：powershell -ExecutionPolicy Bypass -File .\scripts\Setup.ps1 -InstallPrerequisites" } }
& npm ci
if ($LASTEXITCODE -ne 0) { throw 'npm 依赖安装失败，请检查网络连接。' }
& npm run build
if ($LASTEXITCODE -ne 0) { throw '前端构建失败，请查看上方错误。' }
& cargo check --locked --manifest-path src-tauri/Cargo.toml -j 2
if ($LASTEXITCODE -ne 0) { throw 'Rust 检查失败，请确认 C++ Build Tools 和 Windows SDK 已安装。' }
Write-Host '安装和检查完成。运行 scripts\Start.ps1 即可启动。' -ForegroundColor Green
