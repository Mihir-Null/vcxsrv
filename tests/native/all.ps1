param([Parameter(Mandatory)][string]$XcbBuildRoot)
$ErrorActionPreference = 'Stop'
$tests = @(
    @{ Script = 'run.ps1'; Args = @() },
    @{ Script = 'run.ps1'; Args = @('-Test', 'winpoll') },
    @{ Script = 'xtrans.ps1'; Args = @() },
    @{ Script = 'winxcb.ps1'; Args = @() },
    @{ Script = 'winxcb-live.ps1'; Args = @('-XcbBuildRoot', $XcbBuildRoot) },
    @{ Script = 'winadmission.ps1'; Args = @() },
    @{ Script = 'discovery.ps1'; Args = @() },
    @{ Script = 'managed-profile.ps1'; Args = @() },
    @{ Script = 'managed-auth.ps1'; Args = @('-BuildRoot', $XcbBuildRoot) },
    @{ Script = 'compile-units.ps1'; Args = @('-BuildRoot', $XcbBuildRoot) }
)
foreach ($test in $tests) {
    $script = Join-Path $PSScriptRoot $test.Script
    $testArgs = $test.Args
    & (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File $script @testArgs
    if ($LASTEXITCODE -ne 0) { throw "$($test.Script) failed with exit code $LASTEXITCODE" }
}
Write-Host 'PASS all native socket fixtures and affected production units'
