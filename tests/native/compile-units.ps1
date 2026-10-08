param([Parameter(Mandatory)][string]$BuildRoot)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$dirs=@('','include','pthreads','pixman/pixman','pixman-1','xorg-server','xorg-server/include','xorg-server/dix','xorg-server/mi','xorg-server/mfb','xorg-server/hw/xfree86/common','xorg-server/render','xorg-server/miext/damage','xorg-server/damageext','xorg-server/randr','xorg-server/xfixes','xorg-server/Xi','xorg-server/fb','xorg-server/miext/cw','xorg-server/miext/shadow','xorg-server/miext/sync','xorg-server/miext/rootless','xorg-server/Xext','xorg-server/present','xorg-server/hw/xwin','xorg-server/hw/xwin/winclipboard')
$includes=$dirs|ForEach-Object{'/I"'+(Join-Path $root $_).TrimEnd('\')+'"'}
$includes += '/I"'+(Join-Path $BuildRoot 'xorg-server/hw/xwin/obj64/servrelease')+'"'
$flags='/nologo /c /W3 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /D_WINDOWS /D__MINGW32__ /D_POSIX_ /DX_NOT_POSIX /D_TIMEVAL_DEFINED /Dmode_t=int /D__STDC__ /DFAKEIT /DHAVE_CONFIG_H /D_BSD_SOURCE /D_WIN32_WINNT=0x0501 /DHAVE_XWIN_CONFIG_H /DPTW32_STATIC_LIB /DPIXMAN_API= /DRELOCATE_PROJECTROOT /DREAD_FONTDIRS'
$files=@('os/xstrans.c','os/xserver_poll.c','os/winpoll.c','os/winlocal.c','os/connection.c','os/access.c','hw/xwin/windisplay.c','hw/xwin/winxcb.c','hw/xwin/winauth.c','hw/xwin/InitOutput.c','hw/xwin/winmultiwindowwm.c','hw/xwin/winclipboardinit.c','hw/xwin/winclipboard/thread.c')
$commands=@(('call "'+$vc+'" x64 >nul'))
foreach($file in $files){$commands += 'cl '+$flags+' '+($includes -join ' ')+' "'+(Join-Path $root ('xorg-server/'+$file))+'"'}
$batch=Join-Path $out 'compile-units.cmd'
($commands -join "`r`nif errorlevel 1 exit /b 1`r`n") | Set-Content $batch
Push-Location $out
try { & cmd.exe /d /s /c $batch; if($LASTEXITCODE -ne 0){exit $LASTEXITCODE} } finally {Pop-Location}
