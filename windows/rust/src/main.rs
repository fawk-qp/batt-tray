#![windows_subsystem = "windows"]

mod device;
mod history;
mod icon;
mod scanner;
mod tray;
mod win32;

use device::DeviceState;
use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use tray::{toggle_autostart, TrayManager, IDM_AUTOSTART, IDM_EXIT, IDM_HIST_CLEAR, IDM_REFRESH, WM_APP_SCAN_DONE, WM_APP_TRAY};
use win32::*;

const TIMER_DEBOUNCE: usize = 1;
const DEBOUNCE_MS: u32 = 2000;
const COALESCE_TOLERANCE_MS: u32 = 500;
const POLL_INTERVAL_MS: u32 = 120_000;

#[derive(Clone, Copy)]
struct SendPtr(*mut std::ffi::c_void);
unsafe impl Send for SendPtr {}
unsafe impl Sync for SendPtr {}

fn apply_process_hardening_and_eco_mode() {
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32);

        let dyn_code_policy = MitigationFlagsPolicy { flags: 0x1 };
        SetProcessMitigationPolicy(2, &dyn_code_policy as *const _ as *const c_void, std::mem::size_of::<MitigationFlagsPolicy>());

        let ext_policy = MitigationFlagsPolicy { flags: 0x1 };
        SetProcessMitigationPolicy(6, &ext_policy as *const _ as *const c_void, std::mem::size_of::<MitigationFlagsPolicy>());

        let font_policy = MitigationFlagsPolicy { flags: 0x1 };
        SetProcessMitigationPolicy(9, &font_policy as *const _ as *const c_void, std::mem::size_of::<MitigationFlagsPolicy>());

        let img_policy = MitigationFlagsPolicy { flags: 0x1 | 0x2 | 0x4 };
        SetProcessMitigationPolicy(10, &img_policy as *const _ as *const c_void, std::mem::size_of::<MitigationFlagsPolicy>());

        let handle_policy = MitigationFlagsPolicy { flags: 0x1 | 0x2 };
        SetProcessMitigationPolicy(3, &handle_policy as *const _ as *const c_void, std::mem::size_of::<MitigationFlagsPolicy>());

        SetPriorityClass(GetCurrentProcess(), IDLE_PRIORITY_CLASS);
        let eco = PowerThrottlingPolicy { version: 1, control_mask: 1, state_mask: 1 };
        SetProcessInformation(GetCurrentProcess(), 4, &eco as *const _ as *const c_void, std::mem::size_of::<PowerThrottlingPolicy>() as u32);
    }
}

fn trim_idle_working_set() {
    unsafe {
        SetProcessWorkingSetSizeEx(GetCurrentProcess(), usize::MAX, usize::MAX, 0);
    }
}

struct AppState {
    tray: TrayManager,
    msg_taskbar_created: u32,
    stop_event: HANDLE,
    scan_event: HANDLE,
    pending: Arc<Mutex<[DeviceState; 3]>>,
    worker: Option<JoinHandle<()>>,
}

