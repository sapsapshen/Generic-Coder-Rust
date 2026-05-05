@echo off
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
for %%i in ("%SCRIPT_DIR%\..") do set "PROJECT_DIR=%%~fi"

echo === Generic Coder TUI Launcher (Windows) ===
echo.

REM ── Build (release) ──────────────────────────────────────────────
echo Building TUI...
cd /d "%PROJECT_DIR%"
cargo build --release -p generic-coder-tui
echo.

set "BIN=%PROJECT_DIR%\target\release\generic-coder-tui.exe"

if not exist "%BIN%" (
    echo ERROR: Build failed — binary not found at %BIN%
    pause
    exit /b 1
)

REM ── Run ──────────────────────────────────────────────────────────
echo Launching Generic Coder TUI...
echo   Ctrl+Q  Quit
echo   F1      Work mode
echo   F2      Plan mode
echo   F3      Review mode
echo   Ctrl+S  Settings
echo   Ctrl+W  Toggle sidebar
echo.

"%BIN%" %*
pause
