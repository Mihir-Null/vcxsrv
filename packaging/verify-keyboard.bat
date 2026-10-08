@echo off
setlocal
if "%~1"=="" exit /b 2
for %%F in ("%~1\xorg-server\xkbdata\rules\base" "%~1\xorg-server\xkbdata\symbols\us") do (
  if not exist "%%~fF" (
    echo Missing keyboard asset: "%%~fF" 1>&2
    exit /b 1
  )
  if %%~zF EQU 0 (
    echo Empty keyboard asset: "%%~fF" 1>&2
    exit /b 1
  )
)
exit /b 0
