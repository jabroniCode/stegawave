@echo off
setlocal enabledelayedexpansion

REM This script ensures the Rust toolchain is available and then runs the setup application.

REM --- Restore Logic ---
if "%1"=="restore" (
    echo Restoring project to its original state...

    REM Get the current service name from fastly.toml and Cargo.toml
    for /f "tokens=2 delims==" %%a in ('findstr "^name" fastly.toml 2^>nul') do (
        set "CURRENT_NAME_FASTLY=%%a"
        set "CURRENT_NAME_FASTLY=!CURRENT_NAME_FASTLY: =!"
        set "CURRENT_NAME_FASTLY=!CURRENT_NAME_FASTLY:"=!"
    )
    for /f "tokens=2 delims==" %%a in ('findstr "^name" Cargo.toml 2^>nul') do (
        set "CURRENT_NAME_CARGO=%%a"
        set "CURRENT_NAME_CARGO=!CURRENT_NAME_CARGO: =!"
        set "CURRENT_NAME_CARGO=!CURRENT_NAME_CARGO:"=!"
    )

    REM Check if both files are already in template state
    if "!CURRENT_NAME_FASTLY!"=="{NAME}" if "!CURRENT_NAME_CARGO!"=="{NAME}" (
        echo Project already seems to be in its original state.
        exit /b 0
    )

    REM Use the name from whichever file has a real name
    if not "!CURRENT_NAME_FASTLY!"=="{NAME}" if not "!CURRENT_NAME_FASTLY!"=="" (
        set "CURRENT_NAME=!CURRENT_NAME_FASTLY!"
    ) else if not "!CURRENT_NAME_CARGO!"=="{NAME}" if not "!CURRENT_NAME_CARGO!"=="" (
        set "CURRENT_NAME=!CURRENT_NAME_CARGO!"
    ) else (
        echo Could not determine current service name.
        exit /b 1
    )

    echo Reverting files for service: !CURRENT_NAME!

    REM Restore fastly.toml
    powershell -Command "(Get-Content fastly.toml) -replace 'name = \"!CURRENT_NAME!\"', 'name = \"{NAME}\"' | Set-Content fastly.toml"
    powershell -Command "(Get-Content fastly.toml) -replace 'build = \"cargo build --bin !CURRENT_NAME! --release --target wasm32-wasip1 --color always\"', 'build = \"cargo build --bin {NAME} --release --target wasm32-wasip1 --color always\"' | Set-Content fastly.toml"
    powershell -Command "(Get-Content fastly.toml) -replace 'service_id = \".*\"', 'service_id = \"\"' | Set-Content fastly.toml"
    powershell -Command "(Get-Content fastly.toml) -replace 'address = \".*\"', 'address = \"{ORIGIN_1}\"' | Set-Content fastly.toml"

    REM Restore Cargo.toml
    powershell -Command "(Get-Content Cargo.toml) -replace 'name = \"!CURRENT_NAME!\"', 'name = \"{NAME}\"' | Set-Content Cargo.toml"

    REM Restore CONFIG.txt
    powershell -Command "(Get-Content CONFIG.txt) -replace 'NAME=.*', 'NAME=' | Set-Content CONFIG.txt"
    powershell -Command "(Get-Content CONFIG.txt) -replace 'FASTLY_API_TOKEN=.*', 'FASTLY_API_TOKEN=' | Set-Content CONFIG.txt"
    powershell -Command "(Get-Content CONFIG.txt) -replace 'STEGAWAVE_API_KEY=.*', 'STEGAWAVE_API_KEY=' | Set-Content CONFIG.txt"
    powershell -Command "(Get-Content CONFIG.txt) -replace 'ORIGIN_1=.*', 'ORIGIN_1=' | Set-Content CONFIG.txt"

    echo Project files have been restored.

    REM Ask to delete the service
    for /f "delims=" %%a in ('fastly service list --json 2^>nul') do set "SERVICE_LIST_JSON=%%a"
    if not "!SERVICE_LIST_JSON!"=="" if not "!SERVICE_LIST_JSON!"=="null" (
        for /f "delims=" %%a in ('echo !SERVICE_LIST_JSON! ^| jq -r ".[] | select(.Name == \"!CURRENT_NAME!\") | .ServiceID" 2^>nul') do set "SERVICE_ID=%%a"
        if not "!SERVICE_ID!"=="" (
            set /p "REPLY=Do you want to delete the Fastly service '!CURRENT_NAME!' (ID: !SERVICE_ID!)? [y/N] "
            if /i "!REPLY!"=="y" (
                echo Deleting Fastly service...
                fastly service delete --service-id "!SERVICE_ID!" --force >nul 2>&1
                echo Service deleted.
            )
        )
    )

    REM Remove directories and files
    if exist "pkg" (
        echo Removing pkg directory...
        rmdir /s /q "pkg"
    )
    if exist "bin" (
        echo Removing bin directory...
        rmdir /s /q "bin"
    )
    if exist "Cargo.lock" (
        echo Removing Cargo.lock...
        del "Cargo.lock"
    )
    if exist "target" (
        echo Removing target directory...
        rmdir /s /q "target"
    )

    echo Restore complete.
    exit /b 0
)

