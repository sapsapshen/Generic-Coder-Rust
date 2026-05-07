@echo off
setlocal

cd /d "%~dp0"
set "GENERIC_CODER_PROJECT_DIR=%CD%"

echo [Generic Coder] Launching desktop app...

cd ui
if not exist node_modules (
    echo Installing Electron dependencies...
    call npm install
    if errorlevel 1 exit /b %ERRORLEVEL%
)

cd ..
where cargo >nul 2>nul
if errorlevel 1 (
    if not exist "target\release\generic-coder.exe" (
        echo [Generic Coder] Cargo was not found and target\release\generic-coder.exe is missing.
        echo Install Rust from https://rustup.rs/ or build once with: cargo build --release
        exit /b 1
    )
) else (
    echo Building Rust backend...
    cargo build --release -q
    if errorlevel 1 exit /b %ERRORLEVEL%
)

cd ui
call npm start
exit /b %ERRORLEVEL%
