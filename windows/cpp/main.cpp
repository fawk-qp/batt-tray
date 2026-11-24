#include "icon.hpp"
#include "scanner.hpp"
#include "tray.hpp"

#include <dbt.h>
#include <objbase.h>

#include <cwchar>
#include <mutex>
#include <thread>

namespace {

constexpr UINT_PTR kTimerDebounce = 1;
constexpr UINT kDebounceMs = 2000;
constexpr DWORD kPollIntervalMs = 60000;

struct AppContext {
    TrayManager tray;
    UINT msgTaskbarCreated = 0;
    HANDLE stopEvent = nullptr;
    HANDLE scanEvent = nullptr;
    std::mutex stateMtx;
    std::array<DeviceState, 3> pendingStates{};
    std::thread worker;
};

AppContext* g_app = nullptr;

void WorkerLoop(HWND hwnd, AppContext* app) {
    HANDLE waits[2] = {app->stopEvent, app->scanEvent};
    for (;;) {
        auto result = ScanAllDevices();
        {
            std::lock_guard<std::mutex> lock(app->stateMtx);
            app->pendingStates = std::move(result);
        }
        PostMessageW(hwnd, WM_APP_SCAN_DONE, 0, 0);

        const DWORD rc = WaitForMultipleObjects(2, waits, FALSE, kPollIntervalMs);
        if (rc == WAIT_OBJECT_0 || rc == WAIT_FAILED) break;
    }
}

LRESULT CALLBACK WndProc(HWND hwnd, UINT msg, WPARAM wParam, LPARAM lParam) {
    if (g_app && g_app->msgTaskbarCreated != 0 && msg == g_app->msgTaskbarCreated) {
        g_app->tray.ReaddIcons();
        return 0;
    }

    switch (msg) {
        case WM_APP_TRAY:
            if (lParam == WM_RBUTTONUP || lParam == WM_CONTEXTMENU) {
                g_app->tray.ShowContextMenu(hwnd);
            }
            return 0;

        case WM_APP_SCAN_DONE: {
            std::array<DeviceState, 3> snapshot{};
            {
                std::lock_guard<std::mutex> lock(g_app->stateMtx);
                snapshot = g_app->pendingStates;
            }
            g_app->tray.Update(snapshot);
            return 0;
        }

        case WM_COMMAND:
            switch (LOWORD(wParam)) {
                case IDM_REFRESH:
                    if (g_app->scanEvent) SetEvent(g_app->scanEvent);
                    return 0;
                case IDM_AUTOSTART:
                    TrayManager::ToggleAutostart();
                    return 0;
                case IDM_EXIT:
                    DestroyWindow(hwnd);
                    return 0;
                default:
                    break;
            }
            break;

        case WM_DEVICECHANGE:
            if (wParam == DBT_DEVNODES_CHANGED || wParam == DBT_DEVICEARRIVAL || wParam == DBT_DEVICEREMOVECOMPLETE) {
                SetTimer(hwnd, kTimerDebounce, kDebounceMs, nullptr);
            }
            return 0;

        case WM_TIMER:
            if (wParam == kTimerDebounce) {
                KillTimer(hwnd, kTimerDebounce);
                if (g_app->scanEvent) SetEvent(g_app->scanEvent);
            }
            return 0;

        case WM_SETTINGCHANGE:
            if (lParam && std::wcscmp(reinterpret_cast<const wchar_t*>(lParam), L"ImmersiveColorSet") == 0) {
                g_app->tray.RefreshTheme();
            }
            return 0;

        case WM_DPICHANGED:
        case WM_DISPLAYCHANGE:
            g_app->tray.ReaddIcons();
            return 0;

        case WM_DESTROY:
            KillTimer(hwnd, kTimerDebounce);
            if (g_app) {
                if (g_app->stopEvent) SetEvent(g_app->stopEvent);
                if (g_app->worker.joinable()) g_app->worker.join();
                g_app->tray.Shutdown();
            }
            PostQuitMessage(0);
            return 0;

        default:
            break;
    }
    return DefWindowProcW(hwnd, msg, wParam, lParam);
}

} // namespace

int WINAPI wWinMain(HINSTANCE hInst, HINSTANCE, PWSTR, int) {
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    const HRESULT comHr = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);

    HANDLE singleMutex = CreateMutexW(nullptr, FALSE, L"Local\\BattTraySingleInstance");
    if (!singleMutex) {
        if (SUCCEEDED(comHr)) CoUninitialize();
        return 1;
    }
    if (GetLastError() == ERROR_ALREADY_EXISTS) {
        CloseHandle(singleMutex);
        if (SUCCEEDED(comHr)) CoUninitialize();
        return 0;
    }

    WNDCLASSEXW wc{};
    wc.cbSize = sizeof(wc);
    wc.lpfnWndProc = WndProc;
    wc.hInstance = hInst;
    wc.lpszClassName = L"BattTrayHiddenWindow";
    if (!RegisterClassExW(&wc)) {
        CloseHandle(singleMutex);
        if (SUCCEEDED(comHr)) CoUninitialize();
        return 1;
    }

    AppContext app{};
    app.msgTaskbarCreated = RegisterWindowMessageW(L"TaskbarCreated");
    app.stopEvent = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    app.scanEvent = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    g_app = &app;

    HWND hwnd = CreateWindowExW(0, wc.lpszClassName, L"", 0, 0, 0, 0, 0, nullptr, nullptr, hInst, nullptr);
    if (!hwnd || !app.stopEvent || !app.scanEvent) {
        if (hwnd) DestroyWindow(hwnd);
        if (app.stopEvent) CloseHandle(app.stopEvent);
        if (app.scanEvent) CloseHandle(app.scanEvent);
        CloseHandle(singleMutex);
        ShutdownIconRenderer();
        if (SUCCEEDED(comHr)) CoUninitialize();
        return 1;
    }

    app.tray.Init(hwnd);
    app.worker = std::thread(WorkerLoop, hwnd, &app);

    MSG msg{};
    while (GetMessageW(&msg, nullptr, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    g_app = nullptr;
    CloseHandle(app.stopEvent);
    CloseHandle(app.scanEvent);
    CloseHandle(singleMutex);
    ShutdownIconRenderer();
    if (SUCCEEDED(comHr)) CoUninitialize();
    return 0;
}