REM --- Tail Logic ---
if "%1"=="tail" (
    echo Starting log tail for service...

    REM Load config
    if not exist CONFIG.txt (
        echo CONFIG.txt not found. Please run setup first.
        exit /b 1
    )

    for /f "tokens=2 delims==" %%a in ('findstr "^NAME=" CONFIG.txt 2^>nul') do (
        set "NAME=%%a"
        REM Remove comments and trim whitespace
        for /f "tokens=1 delims=#" %%b in ("!NAME!") do set "NAME=%%b"
        set "NAME=!NAME: =!"
    )

    if "!NAME!"=="" (
        echo No service name found in CONFIG.txt. Please run setup first.
        exit /b 1
    )

    REM Load Fastly API token
    for /f "tokens=2 delims==" %%a in ('findstr "^FASTLY_API_TOKEN=" CONFIG.txt 2^>nul') do (
        set "FASTLY_API_TOKEN=%%a"
        for /f "tokens=1 delims=#" %%b in ("!FASTLY_API_TOKEN!") do set "FASTLY_API_TOKEN=%%b"
        set "FASTLY_API_TOKEN=!FASTLY_API_TOKEN: =!"
    )

    if "!FASTLY_API_TOKEN!"=="" (
        if not "!FASTLY_API_TOKEN!"=="" (
            REM Use environment variable
        ) else (
            echo No Fastly API token found. Please run setup first or set FASTLY_API_TOKEN environment variable.
            exit /b 1
        )
    )

    echo Tailing logs for service: !NAME!
    fastly log-tail --service-name "!NAME!"
    exit /b 0
)

REM Check if cargo is installed
cargo --version >nul 2>&1
if errorlevel 1 (
    echo Rust and Cargo are not installed. Please install them to continue.
    echo You can find installation instructions at: https://www.rust-lang.org/tools/install
    exit /b 1
)

REM Check if fastly cli is installed
fastly --version >nul 2>&1
if errorlevel 1 (
    echo Fastly CLI is not installed. Please install it to continue.
    echo You can find installation instructions at: https://developer.fastly.com/learning/tools/cli/
    exit /b 1
)

REM Check if jq is installed
jq --version >nul 2>&1
if errorlevel 1 (
    echo jq is not installed, but is required for this script. Please install it to continue.
    echo You can download it from: https://stedolan.github.io/jq/download/
    exit /b 1
)

REM Check if curl is installed
curl --version >nul 2>&1
if errorlevel 1 (
    echo curl is not installed, but is required for this script. Please install it to continue.
    exit /b 1
)

REM --- Configuration ---

REM Load config, prompt for missing values
if not exist CONFIG.txt (
    echo CONFIG.txt not found. Please create it.
    exit /b 1
)

REM Function to get config value
call :get_config NAME NAME
call :get_config FASTLY_API_TOKEN FASTLY_API_TOKEN
call :get_config STEGAWAVE_API_KEY STEGAWAVE_API_KEY
call :get_config ORIGIN_1 ORIGIN_1

