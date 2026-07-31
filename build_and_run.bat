@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

:: kill previous tasks (easyFLP.exe is the pre-split binary name)
taskkill /f /im easyflp-gui.exe >nul 2>&1
taskkill /f /im easyFLP.exe >nul 2>&1

cargo build --release
if errorlevel 1 (
    echo cargo build failed.
    exit /b 1
)

:: quick launch the release gui
start "" "%~dp0target\release\easyflp-gui.exe"
