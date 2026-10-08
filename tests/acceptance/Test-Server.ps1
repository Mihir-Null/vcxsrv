param(
    [Parameter(Mandatory)][string]$Server,
    [Parameter(Mandatory)][string]$Authority,
    [string]$Distro='NixOS',
    [int]$Display=1,
    [int]$StartupDisplay=191,
    [int]$ServerProcessId=0,
    [string]$UnixEndpoint,
    [string]$GuestAuthority,
    [string]$LinuxProbePath,
    [string]$GuestPython='python3',
    [string]$GuestAuthorityDisplay='0'
)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$evidence=Join-Path $root ('.native-tests/acceptance-'+[guid]::NewGuid())
New-Item -ItemType Directory -Force $evidence | Out-Null
$serverPath=(Resolve-Path -LiteralPath $Server).Path
$authorityPath=(Resolve-Path -LiteralPath $Authority).Path
$invalidAuthority=Join-Path $evidence 'invalid.auth'
Set-Content -LiteralPath $invalidAuthority -Value 'invalid authority' -NoNewline

function Assert-RejectedStartup([string[]]$ServerArguments) {
    $info=[Diagnostics.ProcessStartInfo]::new($serverPath)
    $info.WorkingDirectory=Split-Path $serverPath -Parent
    $info.UseShellExecute=$false
    $info.CreateNoWindow=$true
    $info.WindowStyle=[Diagnostics.ProcessWindowStyle]::Hidden
    $info.RedirectStandardError=$true
    $info.RedirectStandardOutput=$true
    foreach($argument in (@(":$StartupDisplay",'-silent-dup-error')+$ServerArguments)) {
        $info.ArgumentList.Add($argument)
    }
    $process=[Diagnostics.Process]::Start($info)
    $stderr=$process.StandardError.ReadToEndAsync()
    $stdout=$process.StandardOutput.ReadToEndAsync()
    try {
        if(!$process.WaitForExit(10000)) {
            $process.Kill() # This helper owns only this rejected-startup process.
            $process.WaitForExit()
            throw "Rejected startup did not exit within ten seconds: $($ServerArguments -join ' ')"
        }
        $diagnostic=$stderr.GetAwaiter().GetResult()+$stdout.GetAwaiter().GetResult()
        if($process.ExitCode -eq 0 -or $diagnostic -notmatch 'WSL profile:') {
            throw "Expected nonzero managed-profile rejection: $($ServerArguments -join ' ')"
        }
        $diagnostic | Add-Content -LiteralPath (Join-Path $evidence 'startup.log')
    } finally { $process.Dispose() }
}

Assert-RejectedStartup @('-wslmanaged')
Assert-RejectedStartup @('-wslmanaged','-auth')
Assert-RejectedStartup @('-wslmanaged','-auth',(Join-Path $evidence 'missing.auth'))
Assert-RejectedStartup @('-auth',(Join-Path $evidence 'missing.auth'),'-wslmanaged')
Assert-RejectedStartup @('-wslmanaged','-auth',$invalidAuthority)
Assert-RejectedStartup @('-auth',$invalidAuthority,'-wslmanaged')
$conflicts=@(
    @('-ac'), @('-listen','tcp'), @('-listen','inet'), @('-listen','inet6'),
    @('-nolisten','winlocal'), @('-nolisten','hyperv'), @('-nowslvsock'),
    @('-noclipboard'), @('-rootless'), @('-fullscreen')
)
foreach($conflict in $conflicts) {
    Assert-RejectedStartup (@('-wslmanaged')+$conflict+@('-auth',$authorityPath))
    Assert-RejectedStartup ($conflict+@('-wslmanaged','-auth',$authorityPath))
}
Write-Host 'PASS managed startup rejects missing/invalid authority and conflicting flags in both orders'

function Assert-NoTcp {
    $listeners=@(Get-NetTCPConnection -ErrorAction Stop |
        Where-Object { $_.State -eq 'Listen' -and $_.OwningProcess -eq $ServerProcessId })
    if($listeners.Count) { throw 'Managed test server owns a TCP listener' }
}
function Assert-Probe([string[]]$ProbeArguments,[int]$Expected) {
    $probeOutput=& (Join-Path $PSHOME 'pwsh.exe') -NoProfile -File (Join-Path $PSScriptRoot 'x11-probe.ps1') @ProbeArguments
    $expectedText=if($Expected -eq 0){'ACCEPTED'}else{'REJECTED'}
    if($LASTEXITCODE -ne $Expected -or @($probeOutput).Count -ne 1 -or
        ([string]@($probeOutput)[0]).Trim() -ne $expectedText) {
        throw "Setup probe expected $expectedText with exit $Expected, received exit $LASTEXITCODE"
    }
    Write-Host $probeOutput
    Assert-NoTcp
}
if($ServerProcessId) {
    $null=Get-Process -Id $ServerProcessId
    if(!$UnixEndpoint) { throw 'UnixEndpoint is required for live local acceptance' }
    Assert-NoTcp
    $base=@('-UnixEndpoint',$UnixEndpoint,'-Display',"$Display")
    Assert-Probe $base 1
    Assert-Probe ($base+@('-Authority',$authorityPath,'-WrongCookie')) 1
    Assert-Probe ($base+@('-Authority',$authorityPath)) 0
    Write-Host 'PASS real local setup: no cookie rejected, wrong cookie rejected, valid cookie accepted; no TCP listeners'
}
if($GuestAuthority -or $LinuxProbePath) {
    if(!$ServerProcessId -or !$GuestAuthority -or !$LinuxProbePath) {
        throw 'Live VSOCK acceptance requires ServerProcessId, GuestAuthority and LinuxProbePath'
    }
    $base=@('--vsock-port',"$(106000+$Display)",'--display',$GuestAuthorityDisplay)
    foreach($case in @(
        @{ Args=$base; Expected=1 },
        @{ Args=($base+@('--authority',$GuestAuthority,'--wrong-cookie')); Expected=1 },
        @{ Args=($base+@('--authority',$GuestAuthority)); Expected=0 }
    )) {
        $probeArguments=$case.Args
        $probeOutput=& wsl.exe --distribution $Distro --exec $GuestPython $LinuxProbePath @probeArguments
        $expectedText=if($case.Expected -eq 0){'ACCEPTED'}else{'REJECTED'}
        if($LASTEXITCODE -ne $case.Expected -or @($probeOutput).Count -ne 1 -or
            ([string]@($probeOutput)[0]).Trim() -ne $expectedText) {
            throw 'Real VSOCK setup acceptance failed or the guest probe did not execute'
        }
        Write-Host $probeOutput
        Assert-NoTcp
    }
    Write-Host 'PASS real VSOCK setup: no cookie rejected, wrong cookie rejected, valid cookie accepted; no TCP listeners'
}
Write-Host "Evidence: $evidence"
# Live server lifecycle remains with the caller, who supplied its exact PID.
# This script never shuts down WSL or an existing server/Emacs session.