REM Check for NAME
if "!NAME!"=="" (
    set /p "NAME_INPUT=Enter a name for your Fastly service: "
    powershell -Command "(Get-Content CONFIG.txt) -replace 'NAME=.*', 'NAME=!NAME_INPUT!' | Set-Content CONFIG.txt"
    set "NAME=!NAME_INPUT!"
)

REM Check for FASTLY_API_TOKEN
if "!FASTLY_API_TOKEN!"=="" (
    set /p "FASTLY_API_TOKEN_INPUT=Enter your Fastly API token: "
    powershell -Command "(Get-Content CONFIG.txt) -replace 'FASTLY_API_TOKEN=.*', 'FASTLY_API_TOKEN=!FASTLY_API_TOKEN_INPUT!' | Set-Content CONFIG.txt"
    set "FASTLY_API_TOKEN=!FASTLY_API_TOKEN_INPUT!"
)

REM Check for STEGAWAVE_API_KEY
if "!STEGAWAVE_API_KEY!"=="" (
    set /p "STEGAWAVE_API_KEY_INPUT=Enter your StegaWave API key: "
    powershell -Command "(Get-Content CONFIG.txt) -replace 'STEGAWAVE_API_KEY=.*', 'STEGAWAVE_API_KEY=!STEGAWAVE_API_KEY_INPUT!' | Set-Content CONFIG.txt"
    set "STEGAWAVE_API_KEY=!STEGAWAVE_API_KEY_INPUT!"
)

REM Check for ORIGIN_1
if "!ORIGIN_1!"=="" (
    set /p "ORIGIN_1_INPUT=Enter your origin server domain: "
    powershell -Command "(Get-Content CONFIG.txt) -replace 'ORIGIN_1=.*', 'ORIGIN_1=!ORIGIN_1_INPUT!' | Set-Content CONFIG.txt"
    set "ORIGIN_1=!ORIGIN_1_INPUT!"
)

echo Configuration loaded.

REM --- Update Project Files ---

echo Updating project files...
powershell -Command "(Get-Content Cargo.toml) -replace '{NAME}', '!NAME!' | Set-Content Cargo.toml"
powershell -Command "(Get-Content fastly.toml) -replace '{NAME}', '!NAME!' | Set-Content fastly.toml"
powershell -Command "(Get-Content fastly.toml) -replace '{ORIGIN_1}', '!ORIGIN_1!' | Set-Content fastly.toml"

REM --- Build and Deploy to Fastly ---

echo Building project...
fastly compute build >nul 2>&1
echo ✓ Build complete

REM --- Create Fastly Service and Resources ---

echo Checking for existing service...
for /f "delims=" %%a in ('fastly service list --json 2^>nul') do set "SERVICE_LIST_JSON=%%a"
if "!SERVICE_LIST_JSON!"=="" set "SERVICE_LIST_JSON=null"

if not "!SERVICE_LIST_JSON!"=="null" (
    for /f "delims=" %%a in ('echo !SERVICE_LIST_JSON! ^| jq -r "if type == \"array\" then .[] | select(.Name == \"!NAME!\") | .ServiceID else .data[]? | select(.Name == \"!NAME!\") | .ServiceID end // empty" 2^>nul') do set "SERVICE_ID=%%a"
) else (
    set "SERVICE_ID="
)

