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
  if not exist "UltraNetNode.env.example" (
    echo UltraNetNode.env and UltraNetNode.env.example were not found.
    echo Extract the complete UltraNet Windows package before starting the node.
    pause
    exit /b 1
  )

  echo First run: creating UltraNetNode.env from the safe template...
  copy /Y "UltraNetNode.env.example" "UltraNetNode.env" >nul
  if errorlevel 1 (
    echo Could not create UltraNetNode.env in this folder.
    echo Extract the package into a writable folder and try again.
    pause
    exit /b 1
  )

  echo Add a strong, randomly generated ULTRANET_ADMIN_TOKEN to UltraNetNode.env.
  echo Use 32 random bytes / 64 hex characters for security.
  echo Never reuse a wallet key, short password, or public value as the token.
  echo The node will not start while the template placeholder is present.
  echo Opening UltraNetNode.env in Notepad...
  start "" /wait notepad.exe "%~dp0UltraNetNode.env"
)

set "ULTRANET_ENV_FILE=%~dp0UltraNetNode.env"
rem The desktop package uses the sibling file as the source of truth. Clear any
rem stale inherited value so it cannot override the token just entered in Notepad.
set "ULTRANET_ADMIN_TOKEN="
set "ULTRANET_PAUSE_ON_ERROR=1"

findstr /C:"ULTRANET_ADMIN_TOKEN=replace-with-" "UltraNetNode.env" >nul 2>&1
if not errorlevel 1 (
  echo The template token is still present in UltraNetNode.env.
  echo Enter a fresh 64-character hexadecimal token, save the file, and continue.
  echo Opening UltraNetNode.env in Notepad...
  start "" /wait notepad.exe "%~dp0UltraNetNode.env"
)

echo Checking UltraNet configuration...
UltraNetNode.exe --check-config
set "CHECK_EXIT=%ERRORLEVEL%"
if not "%CHECK_EXIT%"=="0" exit /b %CHECK_EXIT%

echo Checking FHE initialization...
UltraNetNode.exe --check-fhe
set "FHE_CHECK_EXIT=%ERRORLEVEL%"
if not "%FHE_CHECK_EXIT%"=="0" exit /b %FHE_CHECK_EXIT%

echo Starting UltraNetNode. Press Ctrl+C to stop it.
UltraNetNode.exe
set "EXIT_CODE=%ERRORLEVEL%"
if not "%EXIT_CODE%"=="0" (
  echo UltraNetNode stopped with exit code %EXIT_CODE%.
  pause
)
exit /b %EXIT_CODE%
