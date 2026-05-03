@echo off
setlocal

cd /d "%~dp0"
set "GENERIC_CODER_PROJECT_DIR=%CD%"

set "HOST=127.0.0.1"
set "PORT=8765"
set "BASE_URL=http://%HOST%:%PORT%/"
for /f %%I in ('powershell -NoProfile -Command "[guid]::NewGuid().ToString('N')"') do set "GENERIC_CODER_PICKER_TOKEN=%%I"
set "OPEN_URL=%BASE_URL%#picker_token=%GENERIC_CODER_PICKER_TOKEN%"
set "EXE=target\debug\generic-coder.exe"

where cargo >nul 2>nul
if not errorlevel 1 (
    echo [Generic Coder] Starting from source with cargo run...
    start "Generic Coder Server" cmd /c "cargo run -- serve --host %HOST% --port %PORT%"
    call :wait_for_server
    exit /b %ERRORLEVEL%
)

if exist "%EXE%" (
    echo [Generic Coder] Starting compiled Rust binary...
    start "Generic Coder Server" "%EXE%" serve --host %HOST% --port %PORT%
    call :wait_for_server
    exit /b %ERRORLEVEL%
)

echo [Generic Coder] Neither Cargo nor "%EXE%" was found.
echo Install Rust from https://rustup.rs/ or build once with: cargo build
pause
exit /b 1

:wait_for_server
for /l %%I in (1,1,600) do (
    powershell -NoProfile -Command "try { $r = Invoke-WebRequest -UseBasicParsing '%BASE_URL%health' -TimeoutSec 2; if ($r.StatusCode -eq 200) { exit 0 } else { exit 1 } } catch { exit 1 }" >nul 2>nul
    if not errorlevel 1 (
        start "" "%OPEN_URL%"
        exit /b 0
    )
    timeout /t 1 /nobreak >nul
)

echo [Generic Coder] Server did not become ready in time.
exit /b 1
