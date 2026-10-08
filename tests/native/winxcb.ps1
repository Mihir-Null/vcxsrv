$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$command='call "'+$vc+'" x64 >nul && cl /nologo /W3 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /D_WIN32_WINNT=0x0501 /DPTW32_STATIC_LIB /I"'+$root+'" /I"'+$root+'\include" /I"'+$root+'\pthreads" "'+$root+'\tests\native\winxcb-test.c" "'+$root+'\xorg-server\hw\xwin\winxcb.c" "'+$root+'\xorg-server\os\winlocal.c" /Fe:"'+$out+'\winxcb-test.exe" /link ws2_32.lib advapi32.lib bcrypt.lib && "'+$out+'\winxcb-test.exe"'
Push-Location $out
try { & cmd.exe /d /s /c $command; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}
