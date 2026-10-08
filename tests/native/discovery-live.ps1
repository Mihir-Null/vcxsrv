param([string]$Distro='NixOS',[switch]$QueryVmId)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$command='call "'+$vc+'" x64 >nul && cl /nologo /W4 /D_CRT_SECURE_NO_WARNINGS "'+$root+'\tests\server\discovery-live.c" "'+$root+'\xorg-server\hw\xwin\winwsldiscovery.c" /Fe:"'+$out+'\discovery-live.exe" /link shell32.lib advapi32.lib'
Push-Location $out
try {
    & cmd.exe /d /s /c $command
    if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
    $testArgs=@($Distro)
    if($QueryVmId){$testArgs+='--query-vm-id'}
    & (Join-Path $out 'discovery-live.exe') @testArgs
    if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
} finally {Pop-Location}
