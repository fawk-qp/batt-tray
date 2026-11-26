#![allow(non_snake_case, non_camel_case_types, dead_code)]

use std::ffi::c_void;

pub type HWND = *mut c_void;
pub type HINSTANCE = *mut c_void;
pub type HMODULE = *mut c_void;
pub type HANDLE = *mut c_void;
pub type HICON = *mut c_void;
pub type HCURSOR = *mut c_void;
pub type HBRUSH = *mut c_void;
pub type HMENU = *mut c_void;
pub type HDC = *mut c_void;
pub type HBITMAP = *mut c_void;
pub type HFONT = *mut c_void;
pub type HGDIOBJ = *mut c_void;
pub type HKEY = *mut c_void;
pub type HDEVINFO = *mut c_void;
pub type WPARAM = usize;
pub type LPARAM = isize;
pub type LRESULT = isize;
pub type BOOL = i32;
pub type HRESULT = i32;

pub type WNDPROC = unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT;
pub type FONTENUMPROCW = unsafe extern "system" fn(*const LOGFONTW, *const c_void, u32, LPARAM) -> i32;

pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
pub const ERROR_FILE_EXISTS: u32 = 80;
pub const ERROR_ALREADY_EXISTS: u32 = 183;
pub const ERROR_SUCCESS: i32 = 0;

pub const WM_NULL: u32 = 0x0000;
pub const WM_DESTROY: u32 = 0x0002;
pub const WM_SETTINGCHANGE: u32 = 0x001A;
pub const WM_CONTEXTMENU: u32 = 0x007B;
pub const WM_DISPLAYCHANGE: u32 = 0x007E;
pub const WM_COMMAND: u32 = 0x0111;
pub const WM_TIMER: u32 = 0x0113;
pub const WM_RBUTTONUP: u32 = 0x0205;
pub const WM_DEVICECHANGE: u32 = 0x0219;
pub const WM_DPICHANGED: u32 = 0x02E0;
pub const WM_APP: u32 = 0x8000;

pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: *mut c_void = -4isize as *mut c_void;

pub const DBT_DEVNODES_CHANGED: usize = 0x0007;
pub const DBT_DEVICEARRIVAL: usize = 0x8000;
pub const DBT_DEVICEREMOVECOMPLETE: usize = 0x8004;

pub const SM_CXSMICON: i32 = 49;
pub const TRANSPARENT: i32 = 1;
pub const BI_RGB: u32 = 0;
pub const DIB_RGB_COLORS: u32 = 0;
pub const DEFAULT_CHARSET: u8 = 1;
pub const ANTIALIASED_QUALITY: u8 = 4;
pub const FW_NORMAL: i32 = 400;
pub const DT_CENTER: u32 = 0x0001;
pub const DT_VCENTER: u32 = 0x0004;
pub const DT_SINGLELINE: u32 = 0x0020;
pub const DT_NOPREFIX: u32 = 0x0800;

pub const NIM_ADD: u32 = 0x0000_0000;
pub const NIM_MODIFY: u32 = 0x0000_0001;
pub const NIM_DELETE: u32 = 0x0000_0002;
pub const NIF_MESSAGE: u32 = 0x0000_0001;
pub const NIF_ICON: u32 = 0x0000_0002;
pub const NIF_TIP: u32 = 0x0000_0004;

pub const MF_STRING: u32 = 0x0000_0000;
pub const MF_GRAYED: u32 = 0x0000_0001;
pub const MF_UNCHECKED: u32 = 0x0000_0000;
pub const MF_CHECKED: u32 = 0x0000_0008;
pub const MF_POPUP: u32 = 0x0000_0010;
pub const MF_SEPARATOR: u32 = 0x0000_0800;
pub const TPM_LEFTALIGN: u32 = 0x0000;
pub const TPM_RIGHTBUTTON: u32 = 0x0002;
pub const TPM_BOTTOMALIGN: u32 = 0x0020;
pub const TPM_RETURNCMD: u32 = 0x0100;

pub const HKEY_CURRENT_USER: HKEY = 0x8000_0001usize as HKEY;
pub const RRF_RT_REG_SZ: u32 = 0x0000_0002;
pub const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
pub const KEY_SET_VALUE: u32 = 0x0002;
pub const REG_SZ: u32 = 1;

pub const WAIT_OBJECT_0: u32 = 0x0000_0000;
pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

