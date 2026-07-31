@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

cargo build --release
if errorlevel 1 (
    echo BUILD FAILED
    exit /b 1
)