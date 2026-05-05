@echo off
setlocal enabledelayedexpansion

REM ── macOS build-script 问题清单 & Windows 对策 ──────────────────────
REM  1. $1 unbound  →  %~1 加双引号默认空 (无位置参数依赖)
REM  2. 缺少图标工具  →  ICO 已在 macOS 端预生成，跳过生成步骤
REM  3. python 缺失  →  electron-builder win 不用 python blockmap，跳过
REM  4. hdiutil 挂载冲突 → Windows NSIS 无此问题，但需清理僵尸 electron-builder
REM  5. 通用包命名  →  artifactName 已加 ${arch}，Windows 仅 x64
REM  6. assets 未打包 → 统一复制 *.txt *.json + skills 到 ui/assets/
REM  7. 二进制未暂存 →  统一复制 generic-coder.exe 到 ui/bin/
REM  8. 粘滞挂载清理  →  对应: 强制终止残留 electron-builder + NSIS 进程
REM  9. 顺序架构构建  →  Windows 仅 x64，无多架构竞态
REM 10. hdiutil convert  →  electron-builder 内部处理 NSIS/portable 输出
REM ──────────────────────────────────────────────────────────────────

set "SCRIPT_DIR=%~dp0"
set "SCRIPT_DIR=%SCRIPT_DIR:~0,-1%"
for %%i in ("%SCRIPT_DIR%\..") do set "PROJECT_DIR=%%~fi"
set "UI_DIR=%SCRIPT_DIR%"
set "RUST_TARGET=target\release\generic-coder.exe"

echo ========================================
echo  Generic Coder — Windows NSIS Builder
echo ========================================
echo.
echo  Project dir : %PROJECT_DIR%
echo  UI dir      : %UI_DIR%
echo.

REM ── 0. 清理上次构建僵尸进程 ────────────────────────────────────────
echo [0/5] Cleaning up stale build processes...
taskkill /f /im electron-builder.exe  >nul 2>&1
taskkill /f /im makensis.exe        >nul 2>&1
taskkill /f /im generic-coder-backend.exe >nul 2>&1
REM 清理可能残留的输出锁文件
if exist "%UI_DIR%\dist\.icon-ico"  del /f /q "%UI_DIR%\dist\.icon-ico"   >nul 2>&1
if exist "%UI_DIR%\dist\__appImage" rmdir /s /q "%UI_DIR%\dist\__appImage" >nul 2>&1
echo   Done.
echo.

REM ── 1. 检查必要工具 ────────────────────────────────────────────────
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

REM ── 2. 构建 Rust 后端 ──────────────────────────────────────────────
echo [2/5] Building Rust backend...

set "BIN_SRC=%PROJECT_DIR%\%RUST_TARGET%"
set "BIN_DST=%UI_DIR%\bin\generic-coder-backend.exe"

if exist "%PROJECT_DIR%\Cargo.toml" (
    pushd "%PROJECT_DIR%"
    cargo build --release 2>&1
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
        REM cargo 可能输出到不同位置，尝试查找
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

REM ── 3. 准备打包资源 ────────────────────────────────────────────────
echo [3/5] Staging assets for packaging...

REM Copy backend assets (*.txt, *.json)
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

REM Copy skills (overwrite target completely to avoid stale files)
set "SRC_SKILLS=%PROJECT_DIR%\skills"
set "DST_SKILLS=%DST_ASSETS%\skills"
if exist "%DST_SKILLS%" rmdir /s /q "%DST_SKILLS%" >nul 2>&1
if exist "%SRC_SKILLS%" (
    xcopy /e /i /q /y "%SRC_SKILLS%" "%DST_SKILLS%" >nul 2>&1
    echo   Skills staged
)

REM Verify staged binary
if not exist "%BIN_DST%" (
    echo   ERROR: Backend binary missing at %BIN_DST%
    pause
    exit /b 1
)
echo.

REM ── 4. 安装依赖并构建 Electron ─────────────────────────────────────
echo [4/5] Installing dependencies...
pushd "%UI_DIR%"
call npm install 2>&1
if errorlevel 1 (
    popd
    echo   ERROR: npm install failed.
    pause
    exit /b 1
)
echo   Done.
echo.
echo [5/5] Building Windows installers...
call npm run build:windows 2>&1
if errorlevel 1 (
    popd
    echo   ERROR: electron-builder failed. See output above.
    pause
    exit /b 1
)
popd
echo.

REM ── 验证输出 ────────────────────────────────────────────────────────
echo ========================================
echo  Build complete!
echo ========================================
echo.
echo  Output files:
for %%f in ("%UI_DIR%\dist\Generic Coder-"*.exe "%UI_DIR%\dist\Generic Coder-"*.zip 2>nul) do (
    echo   %%~nxf  (%%~zf bytes)
)
if not exist "%UI_DIR%\dist\Generic Coder-*" (
    echo   (no matching files found in dist/)
    dir "%UI_DIR%\dist\" /b 2>nul
)
echo.
echo  Expected: Generic Coder-1.0.0-x64.exe (NSIS installer^)
echo            Generic Coder-1.0.0-x64.zip (portable^)
echo ========================================
pause
endlocal