if "!SERVICE_ID!"=="" (
    echo Creating Fastly service...
    
    for /f "delims=" %%a in ('fastly service create --name "!NAME!" --type wasm 2^>^&1') do set "CREATE_SERVICE_OUTPUT=%%a"
    echo !CREATE_SERVICE_OUTPUT! | findstr "already taken" >nul
    if errorlevel 1 (
        REM Service created successfully
        for /f "tokens=4" %%a in ("!CREATE_SERVICE_OUTPUT!") do set "SERVICE_ID=%%a"
        echo ✓ Service created with ID: !SERVICE_ID!
    ) else (
        REM Service name already taken
        echo Service '!NAME!' already exists. Finding existing service ID...
        
        REM Try multiple times to find the service
        for /l %%i in (1,1,5) do (
            timeout /t 2 /nobreak >nul 2>&1
            for /f "delims=" %%a in ('fastly service list --json 2^>nul') do set "SERVICE_LIST_JSON=%%a"
            if not "!SERVICE_LIST_JSON!"=="" if not "!SERVICE_LIST_JSON!"=="null" (
                for /f "delims=" %%a in ('echo !SERVICE_LIST_JSON! ^| jq -r "if type == \"array\" then .[] | select(.Name == \"!NAME!\") | .ServiceID else .data[]? | select(.Name == \"!NAME!\") | .ServiceID end // empty" 2^>nul') do set "SERVICE_ID=%%a"
                if not "!SERVICE_ID!"=="" (
                    echo ✓ Found existing service with ID: !SERVICE_ID!
                    goto :service_found
                )
            )
        )
        
        echo ERROR: Could not find existing service ID after 5 attempts.
        echo Please wait a few minutes and try again, or check your Fastly dashboard.
        exit /b 1
        
        :service_found
    )

    REM Add backends and KV stores
    echo Setting up service resources...
    fastly backend create --service-id "!SERVICE_ID!" --version latest --name "origin_1" --address "!ORIGIN_1!" --port 443 >nul 2>&1
    fastly backend create --service-id "!SERVICE_ID!" --version latest --name "origin_2" --address "api.stegawave.com" --port 443 >nul 2>&1
    fastly kv-store create --name "secrets" >nul 2>&1
    fastly kv-store create --name "api_keys" >nul 2>&1
    fastly kv-store create --name "watermarking_config" >nul 2>&1
    echo ✓ Service resources configured
) else (
    echo ✓ Using existing service with ID: !SERVICE_ID!
    
    REM Ensure KV stores exist
    fastly kv-store create --name "secrets" >nul 2>&1
    fastly kv-store create --name "api_keys" >nul 2>&1
    fastly kv-store create --name "watermarking_config" >nul 2>&1
)

REM Update fastly.toml with the service ID
powershell -Command "(Get-Content fastly.toml) -replace 'service_id = .*', 'service_id = \"!SERVICE_ID!\"' | Set-Content fastly.toml"

echo Deploying to Fastly...
fastly compute deploy --service-id "!SERVICE_ID!" --non-interactive >nul 2>&1
echo ✓ Deployment complete

REM --- Populate KV Stores ---

echo Configuring KV stores...

REM Get the service ID from fastly.toml
for /f "tokens=2 delims==" %%a in ('findstr "^service_id" fastly.toml 2^>nul') do (
    set "SERVICE_ID=%%a"
    set "SERVICE_ID=!SERVICE_ID: =!"
    set "SERVICE_ID=!SERVICE_ID:"=!"
)

if "!SERVICE_ID!"=="" (
    for /f "delims=" %%a in ('fastly service list --json 2^>nul') do set "SERVICE_LIST_JSON=%%a"
    if not "!SERVICE_LIST_JSON!"=="" if not "!SERVICE_LIST_JSON!"=="null" (
        for /f "delims=" %%a in ('echo !SERVICE_LIST_JSON! ^| jq -r ".[] | select(.Name == \"!NAME!\") | .ServiceID" 2^>nul') do set "SERVICE_ID=%%a"
    )
)

if "!SERVICE_ID!"=="" (
    echo ERROR: Failed to get service ID for service '!NAME!'. Please check the Fastly UI.
    exit /b 1
)

REM Get KV store IDs
for /f "delims=" %%a in ('fastly kv-store list --json 2^>nul') do set "KV_STORE_LIST_JSON=%%a"
if "!KV_STORE_LIST_JSON!"=="" set "KV_STORE_LIST_JSON=null"

if "!KV_STORE_LIST_JSON!"=="null" (
    echo ERROR: Failed to list KV stores. Please check your permissions.
    exit /b 1
)

REM Get KV store IDs
call :get_kv_store_id "secrets" "!KV_STORE_LIST_JSON!" SECRETS_STORE_ID
call :get_kv_store_id "api_keys" "!KV_STORE_LIST_JSON!" API_KEYS_STORE_ID
call :get_kv_store_id "watermarking_config" "!KV_STORE_LIST_JSON!" WATERMARKING_CONFIG_STORE_ID

