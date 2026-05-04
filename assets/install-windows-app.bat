@echo off
setlocal EnableExtensions EnableDelayedExpansion
title Generic Coder Installer
color 0A

set "EMBEDDED_MODE=0"
for %%A in (%*) do (
    if /I "%%~A"=="--embedded" set "EMBEDDED_MODE=1"
)

set "APP_NAME=Generic Coder"
set "SCRIPT_DIR=%~dp0"
for %%I in ("%SCRIPT_DIR%..") do set "PROJECT_ROOT=%%~fI"
set "INSTALL_ROOT=%LocalAppData%\Programs\%APP_NAME%"
set "APP_DIR=%INSTALL_ROOT%\app"
set "LAUNCHER_BAT=%INSTALL_ROOT%\Launch Generic Coder.bat"
set "UNINSTALL_BAT=%INSTALL_ROOT%\Uninstall Generic Coder.bat"
set "DESKTOP_SHORTCUT=%USERPROFILE%\Desktop\%APP_NAME%.lnk"
set "STARTMENU_DIR=%AppData%\Microsoft\Windows\Start Menu\Programs\%APP_NAME%"

echo.
echo ========================================
echo        Generic Coder Installer
echo ========================================
echo.

if /I not "%OS%"=="Windows_NT" (
    echo [x] This installer only supports Windows.
    call :finish_fail 1
)

call :ensure_python || exit /b 1
call :resolve_python || exit /b 1

echo [*] Installing into:
echo     %INSTALL_ROOT%
echo.

if exist "%INSTALL_ROOT%" (
    echo [*] Existing installation found. It will be updated.
)

mkdir "%INSTALL_ROOT%" >nul 2>&1
mkdir "%APP_DIR%" >nul 2>&1

echo [*] Copying application files...
robocopy "%PROJECT_ROOT%" "%APP_DIR%" /MIR /NFL /NDL /NJH /NJS /NP /XD ".git" "build" "dist" ".venv" "__pycache__" ".pytest_cache" "temp"
set "ROBOCODE=%errorlevel%"
if %ROBOCODE% GEQ 8 (
    echo [x] File copy failed. robocopy exit code: %ROBOCODE%
    call :finish_fail %ROBOCODE%
)

if not exist "%APP_DIR%\mykey.py" if exist "%APP_DIR%\mykey_template.py" (
    copy /Y "%APP_DIR%\mykey_template.py" "%APP_DIR%\mykey.py" >nul
    echo [*] Created mykey.py from template. Please fill in your API settings before first use.
)

echo [*] Installing runtime dependencies...
pushd "%APP_DIR%"
call "%PYTHON_CMD%" -m pip install -e ".[ui,media,remote,workspace]"
if errorlevel 1 (
    popd
    echo [x] Dependency installation failed.
    call :finish_fail 1
)
popd

echo [*] Creating launcher...
> "%LAUNCHER_BAT%" echo @echo off
>> "%LAUNCHER_BAT%" echo cd /d "%APP_DIR%"
>> "%LAUNCHER_BAT%" echo where pyw ^>nul 2^>^&1 ^&^& start "" pyw -3 launch.pyw ^& exit /b 0
>> "%LAUNCHER_BAT%" echo where pythonw ^>nul 2^>^&1 ^&^& start "" pythonw launch.pyw ^& exit /b 0
>> "%LAUNCHER_BAT%" echo start "" %PYTHON_CMD% launch.pyw

echo [*] Creating shortcuts...
mkdir "%STARTMENU_DIR%" >nul 2>&1
powershell -NoProfile -ExecutionPolicy Bypass -Command "$ws = New-Object -ComObject WScript.Shell; $desktop = $ws.CreateShortcut('%DESKTOP_SHORTCUT%'); $desktop.TargetPath = '%LAUNCHER_BAT%'; $desktop.WorkingDirectory = '%INSTALL_ROOT%'; $desktop.IconLocation = '%SystemRoot%\\System32\\shell32.dll,220'; $desktop.Save(); $menu = $ws.CreateShortcut('%STARTMENU_DIR%\\%APP_NAME%.lnk'); $menu.TargetPath = '%LAUNCHER_BAT%'; $menu.WorkingDirectory = '%INSTALL_ROOT%'; $menu.IconLocation = '%SystemRoot%\\System32\\shell32.dll,220'; $menu.Save()"
if errorlevel 1 (
    echo [x] Failed to create shortcuts.
    call :finish_fail 1
)

echo [*] Creating uninstaller...
> "%UNINSTALL_BAT%" echo @echo off
>> "%UNINSTALL_BAT%" echo setlocal
>> "%UNINSTALL_BAT%" echo del /f /q "%DESKTOP_SHORTCUT%" ^>nul 2^>^&1
>> "%UNINSTALL_BAT%" echo rmdir /s /q "%STARTMENU_DIR%" ^>nul 2^>^&1
>> "%UNINSTALL_BAT%" echo rmdir /s /q "%INSTALL_ROOT%"
>> "%UNINSTALL_BAT%" echo echo Generic Coder has been removed.
>> "%UNINSTALL_BAT%" echo pause

echo.
echo [+] Installation complete.
echo [*] Desktop shortcut: %DESKTOP_SHORTCUT%
echo [*] Start menu folder: %STARTMENU_DIR%
echo [*] Uninstaller: %UNINSTALL_BAT%
echo.
echo [!] If this is your first install, open %APP_DIR%\mykey.py and fill in your API settings.
echo.
if "%EMBEDDED_MODE%"=="1" exit /b 0

choice /C YN /N /M "Launch Generic Coder now? [Y/N] "
if errorlevel 2 goto :end
start "" "%LAUNCHER_BAT%"

:end
echo.
pause
exit /b 0

:finish_fail
set "EXIT_CODE=%~1"
if "%EMBEDDED_MODE%"=="0" (
    echo.
    pause
)
exit /b %EXIT_CODE%

:ensure_python
where py >nul 2>nul
if not errorlevel 1 exit /b 0
where python >nul 2>nul
if not errorlevel 1 exit /b 0
echo [!] Python 3 was not found. Running bundled Python installer...
call "%SCRIPT_DIR%install_python_windows.bat"
if errorlevel 1 exit /b 1
exit /b 0

:resolve_python
where py >nul 2>nul
if not errorlevel 1 (
    set "PYTHON_CMD=py -3"
    exit /b 0
)
where python >nul 2>nul
if not errorlevel 1 (
    set "PYTHON_CMD=python"
    exit /b 0
)
echo [x] Python is still unavailable after installation.
exit /b 1