@echo off
setlocal
cd /d "%~dp0"

if not exist dist mkdir dist
del /q dist\batt-tray-win-*.exe 2>nul
if not exist "%TEMP%\batt-tray-cpp" mkdir "%TEMP%\batt-tray-cpp"

where cl >nul 2>nul
if %errorlevel%==0 (
    echo [cpp] building windows/cpp...
    rc /nologo /fo windows\app.res windows\app.rc
    if errorlevel 1 exit /b 1
    cl /nologo /std:c++17 /O2 /MT /utf-8 /EHsc /GS /guard:cf /DUNICODE /D_UNICODE /D_WIN32_WINNT=0x0A00 ^
       windows\cpp\*.cpp windows\app.res /Fe:dist\batt-tray-win-cpp.exe /Fo:%TEMP%\batt-tray-cpp\ ^
       /link /SUBSYSTEM:WINDOWS /ENTRY:wWinMainCRTStartup /DYNAMICBASE /NXCOMPAT /HIGHENTROPYVA /GUARD:CF /OPT:REF /OPT:ICF /MANIFEST:NO ^
       user32.lib gdi32.lib shell32.lib advapi32.lib setupapi.lib hid.lib d2d1.lib dwrite.lib windowscodecs.lib ole32.lib
    if errorlevel 1 exit /b 1
) else (
    echo [cpp] cl.exe not found, skipping C++ build
)

where cargo >nul 2>nul
if %errorlevel%==0 (
    echo [rust] building windows/rust...
    set "RUSTFLAGS=-C control-flow-guard=yes"
    pushd windows\rust
    cargo build --release
    if errorlevel 1 (
        popd
        exit /b 1
    )
    copy /y target\release\batt-tray-win.exe ..\..\dist\batt-tray-win-rust.exe >nul
    if errorlevel 1 (
        popd
        exit /b 1
    )
    popd
) else (
    echo [rust] cargo not found, skipping Rust build
)

echo Done. Binaries in dist\
endlocal
