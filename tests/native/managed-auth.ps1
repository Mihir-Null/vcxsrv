param([Parameter(Mandatory)][string]$BuildRoot)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$dirs=@('','include','pthreads','pixman/pixman','xorg-server','xorg-server/include','xorg-server/dix','xorg-server/mi','xorg-server/hw/xfree86/common','xorg-server/render','xorg-server/Xext')
$includes=$dirs|ForEach-Object{'/I"'+(Join-Path $root $_).TrimEnd('\')+'"'}
$includes += '/I"'+(Join-Path $BuildRoot 'xorg-server/hw/xwin/obj64/servrelease')+'"'
$flags='/nologo /Gy /W3 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /D_WINDOWS /D__MINGW32__ /D_POSIX_ /DX_NOT_POSIX /D_TIMEVAL_DEFINED /Dmode_t=int /D__STDC__ /DFAKEIT /DHAVE_CONFIG_H /D_BSD_SOURCE /D_WIN32_WINNT=0x0501 /DPIXMAN_API='
$command='call "'+$vc+'" x64 >nul && cl '+$flags+' '+($includes -join ' ')+' "'+$root+'\tests\server\managed-auth.c" "'+$root+'\xorg-server\os\clientauth.c" /Fe:"'+$out+'\managed-auth.exe" /link /OPT:REF ws2_32.lib && "'+$out+'\managed-auth.exe"'
Push-Location $out
try { & cmd.exe /d /s /c $command; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}


