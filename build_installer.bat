@echo off
setlocal
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
cd /d "%~dp0"

cargo build --release
if errorlevel 1 (
    echo cargo build failed.
    exit /b 1
)

set "ISCC="
for %%P in (
    "%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe"
    "%ProgramFiles(x86)%\Inno Setup 6\ISCC.exe"
    "%ProgramFiles%\Inno Setup 6\ISCC.exe"
) do if not defined ISCC if exist %%P set "ISCC=%%~P"
if not defined ISCC (
    echo Inno Setup 6 not found. Install it from https://jrsoftware.org/isdl.php
    exit /b 1
)

"%ISCC%" /Q installer\easyflp.iss
if errorlevel 1 (
    echo installer build failed.
    exit /b 1
)
echo target\installer\easyflp-setup.exe
