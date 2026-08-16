@echo off
setlocal EnableExtensions
cd /d "%~dp0"

if not exist "UltraNetNode.exe" (
  echo UltraNetNode.exe was not found in this folder.
  echo Extract the complete UltraNet Windows package before starting the node.
  pause
  exit /b 1
)

if not exist "UltraNetNode.env" (
  echo UltraNetNode.env was not found.
  echo Copy UltraNetNode.env.example to UltraNetNode.env, create a private admin token, and try again.
  pause
  exit /b 1
)

set "ULTRANET_ENV_FILE=%~dp0UltraNetNode.env"
set "ULTRANET_PAUSE_ON_ERROR=1"

echo Checking UltraNet configuration...
UltraNetNode.exe --check-config
set "CHECK_EXIT=%ERRORLEVEL%"
if not "%CHECK_EXIT%"=="0" exit /b %CHECK_EXIT%

echo Starting UltraNetNode. Press Ctrl+C to stop it.
UltraNetNode.exe
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo UltraNetNode stopped with exit code %EXIT_CODE%.
  pause
)
exit /b %EXIT_CODE%
