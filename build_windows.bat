@echo off
setlocal
cd /d "%~dp0"
set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
where node >nul 2>nul
if errorlevel 1 goto prerequisites
where cargo >nul 2>nul
if errorlevel 1 goto prerequisites
call npm ci
if errorlevel 1 goto fail
node scripts\licenses.mjs
if errorlevel 1 goto fail
call npm run tauri -- build
if errorlevel 1 goto fail
node scripts\package-portable.mjs
if errorlevel 1 goto fail
echo Portable: dist\FreeCut-Tauri\FreeCut.exe
echo Installer: src-tauri\target\release\bundle\nsis\
echo The portable executable requires the system WebView2 Runtime.
pause
exit /b 0
:prerequisites
echo Install Node.js, Rust MSVC, Visual Studio C++ build tools and WebView2.
echo See README.md, then reopen the terminal and retry.
pause
exit /b 1
:fail
echo Build failed. Check the error above.
pause
exit /b 1
