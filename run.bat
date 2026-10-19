@echo off
setlocal
cd /d "%~dp0"
if exist "src-tauri\target\release\FreeCut.exe" (
    start "" "src-tauri\target\release\FreeCut.exe" %*
    exit /b 0
)
if exist "dist\FreeCut-Tauri\FreeCut.exe" (
    start "" "dist\FreeCut-Tauri\FreeCut.exe" %*
    exit /b 0
)
echo Build FreeCut with build_windows.bat first.
echo For development, run: npm run tauri -- dev
pause
exit /b 1
