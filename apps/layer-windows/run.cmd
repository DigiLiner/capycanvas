@echo off
powershell.exe -NoProfile -ExecutionPolicy RemoteSigned -File "%~dp0scripts\build.ps1" -Run %*
exit /b %errorlevel%
