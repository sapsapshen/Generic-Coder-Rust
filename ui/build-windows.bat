@echo off
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
for %%i in ("%SCRIPT_DIR%\..") do set "PROJECT_DIR=%%~fi"
set "UI_DIR=%SCRIPT_DIR%"

echo ========================================
echo  Generic Coder — Windows NSIS Builder
echo ========================================
echo.

REM ── Check prerequisites ───────────────────────────────────────
where node >nul 2>&1 || (echo ERROR: Node.js not found. Install from https://nodejs.org & pause & exit /b 1)
where npm >nul 2>&1 || (echo ERROR: npm not found. & pause & exit /b 1)
where cargo >nul 2>&1 || (echo WARNING: Rust/Cargo not found. Backend binary will be missing. & pause)

REM ── Rebuild Rust backend ──────────────────────────────────────
if exist "%PROJECT_DIR%\Cargo.toml" (
    echo Building Rust backend...
    cd /d "%PROJECT_DIR%"
    cargo build --release
    echo   - Backend built

    REM Copy binary into electron-builder's reach
    if not exist "%UI_DIR%\bin" mkdir "%UI_DIR%\bin"
    copy /Y target\release\generic-coder.exe "%UI_DIR%\bin\generic-coder-backend.exe" >nul 2>&1
    echo   - Backend binary staged at ui/bin/
) else (
    echo WARNING: Rust project not found. Skipping backend build.
)

echo.

REM ── Install JS dependencies ───────────────────────────────────
echo Installing JS dependencies...
cd /d "%UI_DIR%"
call npm install
echo.

REM ── Build Electron app ────────────────────────────────────────
echo Building Windows .exe installer...
call npm run build:windows
echo.
echo ========================================
echo  Done!
echo  Installer output: ui/dist/
dir "%UI_DIR%\dist\*.exe" /b 2>nul || echo   (check dist/ for output)
echo ========================================
pause
