@echo off
REM This script ensures the Rust toolchain is available and then runs the setup application.

REM Check if cargo is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo Rust and Cargo are not installed. Please install them to continue.
    echo You can find installation instructions at: https://www.rust-lang.org/tools/install
    exit /b 1
)

REM Run the setup binary, passing along all arguments to this script
REM This will compile and run the src/bin/setup.rs file
cargo run --bin setup -- %*
