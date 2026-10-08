Set-StrictMode -Version Latest
$ErrorActionPreference='Stop'
function Resolve-InstallPath([string]$Path) {
    if(!$Path -or ![IO.Path]::IsPathFullyQualified($Path) -or $Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[\x00-\x1f]' -or ($Path -split '[\\/]') -contains '..'){throw 'Expected absolute local path without traversal'}
    $p=[IO.Path]::GetFullPath($Path).TrimEnd('\','/')
    if($p.Length -le 3 -or ($p -split '[\\/]').Count -lt 3){throw 'Unsafe root directory'}
    $cursor=$p
    while($cursor){
        try{$attributes=[IO.File]::GetAttributes($cursor)}catch [IO.FileNotFoundException]{$attributes=0}catch [IO.DirectoryNotFoundException]{$attributes=0}
        if($attributes -band [IO.FileAttributes]::ReparsePoint){throw "Reparse path refused: $cursor"}
        $cursor=[IO.Path]::GetDirectoryName($cursor)
    }
    $p
}
function Join-OwnedPath([string]$Root,[string]$Relative) {
    if(!$Relative -or $Relative -match '[:\x00-\x1f]' -or [IO.Path]::IsPathRooted($Relative)){throw 'Unsafe manifest path'}
    foreach($part in ($Relative -split '[\\/]')) {
        if(!$part -or $part -in @('.','..') -or $part -match '[. ]$|[<>"|?*]' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)'){throw "Unsafe manifest path: $Relative"}
    }
    $rootPath=Resolve-InstallPath $Root
    $p=[IO.Path]::GetFullPath((Join-Path $rootPath $Relative))
    if(!$p.StartsWith($rootPath+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Manifest path escaped root'}
    $null=Resolve-InstallPath $p
    $p
}
function Get-OwnedEntry([string]$Root,[string]$Relative) {
    $path=Join-OwnedPath $Root $Relative
    $f=Get-Item -LiteralPath $path -Force
    if($f.PSIsContainer -or $f.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Owned target is not a regular file'}
    [pscustomobject]@{path=$Relative.Replace('\','/');bytes=$f.Length;sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}
}
function Test-OwnedEntry([string]$Root,$Entry) {
    $path=Join-OwnedPath $Root $Entry.path
    if(!(Test-Path -LiteralPath $path -PathType Leaf)){return $false}
    $f=Get-Item -LiteralPath $path -Force
    if($f.Length -ne $Entry.bytes){return $false}
    (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -eq $Entry.sha256
}
function Assert-Entries([string]$Root,$Entries,[switch]$RequireFiles) {
    $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach($e in $Entries){
        $p=Join-OwnedPath $Root $e.path
        if(!$seen.Add($p) -or $e.sha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $e.bytes -lt 0){throw 'Invalid or duplicate manifest entry'}
        if($RequireFiles -and !(Test-OwnedEntry $Root $e)){throw "Missing or tampered artifact: $($e.path)"}
    }
}
function Read-Bundle([string]$Bundle) {
    $bundle=Resolve-InstallPath $Bundle
    $m=Get-Content -LiteralPath (Join-OwnedPath $bundle 'bundle-manifest.json') -Raw | ConvertFrom-Json
    if($m.schemaVersion -ne 1 -or $m.product -ne 'VcXsrvWSL' -or !$m.unsignedDevelopment -or !$m.files.Count){throw 'Invalid development bundle manifest'}
    Assert-Entries $bundle $m.files -RequireFiles
    foreach($name in @('vcxsrv-wsl-manager.exe','vcxsrv.exe','vcruntime140.dll')){if($name -notin $m.files.path){throw "Required artifact missing: $name"}}
    $m
}
function Read-Install([string]$Root) {
    $path=Join-OwnedPath $Root 'install-manifest.json'
    if(!(Test-Path -LiteralPath $path -PathType Leaf)){return $null}
    $m=Get-Content -LiteralPath $path -Raw | ConvertFrom-Json
    if($m.schemaVersion -ne 1 -or $m.product -ne 'VcXsrvWSL' -or !(Resolve-InstallPath $m.root).Equals((Resolve-InstallPath $Root),[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid installation manifest/root binding'}
    Assert-Entries $Root $m.files
    Assert-Entries $Root $m.history
    if($m.previous){Assert-Entries $Root $m.previous.files; $null=Join-OwnedPath $Root ($m.previous.path+'/install-manifest.json')}
    $m
}
function Copy-Owned([string]$SourceRoot,[string]$DestinationRoot,$Entries,[switch]$Replace) {
    Assert-Entries $SourceRoot $Entries -RequireFiles
    foreach($e in $Entries){
        $dst=Join-OwnedPath $DestinationRoot $e.path
        $null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($dst)) -Force
        if(!$Replace -and (Test-Path -LiteralPath $dst)){throw "Destination collision: $($e.path)"}
        Copy-Item -LiteralPath (Join-OwnedPath $SourceRoot $e.path) -Destination $dst -Force:$Replace
    }
    Assert-Entries $DestinationRoot $Entries -RequireFiles
}
function Remove-Owned([string]$Root,$Entries) {
    foreach($e in $Entries){
        $p=Join-OwnedPath $Root $e.path
        if(!(Test-Path -LiteralPath $p)){continue}
        if(Test-OwnedEntry $Root $e){Remove-Item -LiteralPath $p -Force}else{Write-Warning "Preserved modified owned file: $($e.path)"}
    }
}
function Initialize-HandleInterop {
    if('VcXsrvPackagingHandles' -as [type]){return}
    Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
using System.IO;
using System.ComponentModel;
using Microsoft.Win32.SafeHandles;
public static class VcXsrvPackagingHandles {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool DuplicateHandle(SafeProcessHandle source, IntPtr handle, IntPtr target, out SafeWaitHandle duplicate, uint access, bool inherit, uint options);
 [DllImport("kernel32.dll")] public static extern IntPtr GetCurrentProcess();
 [StructLayout(LayoutKind.Sequential)] public struct Info { public uint Attributes, CreationLow, CreationHigh, AccessLow, AccessHigh, WriteLow, WriteHigh, Volume, SizeHigh, SizeLow, Links, IndexHigh, IndexLow; }
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool GetFileInformationByHandle(SafeFileHandle file, out Info info);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] public static extern SafeFileHandle CreateFileW(string path,uint access,uint share,IntPtr security,uint creation,uint flags,IntPtr template);
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool SetFileInformationByHandle(SafeFileHandle file,int kind,ref byte disposition,uint size);
 public static string Identity(SafeFileHandle file) { Info i; if(!GetFileInformationByHandle(file,out i)) throw new Win32Exception(Marshal.GetLastWin32Error()); return i.Volume+":"+i.IndexHigh+":"+i.IndexLow; }
 public static string Identity(string path) { using(var file=new FileStream(path,FileMode.Open,FileAccess.Read,FileShare.ReadWrite|FileShare.Delete)) return Identity(file.SafeFileHandle); }
}
public sealed class VcXsrvOwnedTemporary : IDisposable {
 public string Path {get;private set;}
 public FileStream Stream {get;private set;}
 private bool committed;
 private bool disposed;
 public VcXsrvOwnedTemporary(string path) {
  Path=path;
  var handle=VcXsrvPackagingHandles.CreateFileW(path,0xC0010000,5,IntPtr.Zero,1,0x100,IntPtr.Zero);
  if(handle.IsInvalid){int error=Marshal.GetLastWin32Error();handle.Dispose();throw new Win32Exception(error);}
  Stream=new FileStream(handle,FileAccess.ReadWrite);
 }
 public void Commit(string destination,bool overwrite) {
  if(VcXsrvPackagingHandles.Identity(Path)!=VcXsrvPackagingHandles.Identity(Stream.SafeFileHandle)) throw new IOException("Temporary pathname no longer names the created file");
  File.Move(Path,destination,overwrite);committed=true;disposed=true;Stream.Dispose();
 }
 public void Dispose() {
  if(disposed)return;disposed=true;
  try{if(!committed){byte delete=1;if(!VcXsrvPackagingHandles.SetFileInformationByHandle(Stream.SafeFileHandle,4,ref delete,1))throw new Win32Exception(Marshal.GetLastWin32Error());}}
  finally{Stream.Dispose();}
 }
}
"@
}
function Acquire-InstallationGuard([string]$GuardRoot,[string]$Root,[string]$StateRoot) {
    Initialize-HandleInterop
    $start=[Diagnostics.ProcessStartInfo]::new((Join-OwnedPath $GuardRoot 'vcxsrv-wsl-manager.exe'))
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true
    $start.RedirectStandardInput=$true;$start.RedirectStandardOutput=$true;$start.RedirectStandardError=$true
    foreach($a in @('installation-guard','--root',$Root,'--state-dir',$StateRoot)){$start.ArgumentList.Add($a)}
    $p=[Diagnostics.Process]::Start($start)
    $local=$null
    try {
        $read=$p.StandardOutput.ReadLineAsync()
        if(!$read.Wait(15000)){throw 'Installation guard reply timed out'}
        $line=$read.Result
        if(!$line -or $line.Length -gt 512){throw 'Installation guard rejected maintenance; close manager and Stop owned server or recover stopped metadata explicitly'}
        $reply=$line|ConvertFrom-Json
        if($reply.version -ne 1 -or !$reply.ready -or $reply.mutex_handle -notmatch '^0x[0-9a-f]{1,16}$'){throw 'Invalid installation guard reply'}
        $remote=[IntPtr]::new([Convert]::ToInt64($reply.mutex_handle.Substring(2),16))
        $dup=[VcXsrvPackagingHandles]::DuplicateHandle($p.SafeHandle,$remote,[VcXsrvPackagingHandles]::GetCurrentProcess(),[ref]$local,0,$false,2)
        if(!$dup){throw 'Could not retain installation singleton'}
        $p.StandardInput.WriteLine('RELEASE');$p.StandardInput.Close()
        if(!$p.WaitForExit(15000) -or $p.ExitCode -ne 0){throw 'Installation guard handoff did not complete'}
        $result=$local;$local=$null
        return $result
    } finally {
        if($local){$local.Dispose()}
        try{$p.StandardInput.Close()}catch{}
        if(!$p.HasExited){$null=$p.WaitForExit(16000)}
        $p.Dispose()
    }
}
function Get-Defaults {
    $local=[Environment]::GetFolderPath('LocalApplicationData')
    @{Root=(Join-Path $local 'Programs/VcXsrvWSL');StateRoot=(Join-Path $local 'VcXsrvWSL');ShortcutPath=(Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs/VcXsrv WSL Manager.lnk');StartupKey='Software\Microsoft\Windows\CurrentVersion\Run'}
}
function New-GuardStage([string]$Root,$Manifest) {
    $p=Join-Path ([IO.Path]::GetTempPath()) ('vcxsrv-install-guard-'+[guid]::NewGuid())
    $e=@($Manifest.files|Where-Object path -In @('vcxsrv-wsl-manager.exe','vcruntime140.dll'))
    if($e.Count -ne 2){throw 'Verified guard files are missing'}
    Copy-Owned $Root $p $e
    @{Root=$p;Files=$e}
}
function Get-StartupCommand([string]$Root,[string]$StateRoot) {
    $args=@((Join-Path $Root 'vcxsrv-wsl-manager.exe'),'--tray','--state-dir',$StateRoot)
    (@($args | ForEach-Object {if($_ -match '"'){throw 'Invalid startup path'};if($_.Contains(' ')){'"'+$_+'"'}else{$_}}) -join ' ')
}
function Get-RunValue([string]$Key) {
    $k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Key,$false)
    if(!$k){return $null}
    try{$v=$k.GetValue('VcXsrvWSL',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames);if($v -and $k.GetValueKind('VcXsrvWSL') -ne [Microsoft.Win32.RegistryValueKind]::String){throw 'Startup entry kind is invalid'};$v}finally{$k.Dispose()}
}
function Set-RunValue([string]$Key,[string]$Value) {
    $k=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($Key)
    try{$k.SetValue('VcXsrvWSL',$Value,[Microsoft.Win32.RegistryValueKind]::String)}finally{$k.Dispose()}
}
function Remove-MatchingRun([string]$Key,[string]$Value) {
    if((Get-RunValue $Key) -eq $Value){$k=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($Key,$true);try{$k.DeleteValue('VcXsrvWSL',$false)}finally{$k.Dispose()}}else{Write-Warning 'Preserved startup entry changed by user/another installation'}
}
function Assert-Shortcut([string]$Path,[string]$Target,[string]$Arguments,[string]$Directory) {
    $null=Resolve-InstallPath $Path
    if(!(Test-Path -LiteralPath $Path)){return}
    $shell=New-Object -ComObject WScript.Shell
    try{$s=$shell.CreateShortcut($Path);if($s.TargetPath -ne $Target -or $s.Arguments -ne $Arguments -or $s.WorkingDirectory -ne $Directory){throw 'Shortcut belongs to another installation or was modified'}}finally{[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
}
function Save-Shortcut([string]$Path,[string]$Root,[string]$StateRoot) {
    $target=Join-Path $Root 'vcxsrv-wsl-manager.exe'
    $args='--tray --state-dir "'+$StateRoot+'"'
    Assert-Shortcut $Path $target $args $Root
    $null=New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($Path)) -Force
    $shell=New-Object -ComObject WScript.Shell
    try{$s=$shell.CreateShortcut($Path);$s.TargetPath=$target;$s.Arguments=$args;$s.WorkingDirectory=$Root;$s.Save()}finally{[void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($shell)}
    @{path=$Path;target=$target;arguments=$args;directory=$Root;sha256=(Get-FileHash -LiteralPath $Path).Hash}
}
function Write-TemporaryPayload($Temporary,[IO.Stream]$Source) {$Source.CopyTo($Temporary.Stream)}
function Write-TemporaryText($Temporary,[string]$Text) {
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes($Text)
    $Temporary.Stream.Write($bytes,0,$bytes.Length)
}
function Close-OwnedTemporary($Temporary) {if($Temporary){try{$Temporary.Dispose()}catch{Write-Warning "Exact temporary cleanup failed: $($_.Exception.Message)"}}}
function Save-Ledger([string]$Root,$Ledger,[string]$ExpectedIdentity,[string]$ExpectedHash,[switch]$RequireAbsent) {
    $path=Join-OwnedPath $Root 'install-manifest.json'
    $tmp=Join-OwnedPath $Root ('ledger-'+[guid]::NewGuid()+'.tmp')
    Initialize-HandleInterop
    $owned=[VcXsrvOwnedTemporary]::new($tmp)
    try{
        Write-TemporaryText $owned ($Ledger|ConvertTo-Json -Depth 30)
        $owned.Stream.Flush($true)
        $null=Join-OwnedPath $Root 'install-manifest.json'
        if($ExpectedIdentity){if([VcXsrvPackagingHandles]::Identity($path) -ne $ExpectedIdentity -or (Get-FileHash -LiteralPath $path).Hash -ne $ExpectedHash){throw 'Ledger changed before commit'}}
        elseif($RequireAbsent -and (Test-Path -LiteralPath $path)){throw 'Unowned ledger appeared before commit'}
        $owned.Commit($path,$true)
    }finally{Close-OwnedTemporary $owned}
}
Export-ModuleMember -Function *
