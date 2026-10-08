[CmdletBinding()]
param([string]$Root,[string]$StateRoot)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'Package.psm1') -Force -DisableNameChecking
$d=Get-Defaults;if(!$Root){$Root=$d.Root}
$Root=Resolve-InstallPath $Root
$ledger=Read-Install $Root
if(!$ledger){throw 'No owned installation manifest; nothing removed'}
if(!$StateRoot){$StateRoot=$ledger.stateRoot}
$StateRoot=Resolve-InstallPath $StateRoot
if(!(Resolve-InstallPath $ledger.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Maintenance state must match the recorded installation profile'}
$stage=New-GuardStage $Root $ledger
$guard=$null
try {
    $guard=Acquire-InstallationGuard $stage.Root $Root $StateRoot
    $ledger=Read-Install $Root
    if(!$ledger){throw 'Installation changed while acquiring guard'}
    if(!(Resolve-InstallPath $ledger.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Installation state profile changed while acquiring guard'}
    if($ledger.shortcut -and (Test-Path -LiteralPath $ledger.shortcut.path)){
        $null=Resolve-InstallPath $ledger.shortcut.path
        if((Get-FileHash -LiteralPath $ledger.shortcut.path).Hash -eq $ledger.shortcut.sha256){Remove-Item -LiteralPath $ledger.shortcut.path}else{Write-Warning 'Preserved modified shortcut'}
    }
    if($ledger.startup){Remove-MatchingRun $ledger.startup.key $ledger.startup.command}
    Remove-Owned $Root $ledger.files
    Remove-Owned $Root $ledger.history
    Remove-Item -LiteralPath (Join-OwnedPath $Root 'install-manifest.json')
    "Uninstalled matching owned files at $Root; modified/unrelated files and state preserved"
} finally {if($guard){$guard.Dispose()};Remove-Owned $stage.Root $stage.Files}
