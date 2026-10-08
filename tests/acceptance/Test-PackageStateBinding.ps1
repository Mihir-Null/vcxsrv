param([Parameter(Mandatory)][string]$Bundle)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$base=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv state binding '+[guid]::NewGuid())
$root=Join-Path $base 'install';$state=Join-Path $base 'state';$foreign=Join-Path $base 'foreign state'
$shortcut=Join-Path $base 'shortcut.lnk';$key='Software\VcXsrvWSL\Acceptance\'+[guid]::NewGuid()
function Assert($condition,$message){if(!$condition){throw $message}}
function Refuses([scriptblock]$action){$failed=$false;try{& $action}catch{$failed=$true};Assert $failed 'Maintenance accepted a different state profile'}
& (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -StateRoot $state -ShortcutPath $shortcut -StartupKey $key
$ledger=(Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash
Refuses {& (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -StateRoot $foreign -ShortcutPath (Join-Path $base 'other.lnk') -StartupKey $key}
Assert ((Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash -eq $ledger) 'Foreign state replacement changed ledger'
Refuses {& (Join-Path $repo 'packaging/uninstall.ps1') -Root $root -StateRoot $foreign}
Assert ((Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash -eq $ledger) 'Foreign state uninstall changed ledger'
# An omitted profile must inherit this installation's recorded custom state path.
& (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -ShortcutPath $shortcut -StartupKey $key
& (Join-Path $repo 'packaging/uninstall.ps1') -Root $root
Assert (!(Test-Path -LiteralPath (Join-Path $root 'install-manifest.json'))) 'Bound maintenance did not complete'
"PASS recorded-state binding/refusal and omitted-profile inheritance; retained $base"
