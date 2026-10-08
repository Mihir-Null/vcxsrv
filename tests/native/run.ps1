param([string]$Test='winlocal')
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$extra=if($Test -eq 'winpoll'){' "'+$root+'\xorg-server\os\winlocal.c"'}else{''}
$command='call "'+$vc+'" x64 >nul && cl /nologo /W4 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /D_WIN32_WINNT=0x0A00 "'+$root+'\tests\native\'+$Test+'-test.c" "'+$root+'\xorg-server\os\'+$Test+'.c"'+$extra+' /Fe:"'+$out+'\'+$Test+'-test.exe" /link ws2_32.lib advapi32.lib bcrypt.lib && "'+$out+'\'+$Test+'-test.exe"'
Push-Location $out
try { & cmd.exe /d /s /c $command; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}
