param([Parameter(Mandatory)][string]$Bundle,[ValidateSet('first','edit','appearance','identity','uninstall','copy','ledger','replacement')][string]$Case)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Import-Module (Join-Path $repo 'packaging/Package.psm1') -Force -DisableNameChecking
$base=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv package review '+$Case+' '+[guid]::NewGuid())
$root=Join-Path $base 'install';$state=Join-Path $base 'state';$shortcut=Join-Path $base 'Manager.lnk';$key='Software\VcXsrvWSL\Acceptance\'+[guid]::NewGuid()
$input=Read-Bundle $Bundle;$local=Join-Path $base 'bundle'
Copy-Owned $Bundle $local $input.files
# Put protocol after the manager, so a later busy-file error tests actual recovery.
$input.files=@($input.files|Sort-Object {if($_.path -eq 'protocol.txt'){1}else{0}})
$input|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $local 'bundle-manifest.json')
function Assert($value,$message){if(!$value){throw $message}}
function Install {& (Join-Path $repo 'packaging/install.ps1') -Bundle $local -Root $root -StateRoot $state -ShortcutPath $shortcut -StartupKey $key @args}
function Refuses([scriptblock]$action){$failed=$false;try{& $action}catch{$failed=$true};Assert $failed 'Injected failure did not abort maintenance'}
Install -EnableStartup
$ledger=(Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash
$shortcutHash=(Get-FileHash -LiteralPath $shortcut).Hash;$run=Get-RunValue $key
if($Case -eq 'appearance'){
 [IO.File]::WriteAllText((Join-Path $local 'new-note.txt'),'bundle addition')
 $input.files=@($input.files)+@(Get-OwnedEntry $local 'new-note.txt')
 $input|ConvertTo-Json -Depth 30|Set-Content -LiteralPath (Join-Path $local 'bundle-manifest.json')
}
try{
 if($Case -eq 'first'){
  $shell=New-Object -ComObject WScript.Shell
  try{$s=$shell.CreateShortcut($shortcut);Assert (!$s.Arguments.Contains('--config')) 'Fresh shortcut selects absent explicit config'}finally{[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
  Assert (!$run.Contains('--config')) 'Fresh startup selects absent explicit config'
  Assert (!(Test-Path -LiteralPath $state)) 'Installer created state before first launch'
  @{root=$root;state=$state;shortcut=$shortcut;run=$run;key=$key}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $base 'first-launch-fixture.json')
 }elseif($Case -eq 'uninstall'){
  $locked=[IO.File]::Open((Join-Path $root 'protocol.txt'),[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
  try{Refuses {& (Join-Path $repo 'packaging/uninstall.ps1') -Root $root}}finally{$locked.Dispose()}
  $m=Read-Install $root;Assert-Entries $root $m.files -RequireFiles
  Assert ((Get-FileHash -LiteralPath (Join-Path $root 'install-manifest.json')).Hash -eq $ledger) 'Failed uninstall changed ledger'
  Assert ((Get-FileHash -LiteralPath $shortcut).Hash -eq $shortcutHash) 'Failed uninstall removed matching shortcut'
  Assert ((Get-RunValue $key) -eq $run) 'Failed uninstall removed matching startup'
  & (Join-Path $repo 'packaging/uninstall.ps1') -Root $root
  Assert (!(Test-Path -LiteralPath (Join-Path $root 'install-manifest.json'))) 'Retry uninstall did not complete'
 }else{
  $global:PackagingFaultCase=$Case;$global:PackagingFaultRoot=$root;$global:PackagingFaultHit=$false
  # Test-process-only cmdlet interception reproduces the original pre-fix copy/write gaps.
  function global:Copy-Item {
   [CmdletBinding()]param([string]$LiteralPath,[string]$Destination,[switch]$Force)
   if($Destination -like '*.incoming-*' -and [IO.Path]::GetFileName($LiteralPath) -eq 'protocol.txt'){
    $global:PackagingFaultHit=$true
    if($global:PackagingFaultCase -eq 'edit'){[IO.File]::WriteAllText((Join-Path $global:PackagingFaultRoot 'protocol.txt'),'concurrent user edit')}
    elseif($global:PackagingFaultCase -eq 'copy'){[IO.File]::WriteAllText($Destination,'partial');throw 'Injected mid-copy failure'}
   }
   Microsoft.PowerShell.Management\Copy-Item @PSBoundParameters
  }
  function global:Set-Content {
   [CmdletBinding()]param([string]$LiteralPath,[Parameter(ValueFromPipeline)]$Value,[string]$Encoding)
   process{
    if($global:PackagingFaultCase -eq 'ledger' -and [IO.Path]::GetFileName($LiteralPath) -like 'ledger-*.tmp'){
     $global:PackagingFaultHit=$true;[IO.File]::WriteAllText($LiteralPath,'partial');throw 'Injected mid-ledger failure'
    }
    Microsoft.PowerShell.Management\Set-Content @PSBoundParameters
   }
  }
  $module=Get-Module Package
  if(Get-Command Write-TemporaryPayload -ErrorAction SilentlyContinue){
   & $module {
    param($mode,$target)
    $script:reviewMode=$mode;$script:reviewRoot=$target
    $script:normalPayload=${function:Write-TemporaryPayload};$script:normalText=${function:Write-TemporaryText}
    function script:Write-TemporaryPayload($temporary,[IO.Stream]$source){
     if($script:reviewMode -eq 'appearance' -and [IO.Path]::GetFileName($source.Name) -eq 'new-note.txt'){
      & $script:normalPayload $temporary $source;[IO.File]::WriteAllText((Join-Path $script:reviewRoot 'new-note.txt'),'unrelated user addition');$global:PackagingFaultHit=$true;return
     }
     if([IO.Path]::GetFileName($source.Name) -eq 'protocol.txt'){
      if($script:reviewMode -eq 'edit'){& $script:normalPayload $temporary $source;[IO.File]::WriteAllText((Join-Path $script:reviewRoot 'protocol.txt'),'concurrent user edit');$global:PackagingFaultHit=$true;return}
      if($script:reviewMode -in @('copy','replacement')){
       $global:PackagingFaultHit=$true;$bytes=[Text.Encoding]::UTF8.GetBytes('partial');$temporary.Stream.Write($bytes,0,$bytes.Length);$temporary.Stream.Flush()
       if($script:reviewMode -eq 'replacement'){
        $global:PackagingReplacementPath=$temporary.Path
        [IO.File]::Move($temporary.Path,$temporary.Path+'.renamed',$false)
        [IO.File]::WriteAllText($temporary.Path,'user replacement')
       }
       throw 'Injected mid-copy failure'
      }
      if($script:reviewMode -eq 'identity'){
       & $script:normalPayload $temporary $source;$path=Join-Path $script:reviewRoot 'install-manifest.json';$bytes=[IO.File]::ReadAllBytes($path)
       [IO.File]::Move($path,$path+'.user-saved',$false);[IO.File]::WriteAllBytes($path,$bytes);$global:PackagingFaultHit=$true;return
      }
     }
     & $script:normalPayload $temporary $source
    }
    function script:Write-TemporaryText($temporary,[string]$text){
     if($script:reviewMode -eq 'ledger'){$global:PackagingFaultHit=$true;$bytes=[Text.Encoding]::UTF8.GetBytes('partial');$temporary.Stream.Write($bytes,0,$bytes.Length);throw 'Injected mid-ledger failure'}
     & $script:normalText $temporary $text
    }
   } $Case $root
  }
  if($Case -in @('edit','appearance','identity')){
   $failed=$false;try{Install}catch{$failed=$true}
   if($Case -eq 'edit'){Assert ((Get-Content -LiteralPath (Join-Path $root 'protocol.txt') -Raw) -eq 'concurrent user edit') 'Post-preflight user edit was overwritten'}
   elseif($Case -eq 'appearance'){Assert ((Get-Content -LiteralPath (Join-Path $root 'new-note.txt') -Raw) -eq 'unrelated user addition') 'Post-preflight unowned addition was overwritten'}
   else{Assert (Test-Path -LiteralPath (Join-Path $root 'install-manifest.json.user-saved')) 'Replaced ledger identity was not exercised'}
   Assert $global:PackagingFaultHit 'Late-change seam was not exercised'
   Assert $failed 'Changed destination did not refuse replacement'
  }else{
   Refuses {Install}
   Assert $global:PackagingFaultHit 'Partial-write fault was not exercised'
   if($Case -eq 'replacement'){
    Assert ((Get-Content -LiteralPath $global:PackagingReplacementPath -Raw) -eq 'user replacement') 'Temporary cleanup deleted user replacement'
    Assert (!(Test-Path -LiteralPath ($global:PackagingReplacementPath+'.renamed'))) 'Exact created partial was not deleted by retained handle'
   }else{Assert (@(Get-ChildItem -LiteralPath $root -File -Recurse -Filter '*.incoming-*').Count -eq 0) 'Partial incoming temporary remained'}
   Assert (@(Get-ChildItem -LiteralPath $root -File -Filter 'ledger-*.tmp').Count -eq 0) 'Partial ledger temporary remained'
  }
 }
}finally{
 if($module -and (Get-Command Write-TemporaryPayload -ErrorAction SilentlyContinue)){
  & $module {if($script:normalPayload){Set-Item Function:script:Write-TemporaryPayload $script:normalPayload;Set-Item Function:script:Write-TemporaryText $script:normalText}}
 }
 Remove-Item Function:\Copy-Item,Function:\Set-Content -ErrorAction SilentlyContinue
 $k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($key,$true);if($k){try{$k.DeleteValue('VcXsrvWSL',$false)}finally{$k.Dispose()}}
 [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($key,$false)
}
"PASS package review case $Case; retained $base"
