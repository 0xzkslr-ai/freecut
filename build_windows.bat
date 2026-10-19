@echo off
setlocal
cd /d "%~dp0"
if not exist ".venv\Scripts\python.exe" (
    echo Run run.bat first to create the Python environment.
    pause
    exit /b 1
)
".venv\Scripts\python.exe" -m pip install -e ".[bundle]"
if errorlevel 1 goto fail
".venv\Scripts\python.exe" -m PyInstaller --noconfirm --clean --windowed --onedir --name FreeCut --paths src src/freecut/__main__.py
if errorlevel 1 goto fail
copy /Y LICENSE dist\FreeCut\LICENSE.txt >nul
copy /Y THIRD_PARTY_NOTICES.md dist\FreeCut\THIRD_PARTY_NOTICES.md >nul
echo Built: dist\FreeCut\FreeCut.exe
echo Distribute the entire dist\FreeCut directory, including Qt libraries and licenses.
pause
exit /b 0
:fail
echo Build failed. Check the error above.
pause
exit /b 1
