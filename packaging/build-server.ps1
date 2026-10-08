#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$SourceRoot,
    [Parameter(Mandatory)][string]$OutputRoot,
    [ValidateSet('Preflight','Build')][string]$Mode = 'Preflight',
    [string]$ToolPathsFile,
    [ValidateRange(1,256)][int]$Jobs = [Environment]::ProcessorCount
)
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'BuildServer.psm1') -Force
try {
    $source = (Resolve-Path -LiteralPath $SourceRoot).Path
    $output = [IO.Path]::GetFullPath($OutputRoot)
    if (-not (Test-Path -LiteralPath (Join-Path $source 'xorg-server/hw/xwin/winvsock.c'))) { throw 'SourceRoot is not the VSOCK-capable VcXsrv source tree.' }
    $overrides = @{}
    if ($ToolPathsFile) { $overrides = Get-Content -LiteralPath $ToolPathsFile -Raw | ConvertFrom-Json -AsHashtable }
    $inventory = Get-BuildInventory $source $overrides
    if ($Mode -eq 'Preflight') {
        $inventory | ConvertTo-Json -Depth 10
        if (-not $inventory.ready) { exit 2 }
        exit 0
    }
    if (-not $inventory.ready) { throw 'Build prerequisites are missing. Run -Mode Preflight for the full inventory.' }
    if (Test-Path -LiteralPath $output) { throw 'OutputRoot already exists; choose a new staging directory.' }
    $tools = @{}; foreach ($tool in $inventory.tools) { $tools[$tool.name] = $tool.path }
    $sourceCommit = (& $tools.git -C $source rev-parse HEAD).Trim()
    if ($LASTEXITCODE) { throw 'Cannot determine source commit.' }
    $dirty = @(& $tools.git -C $source status --porcelain).Count -ne 0
    if ($LASTEXITCODE) { throw 'Cannot determine dirty state.' }
    # A new tracked-file snapshot excludes ignored binaries and stale object directories.
    $workspace = $output + '.work-' + [guid]::NewGuid()
    $files = @(& $tools.git -C $source -c core.quotepath=false ls-files)
    if ($LASTEXITCODE) { throw 'Cannot enumerate tracked build inputs.' }
    foreach ($relative in $files) {
        $destination = Join-Path $workspace $relative
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $source $relative) -Destination $destination
    }
    $started = [datetime]::UtcNow
    & (Join-Path $PSScriptRoot 'build-driver.ps1') -BuildRoot $workspace -Tools $tools -Jobs $Jobs
    $manifest = @{schemaVersion=1; sourceCommit=$sourceCommit; dirty=$dirty; startedUtc=$started.ToString('o'); sourceRoot=$source; buildWorkspace=$workspace; toolInventory=$inventory; buildCommand=@('build-driver.ps1','-BuildRoot',$workspace,'-Jobs',"$Jobs"); status='built'}
    Publish-BuildArtifacts $workspace $output @('xorg-server/obj64/servrelease/vcxsrv.exe','apps/xauth/obj64/release/xauth.exe','xkbcomp/obj64/release/xkbcomp.exe') $started $manifest
    Write-Host "Build staged at $output (workspace retained at $workspace)."
} catch {
    Write-Error $_ -ErrorAction Continue
    if ($_.Exception.Data.Contains('ExitCode')) { exit [int]$_.Exception.Data['ExitCode'] }
    exit 1
}