pub const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
pub const GENERIC_READ: u32 = 0x8000_0000;
pub const GENERIC_WRITE: u32 = 0x4000_0000;
pub const FILE_SHARE_READ: u32 = 0x0000_0001;
pub const FILE_SHARE_WRITE: u32 = 0x0000_0002;
pub const CREATE_NEW: u32 = 1;
pub const OPEN_EXISTING: u32 = 3;
pub const FILE_ATTRIBUTE_NORMAL: u32 = 0x0000_0080;
pub const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
pub const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
pub const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
pub const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;

pub const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;

pub const DIGCF_PRESENT: u32 = 0x0000_0002;
pub const DIGCF_ALLCLASSES: u32 = 0x0000_0004;
pub const DIGCF_DEVICEINTERFACE: u32 = 0x0000_0010;

pub const DEVPROP_TYPE_BYTE: u32 = 0x0000_0003;
pub const DEVPROP_TYPE_UINT32: u32 = 0x0000_0007;
pub const DEVPROP_TYPE_GUID: u32 = 0x0000_000D;
pub const DEVPROP_TYPE_BOOLEAN: u32 = 0x0000_0011;
pub const DEVPROP_TYPE_STRING: u32 = 0x0000_0012;

pub const BLUETOOTH_GATT_FLAG_NONE: u32 = 0x0000_0000;