thread_local! {
    static APP: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

fn worker_loop(hwnd: SendPtr, stop: SendPtr, scan: SendPtr, pending: Arc<Mutex<[DeviceState; 3]>>) {
    let com_hr = unsafe { CoInitializeEx(null_mut(), COINIT_MULTITHREADED) };
    let waits = [stop.0, scan.0];
    loop {
        let res = scanner::scan_all_devices();
        if let Ok(mut guard) = pending.lock() {
            *guard = res;
        }
        unsafe {
            PostMessageW(hwnd.0, WM_APP_SCAN_DONE, 0, 0);
            let rc = WaitForMultipleObjects(2, waits.as_ptr(), 0, POLL_INTERVAL_MS);
            if rc == WAIT_OBJECT_0 || rc == WAIT_FAILED {
                break;
            }
        }
    }
    if com_hr >= 0 {
        unsafe { CoUninitialize() };
    }
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let taskbar_recreated = APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            if app.msg_taskbar_created != 0 && msg == app.msg_taskbar_created {
                app.tray.readd_icons();
                trim_idle_working_set();
                return true;
            }
        }
        false
    });
    if taskbar_recreated {
        return 0;
    }

    match msg {
        WM_APP_TRAY => {
            let ev = lparam as u32;
            if ev == WM_RBUTTONUP || ev == WM_CONTEXTMENU {
                APP.with(|cell| {
                    if let Some(app) = cell.borrow_mut().as_mut() {
                        app.tray.show_context_menu(hwnd, wparam as u32);
                    }
                });
            }
            0
        }
        WM_APP_SCAN_DONE => {
            APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    let snapshot = app.pending.lock().map(|g| g.clone()).unwrap_or_default();
                    if app.tray.update(snapshot) {
                        trim_idle_working_set();
                    }
                }
            });
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                IDM_REFRESH => {
                    APP.with(|cell| {
                        if let Some(app) = cell.borrow().as_ref() {
                            SetEvent(app.scan_event);
                        }
                    });
                    0
                }
                IDM_AUTOSTART => {
                    toggle_autostart();
                    0
                }
                IDM_EXIT => {
                    DestroyWindow(hwnd);
                    0
                }
                cmd if cmd >= IDM_HIST_CLEAR => {
                    APP.with(|cell| {
                        if let Some(app) = cell.borrow_mut().as_mut() {
                            app.tray.handle_menu_command(cmd);
                        }
                    });
                    0
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        WM_DEVICECHANGE => {
            if wparam == DBT_DEVNODES_CHANGED || wparam == DBT_DEVICEARRIVAL || wparam == DBT_DEVICEREMOVECOMPLETE {
                SetCoalescableTimer(hwnd, TIMER_DEBOUNCE, DEBOUNCE_MS, null(), COALESCE_TOLERANCE_MS);
            }
            0
        }
        WM_TIMER => {
            if wparam == TIMER_DEBOUNCE {
                KillTimer(hwnd, TIMER_DEBOUNCE);
                APP.with(|cell| {
                    if let Some(app) = cell.borrow().as_ref() {
                        SetEvent(app.scan_event);
                    }
                });
            }
            0
        }
        WM_DPICHANGED | WM_DISPLAYCHANGE => {
            APP.with(|cell| {
                if let Some(app) = cell.borrow_mut().as_mut() {
                    app.tray.readd_icons();
                }
            });
            trim_idle_working_set();
            0
        }
        WM_SETTINGCHANGE => {
            if lparam != 0 {
                let ptr = lparam as *const u16;
                let mut len = 0usize;
                while len < 64 && *ptr.add(len) != 0 {
                    len += 1;
                }
                if String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len)) == "ImmersiveColorSet" {
                    APP.with(|cell| {
                        if let Some(app) = cell.borrow_mut().as_mut() {
                            app.tray.refresh_theme();
                        }
                    });
                    trim_idle_working_set();
                }
            }
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_DEBOUNCE);
            let mut old_app = APP.with(|cell| cell.borrow_mut().take());
            if let Some(ref mut app) = old_app {
                SetEvent(app.stop_event);
                if let Some(handle) = app.worker.take() {
                    let _ = handle.join();
                }
                app.tray.shutdown();
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        apply_process_hardening_and_eco_mode();
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        let mutex_name = to_wide("Local\\BattTraySingleInstance");
        let single_mutex = CreateMutexW(null_mut(), 0, mutex_name.as_ptr());
        if single_mutex.is_null() {
            return;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(single_mutex);
            return;
        }

        let hinst = GetModuleHandleW(null());
        let cls_name = to_wide("BattTrayHiddenWindow");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinst,
            hIcon: null_mut(),
            hCursor: null_mut(),
            hbrBackground: null_mut(),
            lpszMenuName: null(),
            lpszClassName: cls_name.as_ptr(),
            hIconSm: null_mut(),
        };
        if RegisterClassExW(&wc) == 0 {
            CloseHandle(single_mutex);
            return;
        }

        let empty = [0u16; 1];
        let hwnd = CreateWindowExW(0, cls_name.as_ptr(), empty.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null_mut());
        let stop_event = CreateEventW(null_mut(), 1, 0, null());
        let scan_event = CreateEventW(null_mut(), 0, 0, null());
        if hwnd.is_null() || stop_event.is_null() || scan_event.is_null() {
            if !hwnd.is_null() { DestroyWindow(hwnd); }
            if !stop_event.is_null() { CloseHandle(stop_event); }
            if !scan_event.is_null() { CloseHandle(scan_event); }
            CloseHandle(single_mutex);
            return;
        }

        let taskbar_msg_name = to_wide("TaskbarCreated");
        let msg_taskbar_created = RegisterWindowMessageW(taskbar_msg_name.as_ptr());
        let tray = TrayManager::new(hwnd);
        let pending = Arc::new(Mutex::new(Default::default()));

        let hwnd_s = SendPtr(hwnd);
        let stop_s = SendPtr(stop_event);
        let scan_s = SendPtr(scan_event);
        let pending_bg = Arc::clone(&pending);
        let worker = thread::spawn(move || worker_loop(hwnd_s, stop_s, scan_s, pending_bg));

        APP.with(|cell| {
            *cell.borrow_mut() = Some(AppState {
                tray,
                msg_taskbar_created,
                stop_event,
                scan_event,
                pending,
                worker: Some(worker),
            });
        });

        trim_idle_working_set();

        let mut msg = std::mem::zeroed::<MSG>();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        CloseHandle(stop_event);
        CloseHandle(scan_event);
        CloseHandle(single_mutex);
    }
}
