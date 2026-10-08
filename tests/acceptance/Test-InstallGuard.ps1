param([string]$ManagerBinary=(Join-Path $PSScriptRoot '../../manager/target/release/vcxsrv-wsl-manager.exe'))
$ErrorActionPreference='Stop'
if(!('GuardFixtureHandles' -as [type])) {
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class GuardFixtureHandles {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool DuplicateHandle(SafeProcessHandle source, IntPtr handle, IntPtr target, out SafeWaitHandle duplicate, uint access, bool inherit, uint options);
 [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool GetHandleInformation(SafeWaitHandle handle, out uint flags);
}
"@
}
$root=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv guard '+[guid]::NewGuid())
$install=Join-Path $root 'install with spaces';$state=Join-Path $root 'state'
function Start-Guard([string]$Target=$install) {
 $s=[Diagnostics.ProcessStartInfo]::new([IO.Path]::GetFullPath($ManagerBinary));$s.UseShellExecute=$false;$s.CreateNoWindow=$true
 $s.RedirectStandardInput=$true;$s.RedirectStandardOutput=$true;$s.RedirectStandardError=$true
 foreach($a in @('installation-guard','--root',$Target,'--state-dir',$state)){$s.ArgumentList.Add($a)}
 [Diagnostics.Process]::Start($s)
}
function Wait-Guard($P) {if(!$P.WaitForExit(17000)){throw 'Guard exceeded bounded protocol'};$P.ExitCode}
function Assert($Condition,[string]$Message){if(!$Condition){throw $Message}}
$p=Start-Guard
$p.StandardInput.WriteLine('RELEASE');$p.StandardInput.Flush()
Assert ((Wait-Guard $p) -eq 0) 'Expected successful native guard protocol'
$r=$p.StandardOutput.ReadToEnd()|ConvertFrom-Json
Assert ($r.ready -and $r.version -eq 1 -and $r.mutex_handle) 'No validated guard reply'
Assert (!(Test-Path -LiteralPath $root)) 'Read-only guard created installation/state'
$p.Dispose()
foreach($token in @('', 'WRONG',('x'*20))) {
 $p=Start-Guard;$line=$p.StandardOutput.ReadLine();Assert ([bool]$line) 'Expected readiness before invalid handshake'
 if($token){$p.StandardInput.WriteLine($token)};$p.StandardInput.Close()
 Assert ((Wait-Guard $p) -ne 0) 'Invalid release must fail';$p.Dispose()
}
# Real cross-process duplicate: killing the disposable helper must not release the parent guard.
$p=Start-Guard;$reply=$p.StandardOutput.ReadLine()|ConvertFrom-Json
$local=$null;$remote=[IntPtr]::new([Convert]::ToInt64($reply.mutex_handle.Substring(2),16))
Assert ([GuardFixtureHandles]::DuplicateHandle($p.SafeHandle,$remote,[GuardFixtureHandles]::GetCurrentProcess(),[ref]$local,0,$false,2)) 'Duplicate failed'
try {
 $p.Kill();$null=Wait-Guard $p
 $second=Start-Guard;$second.StandardInput.Close();Assert ((Wait-Guard $second) -ne 0) 'Helper crash reopened manager race';$second.Dispose()
}finally{$local.Dispose();$p.Dispose()}
# Production acquisition helper retains handle after normal child exit too.
$p=Start-Guard;$reply=$p.StandardOutput.ReadLine()|ConvertFrom-Json
$local=$null;$remote=[IntPtr]::new([Convert]::ToInt64($reply.mutex_handle.Substring(2),16))
Assert ([GuardFixtureHandles]::DuplicateHandle($p.SafeHandle,$remote,[GuardFixtureHandles]::GetCurrentProcess(),[ref]$local,0,$false,2)) 'Successful duplicate failed'
$flags=[uint32]0
Assert ([GuardFixtureHandles]::GetHandleInformation($local,[ref]$flags) -and ($flags -band 1) -eq 0) 'Guard was inheritable'
$p.StandardInput.WriteLine('RELEASE');$p.StandardInput.Flush()
Assert ((Wait-Guard $p) -eq 0) 'Successful RELEASE failed';$p.Dispose()
try{$p=Start-Guard;$p.StandardInput.Close();Assert ((Wait-Guard $p) -ne 0) 'Successful handoff lost singleton';$p.Dispose()}finally{$local.Dispose()}
$p=Start-Guard;$p.StandardInput.WriteLine('RELEASE');$p.StandardInput.Flush();Assert ((Wait-Guard $p) -eq 0) 'Final disposal leaked singleton';$p.Dispose()
$p=Start-Guard ($install+'\..\bad');$p.StandardInput.Close();Assert ((Wait-Guard $p) -ne 0) 'Traversal root accepted';$p.Dispose()
$p=Start-Guard;$line=$p.StandardOutput.ReadLine();Assert ([bool]$line) 'Expected timeout readiness'
Assert ((Wait-Guard $p) -ne 0) 'Missing RELEASE did not time out';$p.StandardInput.Close();$p.Dispose()
'PASS native fixed protocol/no-create/invalid-input/crash-after-duplicate/success-handoff/release/path rejection'

