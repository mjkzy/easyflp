@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

:: kill previous tasks (easyflp-gui.exe is the old GUI binary name)
taskkill /f /im easyflp-gui.exe >nul 2>&1
taskkill /f /im easyflp.exe >nul 2>&1

cargo build --release
if errorlevel 1 (
    echo cargo build failed.
    exit /b 1
)

:: quick launch the release gui
start "" "%~dp0target\release\easyflp.exe"
