[CmdletBinding(DefaultParameterSetName='Install')]
param([Parameter(Mandatory,ParameterSetName='Install')][string]$Bundle,[string]$Root,[string]$StateRoot,[switch]$EnableStartup,[string]$ShortcutPath,[string]$StartupKey,[Parameter(Mandatory,ParameterSetName='Rollback')][switch]$Rollback)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSScriptRoot 'Package.psm1') -Force -DisableNameChecking
$d=Get-Defaults
if(!$Root){$Root=$d.Root};if(!$ShortcutPath){$ShortcutPath=$d.ShortcutPath};if(!$StartupKey){$StartupKey=$d.StartupKey}
$Root=Resolve-InstallPath $Root
$old=Read-Install $Root
if(!$StateRoot){if($old){$StateRoot=$old.stateRoot}else{$StateRoot=$d.StateRoot}}
$StateRoot=Resolve-InstallPath $StateRoot
if($old -and !(Resolve-InstallPath $old.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Maintenance state must match the recorded installation profile'}
if($Root.Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase) -or $StateRoot.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase) -or $Root.StartsWith($StateRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Install and state paths must be separate'}
if($Rollback){
    if(!$old -or !$old.previous){throw 'No recorded rollback version'}
    $Bundle=Join-OwnedPath $Root $old.previous.path
    $prior=Get-Content -LiteralPath (Join-OwnedPath $Bundle 'install-manifest.json') -Raw|ConvertFrom-Json
    Assert-Entries $Bundle $prior.files -RequireFiles
    $incoming=$prior.files
    $version=$prior.version
    $guardStage=New-GuardStage $Root $old
} else {
    $Bundle=Resolve-InstallPath $Bundle
    if($Bundle.Equals($Root,[StringComparison]::OrdinalIgnoreCase) -or $Bundle.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Bundle must be outside installation root'}
    $manifest=Read-Bundle $Bundle
    $incoming=@($manifest.files)+@(Get-OwnedEntry $Bundle 'bundle-manifest.json')
    $version=$manifest.version
    $guardStage=New-GuardStage $Bundle $manifest
}
$guard=$null;$backup=$null;$copied=@();$staged=@();$removed=@();$oldLedgerBytes=$null;$oldLedgerHash=$null;$shortcutBefore=$null;$shortcut=$null;$runBefore=$null
try {
    $guard=Acquire-InstallationGuard $guardStage.Root $Root $StateRoot
    # Reinspect under the retained singleton; no optimistic preflight authorizes mutation.
    $current=Read-Install $Root
    if(($null -eq $old) -ne ($null -eq $current)){throw 'Installation changed while acquiring guard'}
    if($current -and !(Resolve-InstallPath $current.stateRoot).Equals($StateRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'Installation state profile changed while acquiring guard'}
    $old=$current
    if($old){Assert-Entries $Root $old.files -RequireFiles; $oldLedgerBytes=[IO.File]::ReadAllBytes((Join-OwnedPath $Root 'install-manifest.json'));$oldLedgerHash=(Get-FileHash -LiteralPath (Join-OwnedPath $Root 'install-manifest.json')).Hash}
    foreach($e in $incoming){$dst=Join-OwnedPath $Root $e.path;if((Test-Path -LiteralPath $dst) -and (!$old -or $e.path -notin $old.files.path)){throw "Unowned destination collision: $($e.path)"}}
    $target=Join-Path $Root 'vcxsrv-wsl-manager.exe'
    $args='--tray --state-dir "'+$StateRoot+'" --config "'+(Join-Path $StateRoot 'config.json')+'"'
    Assert-Shortcut $ShortcutPath $target $args $Root
    $command=Get-StartupCommand $Root $StateRoot
    $runBefore=Get-RunValue $StartupKey
    if($EnableStartup -and $runBefore -and $runBefore -ne $command){throw 'Startup belongs to another installation'}
    if(Test-Path -LiteralPath $ShortcutPath){$shortcutBefore=[IO.File]::ReadAllBytes($ShortcutPath)}
    $null=New-Item -ItemType Directory -Path $Root -Force
    $history=@();$previous=$null
    if($old){
        $backupRelative='.rollback/'+[guid]::NewGuid();$backup=Join-OwnedPath $Root $backupRelative
        Copy-Owned $Root $backup $old.files
        [IO.File]::WriteAllBytes((Join-OwnedPath $backup 'install-manifest.json'),$oldLedgerBytes)
        $backupFiles=@($old.files)+@(Get-OwnedEntry $backup 'install-manifest.json')
        $history=@($old.history)+@($backupFiles|ForEach-Object{[pscustomobject]@{path=$backupRelative+'/'+$_.path;sha256=$_.sha256;bytes=$_.bytes}})
        $previous=@{path=$backupRelative;files=@($history|Where-Object path -Like ($backupRelative+'/*'))}
    }
    try {
        foreach($e in $incoming){
            $dst=Join-OwnedPath $Root $e.path
            $null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dst)) -Force
            $temp=$dst+'.incoming-'+[guid]::NewGuid()
            Copy-Item -LiteralPath (Join-OwnedPath $Bundle $e.path) -Destination $temp
            $staged+=,[pscustomobject]@{path=[IO.Path]::GetRelativePath($Root,$temp);bytes=$e.bytes;sha256=$e.sha256}
            if((Get-FileHash -LiteralPath $temp).Hash -ne $e.sha256){Remove-Item -LiteralPath $temp;throw 'Staged copy checksum mismatch'}
            # Windows refuses writes/deletes of in-use images; never force a live process.
            [IO.File]::Move($temp,$dst,$true)
            $copied+=,$e
        }
        Assert-Entries $Root $incoming -RequireFiles
        if($old){$removed=@($old.files|Where-Object path -NotIn $incoming.path);Remove-Owned $Root $removed}
        $shortcut=Save-Shortcut $ShortcutPath $Root $StateRoot
        if($EnableStartup -and !$runBefore){Set-RunValue $StartupKey $command}
        $startup=$null
        if((Get-RunValue $StartupKey) -eq $command){$startup=@{key=$StartupKey;command=$command}}
        Save-Ledger $Root @{schemaVersion=1;product='VcXsrvWSL';root=$Root;version=$version;files=$incoming;history=$history;previous=$previous;shortcut=$shortcut;startup=$startup;stateRoot=$StateRoot}
    } catch {
        $failure=$_
        foreach($e in $copied){try{Remove-Owned $Root @($e)}catch{Write-Warning "Payload cleanup failed: $($_.Exception.Message)"}}
        if($old){
            $changed=@($copied|ForEach-Object path)+@($removed|ForEach-Object path)
            foreach($e in @($old.files|Where-Object path -In $changed)){
                try{
                    $restore=Join-OwnedPath $Root $e.path
                    if(!(Test-Path -LiteralPath $restore)){Copy-Owned $backup $Root @($e)}
                    elseif(!(Test-OwnedEntry $Root $e)){Write-Warning "Preserved changed file during failed-install recovery: $($e.path)"}
                }catch{Write-Warning "Payload recovery failed: $($_.Exception.Message)"}
            }
            try{
                $ledgerPath=Join-OwnedPath $Root 'install-manifest.json'
                if(!(Test-Path -LiteralPath $ledgerPath)){[IO.File]::WriteAllBytes($ledgerPath,$oldLedgerBytes)}
                elseif((Get-FileHash -LiteralPath $ledgerPath).Hash -ne $oldLedgerHash){Write-Warning 'Preserved changed installation ledger during recovery'}
            }catch{Write-Warning "Ledger recovery failed: $($_.Exception.Message)"}
        }
        try{
            if($shortcut -and (Test-Path -LiteralPath $ShortcutPath) -and (Get-FileHash -LiteralPath $ShortcutPath).Hash -eq $shortcut.sha256){
                if($shortcutBefore){[IO.File]::WriteAllBytes($ShortcutPath,$shortcutBefore)}else{Remove-Item -LiteralPath $ShortcutPath}
            }
            if(!$runBefore -and (Get-RunValue $StartupKey) -eq $command){Remove-MatchingRun $StartupKey $command}
        }catch{Write-Warning "Shortcut/startup recovery failed: $($_.Exception.Message)"}
        throw $failure
    } finally {try{Remove-Owned $Root $staged}catch{Write-Warning "Staged cleanup failed: $($_.Exception.Message)"}}
    "Installed unsigned development version $version at $Root; state preserved"
} finally {
    if($guard){$guard.Dispose()}
    Remove-Owned $guardStage.Root $guardStage.Files
}
