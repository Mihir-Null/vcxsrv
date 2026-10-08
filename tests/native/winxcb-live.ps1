param([Parameter(Mandatory)][string]$XcbBuildRoot)
$ErrorActionPreference='Stop'
$root=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$out=Join-Path $root '.native-tests'
New-Item -ItemType Directory -Force $out | Out-Null
$vc='C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'
$lib=Join-Path $XcbBuildRoot 'libxcb/src/obj64/release'
$inputs = @(Get-ChildItem -LiteralPath $lib -Filter '*.obj' | ForEach-Object { '"'+$_.FullName+'"' })
$inputs += @('libXau/obj64/release/libXau.lib','xcb-util-errors/obj64/release/libxcb-errors.lib','pthreads/libpthreadVC364.lib') | ForEach-Object { '"'+(Join-Path $XcbBuildRoot $_)+'"' }
$rsp=Join-Path $out 'xcb-link.rsp'
@('/nologo','/DLL','/MACHINE:X64',('/OUT:"'+$out+'\libxcb.dll"'),('/IMPLIB:"'+$out+'\libxcb.lib"'),('/DEF:"'+$root+'\libxcb\src\libxcb.def"')) + $inputs + @('ws2_32.lib','advapi32.lib','user32.lib') | Set-Content $rsp
$linkCommand='call "'+$vc+'" x64 >nul && link @"'+$rsp+'"'
$command='call "'+$vc+'" x64 >nul && cl /nologo /W3 /D_CRT_SECURE_NO_WARNINGS /DWIN32 /D_WIN32_WINNT=0x0501 /DPTW32_STATIC_LIB /I"'+$root+'" /I"'+$root+'\include" /I"'+$root+'\pthreads" "'+$root+'\tests\native\winxcb-live-test.c" "'+$root+'\xorg-server\hw\xwin\winxcb.c" "'+$root+'\xorg-server\os\winlocal.c" /Fe:"'+$out+'\winxcb-live-test.exe" /link "'+$out+'\libxcb.lib" ws2_32.lib advapi32.lib bcrypt.lib'
Push-Location $out
try {
 & cmd.exe /d /s /c $linkCommand
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 & cmd.exe /d /s /c $command
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
 Copy-Item (Join-Path $XcbBuildRoot 'libXau/obj64/release/libXau.dll') $out
 & (Join-Path $out 'winxcb-live-test.exe')
 if($LASTEXITCODE -ne 0){exit $LASTEXITCODE}
}finally{Pop-Location}
