@echo off
setlocal
cd /d "%~dp0"

if not exist dist mkdir dist

where cl >nul 2>nul
if %errorlevel%==0 (
    echo [cpp] building windows/cpp...
    pushd windows\cpp
    cl /nologo /O2 /MT /EHsc /W4 device.cpp scanner.cpp icon.cpp tray.cpp main.cpp /Fe:..\..\dist\batt-tray-win-cpp.exe /Fo:%TEMP%\batt-tray-cpp-
    popd
) else (
    echo [cpp] cl.exe not found, skipping C++ build
)

where cargo >nul 2>nul
if %errorlevel%==0 (
    echo [rust] building windows/rust...
    pushd windows\rust
    cargo build --release
    copy /y target\release\batt-tray-win.exe ..\..\dist\batt-tray-win-rust.exe >nul
    popd
) else (
    echo [rust] cargo not found, skipping Rust build
)

echo Done. Binaries in dist\
endlocal
x`