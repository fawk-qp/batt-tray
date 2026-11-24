#include "icon.hpp"
#include "scanner.hpp"
#include "tray.hpp"

#include <dbt.h>
#include <mmdeviceapi.h>
#include <objbase.h>

#include <atomic>
#include <cstring>
#include <cwchar>
#include <mutex>
#include <thread>

namespace {

constexpr UINT_PTR kTimerDebounce = 1;
constexpr UINT kDebounceMs = 2000;
constexpr DWORD kPollIntervalMs = 60000;

constexpr GUID kClsidMMDeviceEnumerator = {
    0xBCDE0395, 0xE52F, 0x467C, {0x8E, 0x3D, 0xC4, 0x57, 0x92, 0x91, 0x69, 0x2E}
};
constexpr GUID kIidIMMDeviceEnumerator = {
    0xA95664D2, 0x9614, 0x4F35, {0xA7, 0x46, 0xDE, 0x8D, 0xB6, 0x36, 0x17, 0xE6}
};
constexpr GUID kIidIMMNotificationClient = {
    0x7991EEC9, 0x7E89, 0x4D85, {0x83, 0x90, 0x6C, 0x70, 0x3C, 0xEC, 0x60, 0xC0}
};
constexpr GUID kIidIUnknown = {
    0x00000000, 0x0000, 0x0000, {0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46}
};

class AudioChangeClient final : public IMMNotificationClient {
public:
    explicit AudioChangeClient(HANDLE scanEvent) : scanEvent_(scanEvent) {}

    ULONG STDMETHODCALLTYPE AddRef() override {
        return ++ref_;
    }

    ULONG STDMETHODCALLTYPE Release() override {
        const ULONG n = --ref_;
        if (n == 0) delete this;
        return n;
    }

    HRESULT STDMETHODCALLTYPE QueryInterface(REFIID riid, void** ppv) override {
        if (!ppv) return E_POINTER;
        if (std::memcmp(&riid, &kIidIUnknown, sizeof(GUID)) == 0 ||
            std::memcmp(&riid, &kIidIMMNotificationClient, sizeof(GUID)) == 0) {
            *ppv = static_cast<IMMNotificationClient*>(this);
            AddRef();
            return S_OK;
        }
        *ppv = nullptr;
        return E_NOINTERFACE;
    }

    HRESULT STDMETHODCALLTYPE OnDefaultDeviceChanged(EDataFlow flow, ERole role, LPCWSTR) override {
        if (flow == eRender && (role == eConsole || role == eMultimedia)) Trigger();
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE OnDeviceStateChanged(LPCWSTR, DWORD) override {
        Trigger();
        return S_OK;
    }

    HRESULT STDMETHODCALLTYPE OnDeviceAdded(LPCWSTR) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE OnDeviceRemoved(LPCWSTR) override { return S_OK; }
    HRESULT STDMETHODCALLTYPE OnPropertyValueChanged(LPCWSTR, const PROPERTYKEY) override { return S_OK; }

    void Detach() { scanEvent_.store(nullptr, std::memory_order_release); }

private:
    void Trigger() {
        HANDLE ev = scanEvent_.load(std::memory_order_acquire);
        if (ev) SetEvent(ev);
    }

    std::atomic<ULONG> ref_{1};
    std::atomic<HANDLE> scanEvent_{nullptr};
};

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
    const HRESULT comHr = CoInitializeEx(nullptr, COINIT_MULTITHREADED);

    IMMDeviceEnumerator* audioEnum = nullptr;
    AudioChangeClient* audioClient = nullptr;
    if (SUCCEEDED(comHr) &&
        SUCCEEDED(CoCreateInstance(kClsidMMDeviceEnumerator, nullptr, CLSCTX_INPROC_SERVER,
            kIidIMMDeviceEnumerator, reinterpret_cast<void**>(&audioEnum))) && audioEnum) {
        audioClient = new AudioChangeClient(app->scanEvent);
        if (FAILED(audioEnum->RegisterEndpointNotificationCallback(audioClient))) {
            audioClient->Release();
            audioClient = nullptr;
        }
    }

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

    if (audioEnum) {
        if (audioClient) {
            audioClient->Detach();
            audioEnum->UnregisterEndpointNotificationCallback(audioClient);
            audioClient->Release();
        }
        audioEnum->Release();
    }
    if (SUCCEEDED(comHr)) CoUninitialize();
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