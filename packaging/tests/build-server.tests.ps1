$ErrorActionPreference = 'Stop'
$root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$wrapper = Join-Path $root 'packaging/build-server.ps1'
$module = Join-Path $root 'packaging/BuildServer.psm1'
$work = Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv-build-tests-' + [guid]::NewGuid())
New-Item -ItemType Directory $work | Out-Null
$failures = 0
function Check($name, [scriptblock]$test) {
    try { & $test; Write-Host "PASS $name" } catch { $script:failures++; Write-Host "FAIL $name : $_" }
}
function Assert($condition, $message) { if (-not $condition) { throw $message } }
Check 'missing explicit tool fails preflight without creating output' {
    Assert (Test-Path $wrapper) 'build preflight wrapper is not implemented'
    $output = Join-Path $work 'preflight-output'
    $overrides = Join-Path $work 'tools.json'
    @{ nasm = (Join-Path $work 'nonexistent-nasm.exe') } | ConvertTo-Json | Set-Content $overrides
    $result = & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $wrapper -SourceRoot $root -OutputRoot $output -Mode Preflight -ToolPathsFile $overrides 2>$null
    Assert ($LASTEXITCODE -eq 2) 'missing tool did not return preflight exit 2'
    $inventory = ($result -join "`n") | ConvertFrom-Json
    $nasm = $inventory.tools | Where-Object name -eq 'nasm'
    Assert ($nasm.status -eq 'missing') 'explicit missing tool silently fell back to PATH'
    Assert (-not (Test-Path $output)) 'preflight emitted artifacts'
}
Check 'native command failure retains the exact nonzero exit code' {
    Assert (Test-Path $module) 'checked command helper is not implemented'
    Import-Module $module -Force
    $caught = $null
    try { Invoke-CheckedTool (Join-Path $PSHOME 'pwsh.exe') @('-NoProfile', '-Command', 'exit 23') } catch { $caught = $_ }
    Assert ($null -ne $caught) 'failing child was accepted'
    Assert ($caught.Exception.Data['ExitCode'] -eq 23) 'exit code 23 was lost'
}
Check 'stale artifacts never become a staged success' {
    Assert (Test-Path $module) 'artifact staging helper is not implemented'
    Import-Module $module -Force
    $source = Join-Path $work 'stale-source'; New-Item -ItemType Directory $source | Out-Null
    Set-Content (Join-Path $source 'vcxsrv.exe') 'old'
    (Get-Item (Join-Path $source 'vcxsrv.exe')).LastWriteTimeUtc = [datetime]::UtcNow.AddDays(-1)
    $output = Join-Path $work 'stale-output'
    $caught = $false
    try { Publish-BuildArtifacts $source $output @('vcxsrv.exe') ([datetime]::UtcNow) @{} } catch { $caught = $true }
    Assert $caught 'old artifact was accepted'
    Assert (-not (Test-Path $output)) 'rejected artifact created staging output'
}
Check 'successful staging hashes fresh artifacts and records manifest provenance' {
    Assert (Test-Path $module) 'artifact staging helper is not implemented'
    Import-Module $module -Force
    $source = Join-Path $work 'fresh-source'; New-Item -ItemType Directory $source | Out-Null
    $start = [datetime]::UtcNow.AddSeconds(-1)
    [IO.File]::WriteAllText((Join-Path $source 'vcxsrv.exe'), 'abc')
    $output = Join-Path $work 'fresh-output'
    Publish-BuildArtifacts $source $output @('vcxsrv.exe') $start @{sourceCommit='fixture-commit'; dirty=$true; buildCommand=@('fixture')} | Out-Null
    $manifest = Get-Content (Join-Path $output 'build-manifest.json') -Raw | ConvertFrom-Json
    Assert ($manifest.artifacts[0].sha256 -eq 'BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD') 'artifact hash is incorrect'
    Assert ($manifest.sourceCommit -eq 'fixture-commit' -and $manifest.dirty) 'source provenance was lost'
}
Check 'artifact paths cannot escape staging or publish partial output' {
    Assert (Test-Path $module) 'artifact staging helper is not implemented'
    Import-Module $module -Force
    $output = Join-Path $work 'escape-output'
    $caught = $false
    try { Publish-BuildArtifacts $work $output @('../escape.exe') ([datetime]::UtcNow) @{} } catch { $caught = $true }
    Assert $caught 'path traversal accepted'
    Assert (-not (Test-Path $output)) 'invalid path created output'
}
Check 'build driver stops immediately when the compiler command fails' {
    $driver = Join-Path $root 'packaging/build-driver.ps1'
    Assert (Test-Path $driver) 'checked build driver is not implemented'
    $build = Join-Path $work 'driver-source'
    New-Item -ItemType Directory -Path (Join-Path $build 'tools/mhmake'), (Join-Path $build 'xorg-server/hw/xwin') -Force | Out-Null
    foreach ($file in @('tools/mhmake/makelex.bat','tools/mhmake/makebison.bat','xorg-server/hw/xwin/doflexbison.bat')) { Set-Content (Join-Path $build $file) '@echo off' }
    $compiler = Join-Path $work 'compiler.cmd'; Set-Content $compiler '@exit /b 19'
    $vcvars = Join-Path $work 'vcvars.bat'; Set-Content $vcvars '@exit /b 0'
    $tools = @{vcvarsall=$vcvars; msbuild=$compiler; python=$compiler; win_bison=$compiler; win_flex=$compiler; bash=$compiler}
    $previousLocation = (Get-Location).Path
    $previousMhmakeConf = $env:MHMAKECONF
    $caught = $null
    try { & $driver -BuildRoot $build -Tools $tools -Jobs 1 } catch { $caught = $_ }
    Assert ($null -ne $caught) 'compiler failure was accepted'
    Assert ((Get-Location).Path -eq $previousLocation -and $env:MHMAKECONF -eq $previousMhmakeConf) "driver leaked its environment or location: previous=$previousLocation current=$((Get-Location).Path) previousMH=$previousMhmakeConf currentMH=$env:MHMAKECONF"
    Assert ($caught.Exception.Data['ExitCode'] -eq 19) 'driver lost compiler exit code 19'
    Assert (-not (Test-Path (Join-Path $build 'openssl/release64'))) 'driver continued to OpenSSL after compiler failure'
}
Write-Host "Tests: 6; failures: $failures"
# Temporary fixtures stay under the OS temporary directory; never delete repository files.
$resolvedWork = [IO.Path]::GetFullPath($work)
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
if (-not $resolvedWork.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup target' }
Remove-Item -LiteralPath $resolvedWork -Recurse -Force
if ($failures) { exit 1 }
