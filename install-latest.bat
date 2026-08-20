@echo off
setlocal enableextensions

REM Download the latest jsonsh Windows x64 release from GitHub and
REM extract jsonsh.exe into the .\bin folder next to this script.

set "REPO=q5n/jsonsh"
set "ASSET=jsonsh-windows-x64.zip"
set "URL=https://github.com/%REPO%/releases/latest/download/%ASSET%"

set "SCRIPT_DIR=%~dp0"
set "BIN_DIR=%SCRIPT_DIR%bin"
set "ZIP_FILE=%TEMP%\%ASSET%"

echo Downloading latest %ASSET% ...
echo   %URL%
curl.exe -fL --retry 3 -o "%ZIP_FILE%" "%URL%"
if errorlevel 1 (
    echo [error] download failed 1>&2
    exit /b 1
)

if not exist "%BIN_DIR%" mkdir "%BIN_DIR%"

echo Extracting to %BIN_DIR% ...
if exist "%BIN_DIR%\jsonsh.exe" del /f /q "%BIN_DIR%\jsonsh.exe"

REM tar ships with Windows 10 1803+ and can read zip archives.
where tar.exe >nul 2>&1
if not errorlevel 1 (
    tar.exe -xf "%ZIP_FILE%" -C "%BIN_DIR%"
) else (
    powershell.exe -NoProfile -Command "Expand-Archive -LiteralPath '%ZIP_FILE%' -DestinationPath '%BIN_DIR%' -Force"
)
if errorlevel 1 (
    echo [error] extraction failed 1>&2
    exit /b 1
)

del /f /q "%ZIP_FILE%" 2>nul

echo.
echo Installed:
if exist "%BIN_DIR%\jsonsh.exe" (
    "%BIN_DIR%\jsonsh.exe" --version
) else (
    echo [warn] %BIN_DIR%\jsonsh.exe not found after extraction 1>&2
    exit /b 1
)

endlocal