pub const COINIT_MULTITHREADED: u32 = 0x0;
pub const CLSCTX_INPROC_SERVER: u32 = 0x1;
pub const STGM_READ: u32 = 0x0;
pub const VT_LPWSTR: u16 = 31;
pub const VT_CLSID: u16 = 72;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct DEVPROPKEY {
    pub fmtid: GUID,
    pub pid: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct POINT {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RECT {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
pub struct WNDCLASSEXW {
    pub cbSize: u32,
    pub style: u32,
    pub lpfnWndProc: Option<WNDPROC>,
    pub cbClsExtra: i32,
    pub cbWndExtra: i32,
    pub hInstance: HINSTANCE,
    pub hIcon: HICON,
    pub hCursor: HCURSOR,
    pub hbrBackground: HBRUSH,
    pub lpszMenuName: *const u16,
    pub lpszClassName: *const u16,
    pub hIconSm: HICON,
}

#[repr(C)]
pub struct MSG {
    pub hwnd: HWND,
    pub message: u32,
    pub wParam: WPARAM,
    pub lParam: LPARAM,
    pub time: u32,
    pub pt: POINT,
}

#[repr(C)]
pub struct BITMAPINFOHEADER {
    pub biSize: u32,
    pub biWidth: i32,
    pub biHeight: i32,
    pub biPlanes: u16,
    pub biBitCount: u16,
    pub biCompression: u32,
    pub biSizeImage: u32,
    pub biXPelsPerMeter: i32,
    pub biYPelsPerMeter: i32,
    pub biClrUsed: u32,
    pub biClrImportant: u32,
}

#[repr(C)]
pub struct BITMAPINFO {
    pub bmiHeader: BITMAPINFOHEADER,
    pub bmiColors: [u32; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LOGFONTW {
    pub lfHeight: i32,
    pub lfWidth: i32,
    pub lfEscapement: i32,
    pub lfOrientation: i32,
    pub lfWeight: i32,
    pub lfItalic: u8,
    pub lfUnderline: u8,
    pub lfStrikeOut: u8,
    pub lfCharSet: u8,
    pub lfOutPrecision: u8,
    pub lfClipPrecision: u8,
    pub lfQuality: u8,
    pub lfPitchAndFamily: u8,
    pub lfFaceName: [u16; 32],
}

#[repr(C)]
pub struct ICONINFO {
    pub fIcon: BOOL,
    pub xHotspot: u32,
    pub yHotspot: u32,
    pub hbmMask: HBITMAP,
    pub hbmColor: HBITMAP,
}

#[repr(C)]
pub struct NOTIFYICONDATAW {
    pub cbSize: u32,
    pub hWnd: HWND,
    pub uID: u32,
    pub uFlags: u32,
    pub uCallbackMessage: u32,
    pub hIcon: HICON,
    pub szTip: [u16; 128],
    pub dwState: u32,
    pub dwStateMask: u32,
    pub szInfo: [u16; 256],
    pub uTimeoutOrVersion: u32,
    pub szInfoTitle: [u16; 64],
    pub dwInfoFlags: u32,
    pub guidItem: GUID,
    pub hBalloonIcon: HICON,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVINFO_DATA {
    pub cbSize: u32,
    pub ClassGuid: GUID,
    pub DevInst: u32,
    pub Reserved: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cbSize: u32,
    pub InterfaceClassGuid: GUID,
    pub Flags: u32,
    pub Reserved: usize,
}

#[repr(C)]
pub struct SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    pub cbSize: u32,
    pub DevicePath: [u16; 1],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BTH_LE_UUID {
    pub IsShortUuid: u8,
    pub _pad: [u8; 3],
    pub Value: GUID,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BTH_LE_GATT_CHARACTERISTIC {
    pub ServiceHandle: u16,
    pub CharacteristicUuid: BTH_LE_UUID,
    pub AttributeHandle: u16,
    pub CharacteristicValueHandle: u16,
    pub IsBroadcastable: u8,
    pub IsReadable: u8,
    pub IsWritable: u8,
    pub IsWritableWithoutResponse: u8,
    pub IsSignedWritable: u8,
    pub IsNotifiable: u8,
    pub IsIndicatable: u8,
    pub HasExtendedProperties: u8,
}

#[repr(C)]
pub struct BTH_LE_GATT_CHARACTERISTIC_VALUE {
    pub DataSize: u32,
    pub Data: [u8; 1],
}

#[repr(C)]
pub struct BLUETOOTH_FIND_RADIO_PARAMS {
    pub dwSize: u32,
}

#[repr(C)]
pub struct PROPVARIANT {
    pub vt: u16,
    pub r1: u16,
    pub r2: u16,
    pub r3: u16,
    pub ptr: *mut c_void,
    pub pad: usize,
}

impl Default for PROPVARIANT {
    fn default() -> Self {
        Self { vt: 0, r1: 0, r2: 0, r3: 0, ptr: std::ptr::null_mut(), pad: 0 }
    }
}

#[repr(C)]
pub struct IPropertyStore {
    pub lpVtbl: *const IPropertyStoreVtbl,
}

#[repr(C)]
pub struct IPropertyStoreVtbl {
    pub QueryInterface: *const c_void,
    pub AddRef: *const c_void,
    pub Release: unsafe extern "system" fn(*mut IPropertyStore) -> u32,
    pub GetCount: *const c_void,
    pub GetAt: *const c_void,
    pub GetValue: unsafe extern "system" fn(*mut IPropertyStore, *const DEVPROPKEY, *mut PROPVARIANT) -> HRESULT,
}

#[repr(C)]
pub struct IMMDevice {
    pub lpVtbl: *const IMMDeviceVtbl,
}

#[repr(C)]
pub struct IMMDeviceVtbl {
    pub QueryInterface: *const c_void,
    pub AddRef: *const c_void,
    pub Release: unsafe extern "system" fn(*mut IMMDevice) -> u32,
    pub Activate: *const c_void,
    pub OpenPropertyStore: unsafe extern "system" fn(*mut IMMDevice, u32, *mut *mut IPropertyStore) -> HRESULT,
}

#[repr(C)]
pub struct IMMDeviceEnumerator {
    pub lpVtbl: *const IMMDeviceEnumeratorVtbl,
}

#[repr(C)]
pub struct IMMDeviceEnumeratorVtbl {
    pub QueryInterface: *const c_void,
    pub AddRef: *const c_void,
    pub Release: unsafe extern "system" fn(*mut IMMDeviceEnumerator) -> u32,
    pub EnumAudioEndpoints: *const c_void,
    pub GetDefaultAudioEndpoint: unsafe extern "system" fn(*mut IMMDeviceEnumerator, u32, u32, *mut *mut IMMDevice) -> HRESULT,
}

#[repr(C)]
#[derive(Default)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}

#[repr(C)]
#[derive(Default)]
pub struct BY_HANDLE_FILE_INFORMATION {
    pub dwFileAttributes: u32,
    pub ftCreationTime: FILETIME,
    pub ftLastAccessTime: FILETIME,
    pub ftLastWriteTime: FILETIME,
    pub dwVolumeSerialNumber: u32,
    pub nFileSizeHigh: u32,
    pub nFileSizeLow: u32,
    pub nNumberOfLinks: u32,
    pub nFileIndexHigh: u32,
    pub nFileIndexLow: u32,
}

#[repr(C)]
pub struct MitigationFlagsPolicy {
    pub flags: u32,
}

#[repr(C)]
pub struct PowerThrottlingPolicy {
    pub version: u32,
    pub control_mask: u32,
    pub state_mask: u32,
}

pub type FnBluetoothGattGetCharacteristics = unsafe extern "system" fn(
    HANDLE,
    *mut c_void,
    u16,
    *mut BTH_LE_GATT_CHARACTERISTIC,
    *mut u16,
    u32,
) -> HRESULT;

pub type FnBluetoothGattGetCharacteristicValue = unsafe extern "system" fn(
    HANDLE,
    *mut BTH_LE_GATT_CHARACTERISTIC,
    u32,
    *mut BTH_LE_GATT_CHARACTERISTIC_VALUE,
    *mut u16,
    u32,
) -> HRESULT;

pub type FnBluetoothFindFirstRadio = unsafe extern "system" fn(
    *const BLUETOOTH_FIND_RADIO_PARAMS,
    *mut HANDLE,
) -> HANDLE;

pub type FnBluetoothFindNextRadio = unsafe extern "system" fn(HANDLE, *mut HANDLE) -> BOOL;
pub type FnBluetoothFindRadioClose = unsafe extern "system" fn(HANDLE) -> BOOL;

#[link(name = "kernel32")]
extern "system" {
    pub fn GetCurrentProcess() -> HANDLE;
    pub fn SetPriorityClass(hProcess: HANDLE, dwPriorityClass: u32) -> BOOL;
    pub fn SetProcessInformation(
        hProcess: HANDLE,
        ProcessInformationClass: i32,
        ProcessInformation: *const c_void,
        ProcessInformationSize: u32,
    ) -> BOOL;
    pub fn SetProcessWorkingSetSizeEx(
        hProcess: HANDLE,
        dwMinimumWorkingSetSize: usize,
        dwMaximumWorkingSetSize: usize,
        Flags: u32,
    ) -> BOOL;
    pub fn SetDefaultDllDirectories(DirectoryFlags: u32) -> BOOL;
    pub fn SetProcessMitigationPolicy(
        MitigationPolicy: i32,
        lpBuffer: *const c_void,
        dwLength: usize,
    ) -> BOOL;
    pub fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
    pub fn GetModuleFileNameW(hModule: HMODULE, lpFilename: *mut u16, nSize: u32) -> u32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetTickCount64() -> u64;
    pub fn CreateMutexW(lpAttr: *mut c_void, bInitialOwner: BOOL, lpName: *const u16) -> HANDLE;
    pub fn GetLastError() -> u32;
    pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    pub fn WriteFile(
        hFile: HANDLE,
        lpBuffer: *const c_void,
        nNumberOfBytesToWrite: u32,
        lpNumberOfBytesWritten: *mut u32,
        lpOverlapped: *mut c_void,
    ) -> BOOL;
    pub fn GetFileInformationByHandle(
        hFile: HANDLE,
        lpFileInformation: *mut BY_HANDLE_FILE_INFORMATION,
    ) -> BOOL;
    pub fn DeleteFileW(lpFileName: *const u16) -> BOOL;
    pub fn MoveFileExW(
        lpExistingFileName: *const u16,
        lpNewFileName: *const u16,
        dwFlags: u32,
    ) -> BOOL;
    pub fn DuplicateHandle(
        hSourceProcessHandle: HANDLE,
        hSourceHandle: HANDLE,
        hTargetProcessHandle: HANDLE,
        lpTargetHandle: *mut HANDLE,
        dwDesiredAccess: u32,
        bInheritHandle: BOOL,
        dwOptions: u32,
    ) -> BOOL;
    pub fn Sleep(dwMilliseconds: u32);
    pub fn DeviceIoControl(
        hDevice: HANDLE,
        dwIoControlCode: u32,
        lpInBuffer: *const c_void,
        nInBufferSize: u32,
        lpOutBuffer: *mut c_void,
        nOutBufferSize: u32,
        lpBytesReturned: *mut u32,
        lpOverlapped: *mut c_void,
    ) -> BOOL;
    pub fn CreateEventW(lpAttr: *mut c_void, bManualReset: BOOL, bInitialState: BOOL, lpName: *const u16) -> HANDLE;
    pub fn SetEvent(hEvent: HANDLE) -> BOOL;
    pub fn WaitForMultipleObjects(nCount: u32, lpHandles: *const HANDLE, bWaitAll: BOOL, dwMilliseconds: u32) -> u32;
    pub fn LoadLibraryExW(lpLibFileName: *const u16, hFile: HANDLE, dwFlags: u32) -> HMODULE;
    pub fn GetProcAddress(hModule: HMODULE, lpProcName: *const i8) -> *const c_void;
    pub fn FreeLibrary(hLibModule: HMODULE) -> BOOL;
    pub fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: HANDLE,
    ) -> HANDLE;
}

#[link(name = "user32")]
extern "system" {
    pub fn SetProcessDpiAwarenessContext(value: *mut c_void) -> BOOL;
    pub fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> HWND;
    pub fn GetDpiForWindow(hwnd: HWND) -> u32;
    pub fn GetDpiForSystem() -> u32;
    pub fn GetSystemMetricsForDpi(nIndex: i32, dpi: u32) -> i32;
    pub fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    pub fn CreateWindowExW(
        dwExStyle: u32,
        lpClassName: *const u16,
        lpWindowName: *const u16,
        dwStyle: u32,
        x: i32,
        y: i32,
        nWidth: i32,
        nHeight: i32,
        hWndParent: HWND,
        hMenu: HMENU,
        hInstance: HINSTANCE,
        lpParam: *mut c_void,
    ) -> HWND;
    pub fn DestroyWindow(hWnd: HWND) -> BOOL;
    pub fn DefWindowProcW(hWnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
    pub fn GetMessageW(lpMsg: *mut MSG, hWnd: HWND, wMsgFilterMin: u32, wMsgFilterMax: u32) -> BOOL;
    pub fn TranslateMessage(lpMsg: *const MSG) -> BOOL;
    pub fn DispatchMessageW(lpMsg: *const MSG) -> LRESULT;
    pub fn PostMessageW(hWnd: HWND, msg: u32, wParam: WPARAM, lParam: LPARAM) -> BOOL;
    pub fn PostQuitMessage(nExitCode: i32);
    pub fn RegisterWindowMessageW(lpString: *const u16) -> u32;
    pub fn SetCoalescableTimer(
        hWnd: HWND,
        nIDEvent: usize,
        uElapse: u32,
        lpTimerFunc: *const c_void,
        uToleranceDelay: u32,
    ) -> usize;
    pub fn KillTimer(hWnd: HWND, uIDEvent: usize) -> BOOL;
    pub fn GetDC(hWnd: HWND) -> HDC;
    pub fn ReleaseDC(hWnd: HWND, hDC: HDC) -> i32;
    pub fn DrawTextW(hdc: HDC, lpchText: *const u16, cchText: i32, lprc: *mut RECT, format: u32) -> i32;
    pub fn CreateIconIndirect(piconinfo: *const ICONINFO) -> HICON;
    pub fn DestroyIcon(hIcon: HICON) -> BOOL;
    pub fn CreatePopupMenu() -> HMENU;
    pub fn AppendMenuW(hMenu: HMENU, uFlags: u32, uIDNewItem: usize, lpNewItem: *const u16) -> BOOL;
    pub fn TrackPopupMenu(hMenu: HMENU, uFlags: u32, x: i32, y: i32, nReserved: i32, hWnd: HWND, prcRect: *const RECT) -> BOOL;
    pub fn DestroyMenu(hMenu: HMENU) -> BOOL;
    pub fn GetCursorPos(lpPoint: *mut POINT) -> BOOL;
    pub fn SetForegroundWindow(hWnd: HWND) -> BOOL;
}

#[link(name = "gdi32")]
extern "system" {
    pub fn CreateCompatibleDC(hdc: HDC) -> HDC;
    pub fn DeleteDC(hdc: HDC) -> BOOL;
    pub fn CreateDIBSection(
        hdc: HDC,
        pbmi: *const BITMAPINFO,
        usage: u32,
        ppvBits: *mut *mut c_void,
        hSection: HANDLE,
        offset: u32,
    ) -> HBITMAP;
    pub fn SelectObject(hdc: HDC, h: HGDIOBJ) -> HGDIOBJ;
    pub fn DeleteObject(ho: HGDIOBJ) -> BOOL;
    pub fn CreateFontIndirectW(lplf: *const LOGFONTW) -> HFONT;
    pub fn SetBkMode(hdc: HDC, mode: i32) -> i32;
    pub fn SetTextColor(hdc: HDC, color: u32) -> u32;
    pub fn CreateBitmap(nWidth: i32, nHeight: i32, nPlanes: u32, nBitCount: u32, lpBits: *const c_void) -> HBITMAP;
    pub fn GdiFlush() -> BOOL;
    pub fn EnumFontFamiliesExW(hdc: HDC, lpLogfont: *mut LOGFONTW, lpProc: FONTENUMPROCW, lParam: LPARAM, dwFlags: u32) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    pub fn Shell_NotifyIconW(dwMessage: u32, lpData: *mut NOTIFYICONDATAW) -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
    pub fn RegGetValueW(
        hkey: HKEY,
        lpSubKey: *const u16,
        lpValue: *const u16,
        dwFlags: u32,
        pdwType: *mut u32,
        pvData: *mut c_void,
        pcbData: *mut u32,
    ) -> i32;
    pub fn RegOpenKeyExW(hKey: HKEY, lpSubKey: *const u16, ulOptions: u32, samDesired: u32, phkResult: *mut HKEY) -> i32;
    pub fn RegSetValueExW(hKey: HKEY, lpValueName: *const u16, Reserved: u32, dwType: u32, lpData: *const u8, cbData: u32) -> i32;
    pub fn RegDeleteValueW(hKey: HKEY, lpValueName: *const u16) -> i32;
    pub fn RegCloseKey(hKey: HKEY) -> i32;
}

#[link(name = "setupapi")]
extern "system" {
    pub fn SetupDiGetClassDevsW(ClassGuid: *const GUID, Enumerator: *const u16, hwndParent: HWND, Flags: u32) -> HDEVINFO;
    pub fn SetupDiEnumDeviceInfo(DeviceInfoSet: HDEVINFO, MemberIndex: u32, DeviceInfoData: *mut SP_DEVINFO_DATA) -> BOOL;
    pub fn SetupDiGetDeviceInstanceIdW(
        DeviceInfoSet: HDEVINFO,
        DeviceInfoData: *mut SP_DEVINFO_DATA,
        DeviceInstanceId: *mut u16,
        DeviceInstanceIdSize: u32,
        RequiredSize: *mut u32,
    ) -> BOOL;
    pub fn SetupDiGetDevicePropertyW(
        DeviceInfoSet: HDEVINFO,
        DeviceInfoData: *mut SP_DEVINFO_DATA,
        PropertyKey: *const DEVPROPKEY,
        PropertyType: *mut u32,
        PropertyBuffer: *mut u8,
        PropertyBufferSize: u32,
        RequiredSize: *mut u32,
        Flags: u32,
    ) -> BOOL;
    pub fn SetupDiEnumDeviceInterfaces(
        DeviceInfoSet: HDEVINFO,
        DeviceInfoData: *mut SP_DEVINFO_DATA,
        InterfaceClassGuid: *const GUID,
        MemberIndex: u32,
        DeviceInterfaceData: *mut SP_DEVICE_INTERFACE_DATA,
    ) -> BOOL;
    pub fn SetupDiGetDeviceInterfaceDetailW(
        DeviceInfoSet: HDEVINFO,
        DeviceInterfaceData: *mut SP_DEVICE_INTERFACE_DATA,
        DeviceInterfaceDetailData: *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        DeviceInterfaceDetailDataSize: u32,
        RequiredSize: *mut u32,
        DeviceInfoData: *mut SP_DEVINFO_DATA,
    ) -> BOOL;
    pub fn SetupDiDestroyDeviceInfoList(DeviceInfoSet: HDEVINFO) -> BOOL;
}

#[link(name = "ole32")]
extern "system" {
    pub fn CoInitializeEx(pvReserved: *mut c_void, dwCoInit: u32) -> HRESULT;
    pub fn CoUninitialize();
    pub fn CoCreateInstance(
        rclsid: *const GUID,
        pUnkOuter: *mut c_void,
        dwClsContext: u32,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> HRESULT;
    pub fn PropVariantClear(pvar: *mut PROPVARIANT) -> HRESULT;
}

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn from_wide_slice(slice: &[u16]) -> String {
    let len = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..len])
}