REM Link KV stores to service
for /f "delims=" %%a in ('fastly service-version list --service-id "!SERVICE_ID!" --json 2^>nul') do set "SERVICE_VERSIONS=%%a"
if "!SERVICE_VERSIONS!"=="" set "SERVICE_VERSIONS=[]"

for /f "delims=" %%a in ('echo !SERVICE_VERSIONS! ^| jq -r ".[] | select(.Active == true) | .Number // empty" 2^>nul') do set "CURRENT_VERSION=%%a"
if "!CURRENT_VERSION!"=="" (
    for /f "delims=" %%a in ('echo !SERVICE_VERSIONS! ^| jq -r "map(.Number) | max" 2^>nul') do set "CURRENT_VERSION=%%a"
)
if "!CURRENT_VERSION!"=="" set "CURRENT_VERSION=1"

echo Linking KV stores...
set "KV_STORES_LINKED=0"
call :link_kv_store "!SECRETS_STORE_ID!" "secrets" && set /a "KV_STORES_LINKED+=1"
call :link_kv_store "!API_KEYS_STORE_ID!" "api_keys" && set /a "KV_STORES_LINKED+=1"
call :link_kv_store "!WATERMARKING_CONFIG_STORE_ID!" "watermarking_config" && set /a "KV_STORES_LINKED+=1"

if !KV_STORES_LINKED! gtr 0 (
    echo ✓ KV stores linked to service
) else (
    echo WARNING: No KV stores could be linked
)

REM Populate KV stores
echo Populating KV stores...

REM Get secret from StegaWave API
for /f "delims=" %%a in ('curl -s https://api.stegawave.com/getsecret -H "X-API-Key:!STEGAWAVE_API_KEY!" 2^>nul') do set "SECRET_RESPONSE=%%a"
if "!SECRET_RESPONSE!"=="" set "SECRET_RESPONSE={}"

for /f "delims=" %%a in ('echo !SECRET_RESPONSE! ^| jq -r ".secret // empty" 2^>nul') do set "SECRET_KEY_HEX=%%a"
if "!SECRET_KEY_HEX!"=="" (
    echo WARNING: Failed to get secret from StegaWave API. Generating random secret as fallback...
    REM Generate random 32-byte hex string
    for /f "delims=" %%a in ('powershell -Command "[System.Web.Security.Membership]::GeneratePassword(64, 0)"') do set "SECRET_KEY_HEX=%%a"
)

REM Populate secrets store
fastly kv-store-entry create --store-id "!SECRETS_STORE_ID!" --key "SECRET_KEY_HEX" --value "!SECRET_KEY_HEX!" >nul 2>&1
if errorlevel 1 (
    fastly kv-store-entry delete --store-id "!SECRETS_STORE_ID!" --key "SECRET_KEY_HEX" >nul 2>&1
    fastly kv-store-entry create --store-id "!SECRETS_STORE_ID!" --key "SECRET_KEY_HEX" --value "!SECRET_KEY_HEX!" >nul 2>&1
)

REM Populate api_keys store
fastly kv-store-entry create --store-id "!API_KEYS_STORE_ID!" --key "service_api_key" --value "!STEGAWAVE_API_KEY!" >nul 2>&1
if errorlevel 1 (
    fastly kv-store-entry delete --store-id "!API_KEYS_STORE_ID!" --key "service_api_key" >nul 2>&1
    fastly kv-store-entry create --store-id "!API_KEYS_STORE_ID!" --key "service_api_key" --value "!STEGAWAVE_API_KEY!" >nul 2>&1
)

REM Populate watermarking_config store
call :get_config FMP4_AAC_PROFILE FMP4_AAC_PROFILE
call :get_config FMP4_SAMPLE_RATE FMP4_SAMPLE_RATE
call :get_config FMP4_CHANNELS FMP4_CHANNELS
call :get_config FMP4_TRACK_ID FMP4_TRACK_ID

