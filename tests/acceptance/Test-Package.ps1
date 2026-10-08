param([Parameter(Mandatory)][string]$Bundle)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
if(!(Test-Path -LiteralPath (Join-Path $repo 'packaging/install.ps1'))){throw 'Required safe installer is missing'}
Import-Module (Join-Path $repo 'packaging/Package.psm1') -Force -DisableNameChecking
$base=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv package acceptance '+[guid]::NewGuid())
$root=Join-Path $base 'installation with spaces'
$state=Join-Path $base 'private state'
$shortcut=Join-Path $base 'shortcuts/Manager.lnk'
$run='Software\VcXsrvWSL\Acceptance\'+[guid]::NewGuid()
$assert={param($condition,$message) if(!$condition){throw $message}}
& (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -StateRoot $state -ShortcutPath $shortcut -StartupKey $run
& $assert (!(Test-Path -LiteralPath $state)) 'Install must not create private state'
& $assert (Test-Path -LiteralPath (Join-Path $root 'vcxsrv.exe')) 'Server missing'
[IO.File]::WriteAllText((Join-Path $root 'unrelated.txt'),'keep')
$before=(Get-FileHash -LiteralPath (Join-Path $root 'vcxsrv-wsl-manager.exe')).Hash
& (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -StateRoot $state -ShortcutPath $shortcut -StartupKey $run
& $assert ((Get-FileHash -LiteralPath (Join-Path $root 'vcxsrv-wsl-manager.exe')).Hash -eq $before) 'Repeat changed release unexpectedly'
& (Join-Path $repo 'packaging/install.ps1') -Root $root -StateRoot $state -Rollback -ShortcutPath $shortcut -StartupKey $run
& $assert ((Get-FileHash -LiteralPath (Join-Path $root 'vcxsrv-wsl-manager.exe')).Hash -eq $before) 'Rollback did not restore old payload'
[IO.File]::WriteAllText((Join-Path $root 'protocol.txt'),'user changed owned file')
& (Join-Path $repo 'packaging/uninstall.ps1') -Root $root -StateRoot $state
& $assert ((Get-Content -LiteralPath (Join-Path $root 'protocol.txt') -Raw) -eq 'user changed owned file') 'Modified owned file removed'
& $assert ((Get-Content -LiteralPath (Join-Path $root 'unrelated.txt') -Raw) -eq 'keep') 'Unrelated file removed'
& $assert (!(Test-Path -LiteralPath (Join-Path $root 'vcxsrv.exe'))) 'Matching owned payload not removed'
"PASS isolated repeat/rollback/uninstall preservation; retained fixture $base"
