@echo off
setlocal enabledelayedexpansion

set "SCRIPT_DIR=%~dp0"
set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"
for %%i in ("%SCRIPT_DIR%\..") do set "PROJECT_DIR=%%~fi"
set "UI_DIR=%SCRIPT_DIR%"
set "RUST_TARGET=target\release\generic-coder.exe"

echo ========================================
echo  Generic Coder - Windows Installer Builder
echo ========================================
echo.
echo  Project dir : %PROJECT_DIR%
echo  UI dir      : %UI_DIR%
echo.

REM -- 0. Cleanup stale build processes ----------------------------------
echo [0/5] Cleaning up stale build processes...
taskkill /f /im electron-builder.exe  >nul 2>&1
taskkill /f /im makensis.exe        >nul 2>&1
taskkill /f /im generic-coder-backend.exe >nul 2>&1
if exist "%UI_DIR%\dist\.icon-ico"  del /f /q "%UI_DIR%\dist\.icon-ico"   >nul 2>&1
if exist "%UI_DIR%\dist\__appImage" rmdir /s /q "%UI_DIR%\dist\__appImage" >nul 2>&1
echo   Done.
echo.

REM -- 1. Check prerequisites -------------------------------------------
echo [1/5] Checking prerequisites...

where node >nul 2>&1 || (
    echo   ERROR: Node.js not found. Download from https://nodejs.org
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('node -v') do echo   Node.js : %%v

where npm >nul 2>&1 || (
    echo   ERROR: npm not found.
    pause
    exit /b 1
)
for /f "tokens=*" %%v in ('npm -v') do echo   npm     : %%v

where cargo >nul 2>&1 && (
    for /f "tokens=*" %%v in ('cargo -V') do echo   Cargo   : %%v
) || (
    echo   WARNING: Rust/Cargo not found. Backend binary will NOT be built.
    echo   Install from https://rustup.rs then re-run.
)
echo.

REM -- 2. Build Rust backend --------------------------------------------
echo [2/5] Building Rust backend...

set "BIN_SRC=%PROJECT_DIR%\%RUST_TARGET%"
set "BIN_DST=%UI_DIR%\bin\generic-coder-backend.exe"

if exist "%PROJECT_DIR%\Cargo.toml" (
    pushd "%PROJECT_DIR%"
    cargo build --release -j 1 2>&1
    if errorlevel 1 (
        popd
        echo   ERROR: Rust build failed. Check compiler output above.
        pause
        exit /b 1
    )
    popd

    if exist "%BIN_SRC%" (
        if not exist "%UI_DIR%\bin" mkdir "%UI_DIR%\bin"
        copy /y "%BIN_SRC%" "%BIN_DST%" >nul 2>&1
        for %%A in ("%BIN_DST%") do echo   Backend binary : %%A (%%~zA bytes)
    ) else (
        for /r "%PROJECT_DIR%\target\release" %%f in (generic-coder.exe) do (
            if not exist "%UI_DIR%\bin" mkdir "%UI_DIR%\bin"
            copy /y "%%f" "%BIN_DST%" >nul 2>&1
            echo   Backend binary : %%f
        )
    )
) else (
    echo   WARNING: No Cargo.toml found. Skipping backend build.
    echo   The binary must already exist at ui/bin/generic-coder-backend.exe
    if not exist "%BIN_DST%" (
        echo   ERROR: ui/bin/generic-coder-backend.exe not found.
        echo   Build the Rust backend first or place the binary manually.
        pause
        exit /b 1
    )
)
echo.

REM -- 3. Stage assets --------------------------------------------------
echo [3/5] Staging assets for packaging...

set "SRC_ASSETS=%PROJECT_DIR%\assets"
set "DST_ASSETS=%UI_DIR%\assets"
if not exist "%DST_ASSETS%" mkdir "%DST_ASSETS%"

if exist "%SRC_ASSETS%\*.txt" (
    copy /y "%SRC_ASSETS%\*.txt" "%DST_ASSETS%\" >nul 2>&1
    echo   *.txt assets copied
)
if exist "%SRC_ASSETS%\*.json" (
    copy /y "%SRC_ASSETS%\*.json" "%DST_ASSETS%\" >nul 2>&1
    echo   *.json assets copied
)

set "SRC_SKILLS=%PROJECT_DIR%\skills"
set "DST_SKILLS=%DST_ASSETS%\skills"
if exist "%DST_SKILLS%" rmdir /s /q "%DST_SKILLS%" >nul 2>&1
if exist "%SRC_SKILLS%" (
    xcopy /e /i /q /y "%SRC_SKILLS%" "%DST_SKILLS%" >nul 2>&1
    echo   Skills staged
)

if not exist "%BIN_DST%" (
    echo   ERROR: Backend binary missing at %BIN_DST%
    pause
    exit /b 1
)
echo.

REM -- 4-5. Install deps and build Electron -----------------------------
echo [4/5] Installing dependencies...
pushd "%UI_DIR%"

set ELECTRON_MIRROR=https://npmmirror.com/mirrors/electron/
set ELECTRON_CUSTOM_DIR=v33.4.11

call npm install 2>&1
if errorlevel 1 (
    popd
    echo   ERROR: npm install failed.
    pause
    exit /b 1
)
echo   Done.
echo.
echo [5/5] Building Windows installer...
for /f "delims=" %%v in ('node scripts\resolve-app-version.cjs') do set "APP_VERSION=%%v"
echo   App version : %APP_VERSION%
set ELECTRON_MIRROR=https://npmmirror.com/mirrors/electron/
set ELECTRON_CUSTOM_DIR=v33.4.11
set ELECTRON_BUILDER_BINARIES_MIRROR=https://npmmirror.com/mirrors/electron-builder-binaries/
if exist "%UI_DIR%\dist\win-unpacked" rmdir /s /q "%UI_DIR%\dist\win-unpacked"
del /f /q "%UI_DIR%\dist\*.blockmap" >nul 2>&1
del /f /q "%UI_DIR%\dist\*.yml" >nul 2>&1
del /f /q "%UI_DIR%\dist\*.yaml" >nul 2>&1
del /f /q "%UI_DIR%\dist\*portable*.exe" >nul 2>&1
call npm run build:windows 2>&1
if errorlevel 1 (
    popd
    echo   ERROR: electron-builder failed. See output above.
    pause
    exit /b 1
)
popd
echo.

if exist "%UI_DIR%\dist\win-unpacked" rmdir /s /q "%UI_DIR%\dist\win-unpacked"
del /f /q "%UI_DIR%\dist\*portable*.exe" >nul 2>&1

REM -- Verify output ----------------------------------------------------
echo ========================================
echo  Build complete!
echo ========================================
echo.
echo  Installer file:
for %%f in ("%UI_DIR%\dist\Generic Coder-"*-installer.exe) do (
    echo   %%~nxf  (%%~zf bytes)
)
echo.
    echo  Expected: Generic Coder-%APP_VERSION%-x64-installer.exe (NSIS installer)

echo ========================================
pause
endlocal
