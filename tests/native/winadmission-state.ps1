$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$includes='/I"'+$root+'" /I"'+$root+'\include" /I"'+$root+'\pthreads"'
$flags='/nologo /W3 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /DPTW32_STATIC_LIB'
$command='call "'+$vc+'" x64 >nul && cl '+$flags+' '+$includes+' "'+$root+'\tests\native\winadmission-state-test.c" "'+$root+'\xorg-server\hw\xwin\winadmission.c" /Fe:"'+$out+'\winadmission-state-test.exe" /link user32.lib && "'+$out+'\winadmission-state-test.exe"'
Push-Location $out
try { & cmd.exe /d /s /c $command; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}
