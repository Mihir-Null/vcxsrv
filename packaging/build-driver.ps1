# Checked native x64 Release recipe. Run only on a new build snapshot.
[CmdletBinding()]
param([Parameter(Mandatory)][string]$BuildRoot, [Parameter(Mandatory)][hashtable]$Tools, [int]$Jobs = 1, [string]$BuildDistro = 'NixOS', [string]$BuildUser = 'Empty')
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildServer.psm1') -Force
$originalEnvironment = @{}; Get-ChildItem Env: | ForEach-Object { $originalEnvironment[$_.Name] = $_.Value }
$originalLocation = Get-Location
try {
    foreach ($path in $Tools.Values) {
        if ($path -and $path -match '["&|<>^%!\r\n]') { throw "Tool path cannot contain cmd metacharacters: $path" }
    }
    $vcvars = $Tools.vcvarsall
    $environment = & $env:ComSpec /d /c "call `"$vcvars`" x64 >nul && set"
    if ($LASTEXITCODE -ne 0) {
        $failure = [Exception]::new("vcvarsall failed to initialize the x64 build environment ($LASTEXITCODE).")
        $failure.Data['ExitCode'] = $LASTEXITCODE
        throw $failure
    }
    foreach ($line in $environment) {
        $separator = $line.IndexOf('=')
        if ($separator -gt 0) { [Environment]::SetEnvironmentVariable($line.Substring(0,$separator), $line.Substring($separator+1), 'Process') }
    }
    Set-BuildToolEnvironment $BuildRoot $Tools
    Update-BuildSnapshot $BuildRoot $Tools
    Set-Location -LiteralPath $BuildRoot
    Invoke-CheckedTool $Tools.msbuild @('freetype/MSBuild.sln','-t:Rebuild','-p:Configuration=Release','-p:Platform=x64',"-m:$Jobs")
    New-Item -ItemType Directory -Path 'openssl/release64' -Force | Out-Null
    Set-Location -LiteralPath (Join-Path $BuildRoot 'openssl/release64')
    Invoke-CheckedTool $Tools.perl @('../Configure','VC-WIN64A','--release')
    Invoke-CheckedTool $Tools.jom @("/J$Jobs")
    Set-Location -LiteralPath (Join-Path $BuildRoot 'pthreads')
    Invoke-CheckedTool $Tools.nmake @('VC-static')
    Set-Location -LiteralPath $BuildRoot
    Invoke-CheckedTool $Tools.msbuild @('tools/mhmake/mhmakevc10.sln','-t:Rebuild','-p:Configuration=Release','-p:Platform=x64',"-m:$Jobs")
    $keyboard = Initialize-KeyboardAssets $BuildRoot $BuildDistro $BuildUser
    Invoke-CheckedTool (Join-Path $BuildRoot 'tools/mhmake/Release64/mhmake.exe') @("-P$Jobs",'-C','xorg-server','MAKESERVER=1','DEBUG=0')
    return $keyboard
} finally {
    Set-Location $originalLocation
    Get-ChildItem Env: | Where-Object { -not $originalEnvironment.ContainsKey($_.Name) } | ForEach-Object { Remove-Item -LiteralPath ('Env:' + $_.Name) }
    foreach ($name in $originalEnvironment.Keys) { [Environment]::SetEnvironmentVariable($name,$originalEnvironment[$name],'Process') }
}
