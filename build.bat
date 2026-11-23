@echo off
chcp 65001 >nul
cd /d "%~dp0"
if not exist dist mkdir dist

if not defined GXX set "GXX=g++"
if not defined CARGO set "CARGO=cargo"

echo [1/2] Сборка C++ версии...
"%GXX%" -std=c++17 -O2 -s -static -municode -mwindows cpp\main.cpp -o dist\TrayBattery-cpp.exe -lsetupapi -lhid -lgdi32 -luser32 -lshell32 -ladvapi32
if errorlevel 1 (
    echo [ОШИБКА] C++ не собрался!
    pause
    exit /b 1
)

echo [2/2] Сборка Rust версии...
"%CARGO%" build --manifest-path rust\Cargo.toml --release
if errorlevel 1 (
    echo [ОШИБКА] Rust не собрался!
    pause
    exit /b 1
)
copy /y rust\target\release\tray-battery-rust.exe dist\TrayBattery-rust.exe >nul

echo.
echo Готово! Оба файла лежат в папке dist\:
dir /b dist\*.exe
pause