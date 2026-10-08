[CmdletBinding()]
param([string]$Root,[string]$StateRoot)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'Package.psm1') -DisableNameChecking
$d=Get-Defaults;if(!$Root){$Root=$d.Root}
$Root=Resolve-InstallPath $Root
$ledger=Read-Install $Root
if(!$ledger){throw 'No owned installation manifest; nothing removed'}
if(!$StateRoot){$StateRoot=$ledger.stateRoot}
$StateRoot=Resolve-InstallPath $StateRoot
if(!(Resolve-InstallPath $ledger.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Maintenance state must match the recorded installation profile'}
$stage=New-GuardStage $Root $ledger
$guard=$null;$moved=@();$quarantine=$null;$receipt=$null;$shortcutMoved=$null;$startupRemoved=$false
try {
    $guard=Acquire-InstallationGuard $stage.Root $Root $StateRoot
    $ledger=Read-Install $Root
    if(!$ledger){throw 'Installation changed while acquiring guard'}
    if(!(Resolve-InstallPath $ledger.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Installation state profile changed while acquiring guard'}
    $ledgerPath=Join-OwnedPath $Root 'install-manifest.json'
    $ledgerHash=(Get-FileHash -LiteralPath $ledgerPath).Hash;$ledgerIdentity=[VcXsrvPackagingHandles]::Identity($ledgerPath)
    $quarantine=Join-OwnedPath $Root ('.uninstall/'+[guid]::NewGuid())
    $null=New-Item -ItemType Directory -Path $quarantine
    # A retained receipt preserves the recovery route if storage restoration itself fails.
    Save-Ledger $quarantine @{schemaVersion=1;product='VcXsrvWSL-uninstall-quarantine';root=$Root;files=@($ledger.files)+@($ledger.history);shortcut=$ledger.shortcut;startup=$ledger.startup} -RequireAbsent
    $receipt=Get-OwnedEntry $quarantine 'install-manifest.json'
    try{
        foreach($e in @($ledger.files)+@($ledger.history)){
            $source=Join-OwnedPath $Root $e.path
            if(!(Test-Path -LiteralPath $source)){continue}
            if(!(Test-OwnedEntry $Root $e)){Write-Warning "Preserved modified owned file: $($e.path)";continue}
            $destination=Join-OwnedPath $quarantine $e.path
            $null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($destination)) -Force
            $source=Join-OwnedPath $Root $e.path;$destination=Join-OwnedPath $quarantine $e.path
            if(!(Test-OwnedEntry $Root $e)){throw 'Owned file changed before uninstall move'}
            [IO.File]::Move($source,$destination,$false);$moved+=,$e
        }
        if($ledger.shortcut -and (Test-Path -LiteralPath $ledger.shortcut.path)){
            $null=Resolve-InstallPath $ledger.shortcut.path
            if((Get-FileHash -LiteralPath $ledger.shortcut.path).Hash -eq $ledger.shortcut.sha256){
                $shortcutMoved=Resolve-InstallPath ($ledger.shortcut.path+'.uninstall-'+[guid]::NewGuid())
                [IO.File]::Move($ledger.shortcut.path,$shortcutMoved,$false)
            }else{Write-Warning 'Preserved modified shortcut'}
        }
        if($ledger.startup -and (Get-RunValue $ledger.startup.key) -eq $ledger.startup.command){
            Remove-MatchingRun $ledger.startup.key $ledger.startup.command
            $startupRemoved=!(Get-RunValue $ledger.startup.key)
        }elseif($ledger.startup){Write-Warning 'Preserved startup entry changed by user/another installation'}
        $ledgerPath=Join-OwnedPath $Root 'install-manifest.json'
        if([VcXsrvPackagingHandles]::Identity($ledgerPath) -ne $ledgerIdentity -or (Get-FileHash -LiteralPath $ledgerPath).Hash -ne $ledgerHash){throw 'Ledger changed before uninstall commit'}
        Remove-Item -LiteralPath $ledgerPath
    }catch{
        $failure=$_;$restored=$true
        foreach($e in $moved){
            try{
                $destination=Join-OwnedPath $Root $e.path;$source=Join-OwnedPath $quarantine $e.path
                if(!(Test-Path -LiteralPath $destination)){[IO.File]::Move($source,$destination,$false)}
                elseif(!(Test-OwnedEntry $Root $e)){$restored=$false;Write-Warning "Preserved changed recovery destination: $($e.path)"}
            }catch{$restored=$false;Write-Warning "Uninstall recovery failed: $($_.Exception.Message)"}
        }
        try{
            if($shortcutMoved -and (Test-Path -LiteralPath $shortcutMoved)){
                if(!(Test-Path -LiteralPath $ledger.shortcut.path)){[IO.File]::Move($shortcutMoved,$ledger.shortcut.path,$false)}else{$restored=$false}
            }
            if($startupRemoved -and !(Get-RunValue $ledger.startup.key)){Set-RunValue $ledger.startup.key $ledger.startup.command}
        }catch{$restored=$false;Write-Warning "Entry recovery failed: $($_.Exception.Message)"}
        if($restored){try{Remove-Owned $quarantine $moved;Remove-Owned $quarantine @($receipt)}catch{Write-Warning 'Recovery receipt cleanup failed'}}
        else{Write-Warning "Recovery payload/receipt preserved at $quarantine; inspect before maintenance retry"}
        throw $failure
    }
    # Logical uninstall committed. Cleanup failures preserve the receipt and exact owned files.
    try{
        Remove-Owned $quarantine $moved
        if($shortcutMoved){Remove-Owned ([IO.Path]::GetDirectoryName($shortcutMoved)) @([pscustomobject]@{path=[IO.Path]::GetFileName($shortcutMoved);bytes=(Get-Item -LiteralPath $shortcutMoved).Length;sha256=$ledger.shortcut.sha256})}
        Remove-Owned $quarantine @($receipt)
    }catch{Write-Warning "Uninstall complete; matching cleanup remains at $quarantine : $($_.Exception.Message)"}
    "Uninstalled matching owned files at $Root; modified/unrelated files and state preserved"
} finally {if($guard){$guard.Dispose()};Remove-Owned $stage.Root $stage.Files}
