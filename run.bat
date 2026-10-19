@echo off
setlocal
cd /d "%~dp0"
if exist ".venv\Scripts\python.exe" goto check
where py >nul 2>nul
if errorlevel 1 goto python
py -3 -m venv .venv
if errorlevel 1 goto fail
goto install
:python
python -m venv .venv
if errorlevel 1 goto fail
:install
".venv\Scripts\python.exe" -m pip install -e .
if errorlevel 1 goto fail
goto run
:check
".venv\Scripts\python.exe" -c "import PySide6; import freecut" >nul 2>nul
if errorlevel 1 goto install
:run
start "" ".venv\Scripts\pythonw.exe" -m freecut %*
exit /b 0
:fail
echo.
echo FreeCut could not start. Please install Python 3.10+ and check the error above.
pause
exit /b 1
