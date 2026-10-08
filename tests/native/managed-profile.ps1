$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$command='call "'+$vc+'" x64 >nul && cl /nologo /W4 /D_CRT_SECURE_NO_WARNINGS /I"'+$root+'" "'+$root+'\tests\server\managed-profile.c" "'+$root+'\xorg-server\hw\xwin\winmanaged.c" "'+$root+'\libXau\AuRead.c" "'+$root+'\libXau\AuDispose.c" "'+$root+'\libXau\AuWrite.c" /Fe:"'+$out+'\managed-profile.exe" && "'+$out+'\managed-profile.exe"'
Push-Location $out
try { & cmd.exe /d /s /c $command; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}
