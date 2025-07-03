@echo off
setlocal enabledelayedexpansion

REM This script ensures the Rust toolchain is available and then runs the setup application.

REM Check if cargo is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo Rust and Cargo are not installed. Please install them to continue.
    echo You can find installation instructions at: https://www.rust-lang.org/tools/install
    exit /b 1
)

REM check if fastly cli is installed
where fastly >nul 2>nul
if %errorlevel% neq 0 (
    echo Fastly CLI is not installed. Please install it to continue.
    echo You can find installation instructions at: https://developer.fastly.com/learning/tools/cli/
    echo You may be able to install it with: winget install Fastly.CLI
    exit /b 1
)

REM check if powershell is installed
where powershell >nul 2>nul
if %errorlevel% neq 0 (
    echo PowerShell is not installed, but is required for this script.
    exit /b 1
)

REM --- Configuration ---

REM Function to read a value from CONFIG.txt using PowerShell
:getConfig
for /f "delims=" %%i in ('powershell -Command "$v = (Get-Content -Path 'CONFIG.txt' -ErrorAction SilentlyContinue | Select-String -Pattern \"^%~1=\" | Select -First 1); if ($v) { $v.ToString().Split('=', 2)[1].Trim() }"') do set "%2=%%i"
goto :eof

REM Load config, prompt for missing values
if not exist "CONFIG.txt" (
    echo CONFIG.txt not found. Please create it.
    exit /b 1
)

call :getConfig NAME NAME
call :getConfig FASTLY_API_TOKEN FASTLY_API_TOKEN
call :getConfig STEGAWAVE_API_KEY STEGAWAVE_API_KEY

REM Check for NAME
if not defined NAME (
    set /p NAME_INPUT="Enter a name for your Fastly service: "
    powershell -Command "(Get-Content CONFIG.txt) | ForEach-Object { $_ -replace 'NAME=.*', 'NAME=!NAME_INPUT!' } | Set-Content CONFIG.txt"
    set "NAME=!NAME_INPUT!"
)

REM Check for FASTLY_API_TOKEN
if not defined FASTLY_API_TOKEN (
    set /p FASTLY_API_TOKEN_INPUT="Enter your Fastly API token: "
    powershell -Command "(Get-Content CONFIG.txt) | ForEach-Object { $_ -replace 'FASTLY_API_TOKEN=.*', 'FASTLY_API_TOKEN=!FASTLY_API_TOKEN_INPUT!' } | Set-Content CONFIG.txt"
    set "FASTLY_API_TOKEN=!FASTLY_API_TOKEN_INPUT!"
)
set "FASTLY_API_TOKEN=%FASTLY_API_TOKEN%"

REM Check for STEGAWAVE_API_KEY
if not defined STEGAWAVE_API_KEY (
    set /p STEGAWAVE_API_KEY_INPUT="Enter your StegaWave API key: "
    powershell -Command "(Get-Content CONFIG.txt) | ForEach-Object { $_ -replace 'STEGAWAVE_API_KEY=.*', 'STEGAWAVE_API_KEY=!STEGAWAVE_API_KEY_INPUT!' } | Set-Content CONFIG.txt"
    set "STEGAWAVE_API_KEY=!STEGAWAVE_API_KEY_INPUT!"
)

echo Configuration loaded.

REM --- Update Project Files ---

echo Updating project files with service name: %NAME%
powershell -Command "(Get-Content Cargo.toml) -replace '{NAME}', '%NAME%' | Set-Content Cargo.toml"
powershell -Command "(Get-Content fastly.toml) -replace '{NAME}', '%NAME%' | Set-Content fastly.toml"

REM --- Deploy to Fastly ---

echo Deploying to Fastly. This may take a few minutes...
fastly compute deploy --non-interactive

REM --- Populate KV Stores ---

echo Populating KV stores...

REM The service ID should now be in fastly.toml after deployment
for /f "tokens=1,* delims==" %%a in ('findstr /b "service_id" fastly.toml') do (
    set "SERVICE_ID=%%b"
)
set SERVICE_ID=%SERVICE_ID: =%
set SERVICE_ID=%SERVICE_ID:"=%

