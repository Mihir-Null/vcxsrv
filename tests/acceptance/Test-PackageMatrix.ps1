param([string]$Bundle=(Join-Path ([IO.Path]::GetDirectoryName($PSScriptRoot)) '../../../../../vcxsrv-build-tools/task8-small-fixture'))
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Import-Module (Join-Path $repo 'packaging/Package.psm1') -Force -DisableNameChecking
$base=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv matrix '+[guid]::NewGuid())
$root=Join-Path $base 'installation';$state=Join-Path $base 'state';$shortcut=Join-Path $base 'shortcut/Manager.lnk';$key='Software\VcXsrvWSL\Acceptance\'+[guid]::NewGuid()
$Bundle=[IO.Path]::GetFullPath($Bundle)
function Assert($value,$message){if(!$value){throw $message}}
function Install { & (Join-Path $repo 'packaging/install.ps1') -Bundle $Bundle -Root $root -StateRoot $state -ShortcutPath $shortcut -StartupKey $key @args }
function Refuses([scriptblock]$Action){$failed=$false;try{& $Action}catch{$failed=$true};Assert $failed 'Unsafe operation was accepted'}
Install
Assert (!(Get-RunValue $key)) 'Default installation enabled startup'
$outside=Join-Path $base 'outside';$null=New-Item -ItemType Directory -Path $outside
$null=New-Item -ItemType Junction -Path (Join-Path $base 'junction') -Target $outside
Refuses {$null=Join-OwnedPath $base 'junction/escape.txt'}
# Create exact private fixture ACLs, never repair or touch real user state.
$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
$acl=[Security.AccessControl.DirectorySecurity]::new();$acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false)
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))
[IO.FileSystemAclExtensions]::CreateDirectory($acl,$state)|Out-Null
$config=Join-Path $state 'config.json';[IO.File]::WriteAllText($config,(@{version=1;server_path=(Join-Path $root 'vcxsrv.exe')}|ConvertTo-Json))
$facl=[Security.AccessControl.FileSecurity]::new();$facl.SetOwner($sid);$facl.SetAccessRuleProtection($true,$false);$facl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow));[IO.FileSystemAclExtensions]::SetAccessControl([IO.FileInfo]::new($config),$facl)
$configHash=(Get-FileHash -LiteralPath $config).Hash
Install -EnableStartup
Assert ((Get-FileHash -LiteralPath $config).Hash -eq $configHash) 'Configuration changed during reinstall'
Assert ([bool](Get-RunValue $key)) 'Explicit startup was not enabled'
$ledgerHash=(Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash
$manifest=Get-Content -LiteralPath (Join-Path $Bundle 'bundle-manifest.json') -Raw
try {
 foreach($bad in @('../outside.txt','C:/outside.txt','bad:stream','a/../escape','CON.txt','bad.','/absolute')) {
  $m=$manifest|ConvertFrom-Json;$m.files[0].path=$bad;$m|ConvertTo-Json -Depth 15|Set-Content -LiteralPath (Join-Path $Bundle 'bundle-manifest.json')
  Refuses {Install}
  Assert ((Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash -eq $ledgerHash) 'Unsafe manifest changed installation'
 }
 [IO.File]::WriteAllText((Join-Path $Bundle 'bundle-manifest.json'),$manifest)
 $artifact=Join-Path $Bundle 'protocol.txt';$bytes=[IO.File]::ReadAllBytes($artifact)
 [IO.File]::WriteAllText($artifact,'tampered');Refuses {Install};[IO.File]::WriteAllBytes($artifact,$bytes)
 $missing=Join-Path $Bundle 'vcxsrv.exe';[IO.File]::Move($missing,$missing+'.saved');try{Refuses {Install}}finally{[IO.File]::Move($missing+'.saved',$missing)}
 $locked=[IO.File]::Open((Join-Path $root 'protocol.txt'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{Refuses {Install}}finally{$locked.Dispose()}
 Assert (@(Get-ChildItem -LiteralPath $root -File -Recurse -Filter '*.incoming-*').Count -eq 0) 'Failed transaction left unowned incoming files'
 Assert ((Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash -eq $ledgerHash) 'Failed transaction changed ledger'
 $m=Read-Install $root;Assert-Entries $root $m.files -RequireFiles
 # A late ledger failure must restore a file removed as obsolete by the new version.
 $next=$manifest|ConvertFrom-Json;$next.files=@($next.files|Where-Object path -NE 'protocol.txt')
 $next|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $Bundle 'bundle-manifest.json')
 $locked=[IO.File]::Open((Join-Path $root 'install-manifest.json'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try{Refuses {Install}}finally{$locked.Dispose();[IO.File]::WriteAllText((Join-Path $Bundle 'bundle-manifest.json'),$manifest)}
 Assert-Entries $root $m.files -RequireFiles
 Assert (@(Get-ChildItem -LiteralPath $root -File -Filter 'ledger-*.tmp').Count -eq 0) 'Failed ledger left unowned temporary file'
 $fixture=Join-Path $root 'unrelated-live-fixture.exe';Copy-Item -LiteralPath (Join-Path $repo 'manager/target/debug/process-fixture.exe') -Destination $fixture
 $p=[Diagnostics.Process]::Start($fixture,'sleep')
 try{Refuses {Install};Assert (!$p.HasExited) 'Maintenance killed unrelated fixture'}finally{$p.Kill();$p.WaitForExit();$p.Dispose();Remove-Item -LiteralPath $fixture}
 [IO.File]::WriteAllText((Join-Path $root 'protocol.txt'),'user edited');Refuses {Install}
 Set-RunValue $key 'user changed startup'
 [IO.File]::WriteAllText($shortcut,'user changed shortcut')
 & (Join-Path $repo 'packaging/uninstall.ps1') -Root $root -StateRoot $state
 Assert ((Get-RunValue $key) -eq 'user changed startup') 'Modified startup removed'
 Assert ((Get-Content -LiteralPath $shortcut -Raw) -eq 'user changed shortcut') 'Modified shortcut removed'
 Assert ((Get-FileHash -LiteralPath $config).Hash -eq $configHash) 'Uninstall changed config/state'
 Assert ((Get-Content -LiteralPath (Join-Path $root 'protocol.txt') -Raw) -eq 'user edited') 'Uninstall removed modified payload'
} finally {
 [IO.File]::WriteAllText((Join-Path $Bundle 'bundle-manifest.json'),$manifest)
 $k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key,$true);if($k){try{$k.DeleteValue('VcXsrvWSL',$false)}finally{$k.Dispose()}}
 [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($key,$false)
}
"PASS path grammar/missing/tampered/pre-replacement rejection/private-config-preservation/startup-off+opt-in+modifiedentry-preservation/live-image-refusal/locked-transaction-rollback; retained $base"

