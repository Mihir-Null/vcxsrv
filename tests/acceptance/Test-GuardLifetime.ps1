param(
 [string]$ManagerBinary=(Join-Path $PSScriptRoot '../../manager/target/release/vcxsrv-wsl-manager.exe'),
 [ValidateSet('supervisor','before','after')][string]$FixtureMode='supervisor',
 [string]$FixtureRoot
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
Import-Module (Join-Path $repo 'packaging/Package.psm1') -Force -DisableNameChecking
$ManagerBinary=[IO.Path]::GetFullPath($ManagerBinary)
function Assert($condition,$message){if(!$condition){throw $message}}
function Start-Guard {
 $s=[Diagnostics.ProcessStartInfo]::new($ManagerBinary);$s.UseShellExecute=$false;$s.CreateNoWindow=$true
 $s.RedirectStandardInput=$true;$s.RedirectStandardOutput=$true;$s.RedirectStandardError=$true
 foreach($a in @('installation-guard','--root',(Join-Path $FixtureRoot 'install'),'--state-dir',(Join-Path $FixtureRoot 'state'))){$s.ArgumentList.Add($a)}
 [Diagnostics.Process]::Start($s)
}
function Write-ChildMarker($p){@{pid=$p.Id;startTicks=$p.StartTime.ToUniversalTime().Ticks}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $FixtureRoot 'child.json')}
if($FixtureMode -ne 'supervisor'){
 if($FixtureMode -eq 'before'){
  $child=Start-Guard
  $line=$child.StandardOutput.ReadLine();Assert ([bool]$line) 'Before-duplicate helper did not acquire guard'
  Write-ChildMarker $child
 }else{
  $guard=Acquire-InstallationGuard ([IO.Path]::GetDirectoryName($ManagerBinary)) (Join-Path $FixtureRoot 'install') (Join-Path $FixtureRoot 'state')
  # Spawn an actual living child after duplication; it must not retain the singleton.
  $s=[Diagnostics.ProcessStartInfo]::new((Join-Path $repo 'manager/target/debug/process-fixture.exe'),'sleep');$s.UseShellExecute=$false;$s.CreateNoWindow=$true
  $child=[Diagnostics.Process]::Start($s);Write-ChildMarker $child
 }
 $null=[Console]::ReadLine()
 # Intentional abrupt parent termination: no explicit SafeHandle/pipe disposal.
 [Environment]::Exit(0)
}
$base=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv parent lifetime '+[guid]::NewGuid())
foreach($mode in @('before','after')){
 $FixtureRoot=Join-Path $base $mode;$null=New-Item -ItemType Directory -Path $FixtureRoot
 $s=[Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'pwsh.exe'));$s.UseShellExecute=$false;$s.CreateNoWindow=$true
 $s.RedirectStandardInput=$true;$s.RedirectStandardOutput=$true;$s.RedirectStandardError=$true
 foreach($a in @('-NoProfile','-File',$PSCommandPath,'-ManagerBinary',$ManagerBinary,'-FixtureMode',$mode,'-FixtureRoot',$FixtureRoot)){$s.ArgumentList.Add($a)}
 $parent=[Diagnostics.Process]::Start($s);$child=$null
 try{
  $deadline=[DateTime]::UtcNow.AddSeconds(20);$marker=Join-Path $FixtureRoot 'child.json'
  while(!(Test-Path -LiteralPath $marker)){
   if($parent.HasExited){throw ('Lifetime parent did not publish child: '+$parent.StandardError.ReadToEnd())}
   if([DateTime]::UtcNow -gt $deadline){throw 'Lifetime parent readiness timed out'}
   Start-Sleep -Milliseconds 50
  }
  $identity=Get-Content -LiteralPath $marker -Raw|ConvertFrom-Json
  $child=[Diagnostics.Process]::GetProcessById($identity.pid)
  # Pin the exact process before parent exit, refusing PID reuse.
  $null=$child.SafeHandle;Assert ($child.StartTime.ToUniversalTime().Ticks -eq $identity.startTicks) 'Child PID identity changed'
  $probe=Start-Guard;$probe.StandardInput.Close()
  Assert ($probe.WaitForExit(17000) -and $probe.ExitCode -ne 0) 'Parent did not retain singleton';$probe.Dispose()
  $parent.StandardInput.WriteLine('EXIT');$parent.StandardInput.Flush()
  Assert ($parent.WaitForExit(10000) -and $parent.ExitCode -eq 0) 'Fixture parent exit failed'
  if($mode -eq 'before'){
   Assert ($child.WaitForExit(17000) -and $child.ExitCode -ne 0) 'Parent exit before duplicate did not reject EOF/release guard'
  }else{Assert (!$child.HasExited) 'Noninheritance child was not living after parent exit'}
  $probe=Start-Guard;$line=$probe.StandardOutput.ReadLine();Assert ([bool]$line) 'Parent exit leaked singleton into living child'
  $probe.StandardInput.WriteLine('RELEASE');$probe.StandardInput.Flush()
  Assert ($probe.WaitForExit(17000) -and $probe.ExitCode -eq 0) 'Guard was unavailable after parent exit';$probe.Dispose()
 }finally{
  # Only retained, identity-checked disposable fixture handles may be terminated.
  if(!$parent.HasExited){$parent.Kill();$parent.WaitForExit()};$parent.Dispose()
  if($child){if(!$child.HasExited){$child.Kill();$child.WaitForExit()};$child.Dispose()}
 }
}
"PASS actual parent exit before/after duplicate and living-child noninheritance; retained $base"