if not defined SERVICE_ID (
    echo Failed to get service ID from fastly.toml. Attempting to find it via service name.
    for /f "delims=" %%i in ('fastly service list --json ^| powershell -Command "$json = $input | ConvertFrom-Json; $service = $json | Where-Object { $_.name -eq '%NAME%' }; $service.id"') do set "SERVICE_ID=%%i"
)

if not defined SERVICE_ID (
    echo Failed to get service ID for service '%NAME%'. Please check the Fastly UI.
    exit /b 1
)
echo Service ID: %SERVICE_ID%

REM Get KV store IDs from the Fastly API
for /f "delims=" %%i in ('fastly kv-store list --service-id "%SERVICE_ID%" --json ^| powershell -Command "$json = $input | ConvertFrom-Json; $store = $json | Where-Object { $_.name -eq 'secrets' }; $store.id"') do set "SECRETS_STORE_ID=%%i"
for /f "delims=" %%i in ('fastly kv-store list --service-id "%SERVICE_ID%" --json ^| powershell -Command "$json = $input | ConvertFrom-Json; $store = $json | Where-Object { $_.name -eq 'api_keys' }; $store.id"') do set "API_KEYS_STORE_ID=%%i"
for /f "delims=" %%i in ('fastly kv-store list --service-id "%SERVICE_ID%" --json ^| powershell -Command "$json = $input | ConvertFrom-Json; $store = $json | Where-Object { $_.name -eq 'watermarking_config' }; $store.id"') do set "WATERMARKING_CONFIG_STORE_ID=%%i"


if not defined SECRETS_STORE_ID ( echo "Could not find 'secrets' KV store."; exit /b 1; )
if not defined API_KEYS_STORE_ID ( echo "Could not find 'api_keys' KV store."; exit /b 1; )
if not defined WATERMARKING_CONFIG_STORE_ID ( echo "Could not find 'watermarking_config' KV store."; exit /b 1; )

echo KV Stores found.

REM Populate 'secrets' store
echo Populating 'secrets' store...
for /f "delims=" %%i in ('powershell -Command "$bytes = New-Object byte[] 32; $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create(); $rng.GetBytes($bytes); -join ($bytes | ForEach-Object { $_.ToString('x2') })"') do set "SECRET_KEY_HEX=%%i"
fastly kv-store-entry create --store-id "%SECRETS_STORE_ID%" --key "SECRET_KEY_HEX" --value "%SECRET_KEY_HEX%" --quiet

REM Populate 'api_keys' store
echo Populating 'api_keys' store...
fastly kv-store-entry create --store-id "%API_KEYS_STORE_ID%" --key "service_api_key" --value "%STEGAWAVE_API_KEY%" --quiet

REM Populate 'watermarking_config' store
echo Populating 'watermarking_config' store...
call :getConfig FMP4_AAC_PROFILE FMP4_AAC_PROFILE
call :getConfig FMP4_SAMPLE_RATE FMP4_SAMPLE_RATE
call :getConfig FMP4_CHANNELS FMP4_CHANNELS
call :getConfig FMP4_TRACK_ID FMP4_TRACK_ID

fastly kv-store-entry create --store-id "%WATERMARKING_CONFIG_STORE_ID%" --key "FMP4_AAC_PROFILE" --value "%FMP4_AAC_PROFILE%" --quiet
fastly kv-store-entry create --store-id "%WATERMARKING_CONFIG_STORE_ID%" --key "FMP4_SAMPLE_RATE" --value "%FMP4_SAMPLE_RATE%" --quiet
fastly kv-store-entry create --store-id "%WATERMARKING_CONFIG_STORE_ID%" --key "FMP4_CHANNELS" --value "%FMP4_CHANNELS%" --quiet
fastly kv-store-entry create --store-id "%WATERMARKING_CONFIG_STORE_ID%" --key "FMP4_TRACK_ID" --value "%FMP4_TRACK_ID%" --quiet

echo Setup complete. Your service '%NAME%' is deployed and configured.

endlocal
