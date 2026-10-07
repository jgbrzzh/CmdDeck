param([string]$Session='cmddeck-repair2',[string]$DataDir='test-results/repair-runtime')
$ErrorActionPreference='Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)
$taskData=[IO.Path]::GetFullPath($DataDir)
$testRoot=[IO.Path]::GetFullPath('test-results')+[IO.Path]::DirectorySeparatorChar
if(-not $taskData.StartsWith($testRoot,[StringComparison]::OrdinalIgnoreCase)){throw '只能使用 test-results 下的隔离实例'}
function Invoke-UIRegression([string]$ScriptPath){
    $taskOutput=@(& npx --yes --package @playwright/cli playwright-cli "-s=$Session" run-code --filename $ScriptPath)
    $taskExit=$LASTEXITCODE
    $taskOutput | Write-Output
    if($taskExit -ne 0 -or ($taskOutput -join "`n") -match '(?m)^### Error'){throw "验收失败：$ScriptPath"}
}
Invoke-UIRegression 'scripts/repair-fixtures.cjs'
$bundle=Get-Content -LiteralPath (Join-Path $taskData 'repair-export.json') -Raw | ConvertFrom-Json -Depth 100
$fixture=@($bundle.presets | Where-Object name -eq '修复验收图标')[-1]
$invalid=($bundle | ConvertTo-Json -Depth 100) | ConvertFrom-Json -Depth 100
$invalid.presets[0].name='x'*101
$invalid | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $taskData 'repair-invalid.json') -Encoding utf8NoBOM
$bundle.groups=@();$bundle.presets=@($fixture);$bundle.workflows=@();$bundle.schedules=@()
$bundle.presets[0].notes='事务导入成功'
$bundle | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $taskData 'repair-valid.json') -Encoding utf8NoBOM
Invoke-UIRegression 'scripts/repair-smoke.cjs'
