@echo off
setlocal

set "APP_EXE=%~dp0src-tauri\target\release\tell-you-why.exe"

if not exist "%APP_EXE%" (
  echo Tell You Why Release 程序尚未构建。
  echo.
  echo 请先在项目目录运行：
  echo npm.cmd run tauri -- build
  echo.
  pause
  exit /b 1
)

start "" "%APP_EXE%"
exit /b 0
