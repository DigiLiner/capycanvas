@echo off
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "%~dp0apps\layer-windows\scripts\build.ps1" -Run %*
exit /b %errorlevel%