call :populate_config_entry "!WATERMARKING_CONFIG_STORE_ID!" "FMP4_AAC_PROFILE" "!FMP4_AAC_PROFILE!"
call :populate_config_entry "!WATERMARKING_CONFIG_STORE_ID!" "FMP4_SAMPLE_RATE" "!FMP4_SAMPLE_RATE!"
call :populate_config_entry "!WATERMARKING_CONFIG_STORE_ID!" "FMP4_CHANNELS" "!FMP4_CHANNELS!"
call :populate_config_entry "!WATERMARKING_CONFIG_STORE_ID!" "FMP4_TRACK_ID" "!FMP4_TRACK_ID!"

echo ✓ KV stores populated

echo ✓ Setup complete! Your service '!NAME!' is deployed and configured.

goto :eof

REM --- Helper Functions ---

:get_config
REM %1 = config key, %2 = variable name to set
for /f "tokens=2 delims==" %%a in ('findstr "^%1=" CONFIG.txt 2^>nul') do (
    set "temp_value=%%a"
    REM Remove comments and trim whitespace
    for /f "tokens=1 delims=#" %%b in ("!temp_value!") do set "temp_value=%%b"
    set "temp_value=!temp_value: =!"
    set "%2=!temp_value!"
)
goto :eof

:get_kv_store_id
REM %1 = store name, %2 = JSON response, %3 = variable name to set
for /f "delims=" %%a in ('echo %2 ^| jq -r ".Data[]? | select(.Name == \"%1\") | .StoreID // empty" 2^>nul') do set "%3=%%a"
if "!%3!"=="" (
    for /f "delims=" %%a in ('echo %2 ^| jq -r ".[] | select(.Name == \"%1\") | .StoreID // empty" 2^>nul') do set "%3=%%a"
)
if "!%3!"=="" (
    for /f "delims=" %%a in ('echo %2 ^| jq -r ".Data[]? | select(.Name == \"%1\") | .ID // empty" 2^>nul') do set "%3=%%a"
)
goto :eof

:link_kv_store
REM %1 = store ID, %2 = store name
if "%1"=="" goto :eof
if "%1"=="null" goto :eof

REM Check if already linked
for /f "delims=" %%a in ('fastly resource-link list --service-id "!SERVICE_ID!" --version "!CURRENT_VERSION!" --json 2^>nul') do set "EXISTING_LINKS=%%a"
if "!EXISTING_LINKS!"=="" set "EXISTING_LINKS=[]"

echo !EXISTING_LINKS! | jq -e ".[] | select(.Name == \"%2\")" >nul 2>&1
if not errorlevel 1 goto :eof

REM Try to create new version and link
for /f "delims=" %%a in ('fastly service-version clone --service-id "!SERVICE_ID!" --version "!CURRENT_VERSION!" 2^>nul') do set "NEW_VERSION_OUTPUT=%%a"
if not errorlevel 1 (
    for /f "tokens=2" %%a in ('echo !NEW_VERSION_OUTPUT! ^| findstr "Version"') do set "NEW_VERSION=%%a"
    if not "!NEW_VERSION!"=="" (
        fastly resource-link create --service-id "!SERVICE_ID!" --version "!NEW_VERSION!" --resource-id "%1" --name "%2" >nul 2>&1
        if not errorlevel 1 (
            fastly service-version activate --service-id "!SERVICE_ID!" --version "!NEW_VERSION!" >nul 2>&1
            if not errorlevel 1 (
                set "CURRENT_VERSION=!NEW_VERSION!"
                goto :eof
            )
        )
    )
)

REM Fallback: try direct link with autoclone
fastly resource-link create --service-id "!SERVICE_ID!" --version "!CURRENT_VERSION!" --resource-id "%1" --name "%2" --autoclone >nul 2>&1
goto :eof

:populate_config_entry
REM %1 = store ID, %2 = key, %3 = value
if "%3"=="" goto :eof
fastly kv-store-entry create --store-id "%1" --key "%2" --value "%3" >nul 2>&1
if errorlevel 1 (
    fastly kv-store-entry delete --store-id "%1" --key "%2" >nul 2>&1
    fastly kv-store-entry create --store-id "%1" --key "%2" --value "%3" >nul 2>&1
)
goto :eof