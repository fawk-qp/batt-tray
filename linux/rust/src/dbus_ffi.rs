#![allow(dead_code)]

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr::{null, null_mut};

pub const DBUS_BUS_SESSION: c_int = 0;
pub const DBUS_BUS_SYSTEM: c_int = 1;
pub const DBUS_TYPE_BYTE: c_int = b'y' as c_int;
pub const DBUS_TYPE_BOOLEAN: c_int = b'b' as c_int;
pub const DBUS_TYPE_INT32: c_int = b'i' as c_int;
pub const DBUS_TYPE_UINT32: c_int = b'u' as c_int;
pub const DBUS_TYPE_DOUBLE: c_int = b'd' as c_int;
pub const DBUS_TYPE_STRING: c_int = b's' as c_int;
pub const DBUS_TYPE_OBJECT_PATH: c_int = b'o' as c_int;
pub const DBUS_TYPE_ARRAY: c_int = b'a' as c_int;
pub const DBUS_TYPE_VARIANT: c_int = b'v' as c_int;
pub const DBUS_TYPE_STRUCT: c_int = b'r' as c_int;
pub const DBUS_TYPE_DICT_ENTRY: c_int = b'e' as c_int;

pub const DBUS_MESSAGE_TYPE_SIGNAL: c_int = 4;
pub const DBUS_DISPATCH_DATA_REMAINS: c_int = 0;
pub const DBUS_HANDLER_RESULT_HANDLED: c_int = 0;
pub const DBUS_HANDLER_RESULT_NOT_YET_HANDLED: c_int = 1;
pub const DBUS_NAME_FLAG_DO_NOT_QUEUE: u32 = 0x4;

pub const POLLIN: i16 = 0x0001;
pub const SIG_BLOCK: c_int = 0;
pub const SIGINT: c_int = 2;
pub const SIGTERM: c_int = 15;
pub const SFD_NONBLOCK: c_int = 0o0004000;
pub const SFD_CLOEXEC: c_int = 0o2000000;
pub const LOCK_EX: c_int = 2;
pub const LOCK_NB: c_int = 4;

#[cfg(any(target_arch = "aarch64", target_arch = "arm"))]
pub const O_NOFOLLOW: c_int = 0o100000;
#[cfg(not(any(target_arch = "aarch64", target_arch = "arm")))]
pub const O_NOFOLLOW: c_int = 0o400000;

pub const O_NONBLOCK: c_int = 0o0004000;
pub const O_CLOEXEC: c_int = 0o2000000;

#[repr(C)]
pub struct DBusConnection {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DBusMessage {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DBusError {
    pub name: *const c_char,
    pub message: *const c_char,
    _pad: [usize; 6],
}

#[repr(C)]
pub struct DBusMessageIter {
    _opaque: [usize; 16],
}

pub type MsgFn = unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_void) -> c_int;

#[repr(C)]
pub struct DBusObjectPathVTable {
    pub unregister_function: Option<unsafe extern "C" fn(*mut DBusConnection, *mut c_void)>,
    pub message_function: Option<MsgFn>,
    pub _pad: [usize; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PollFd {
    pub fd: c_int,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
pub struct SigSet {
    pub _val: [u64; 16],
}

#[link(name = "c")]
extern "C" {
    pub fn poll(fds: *mut PollFd, nfds: usize, timeout: c_int) -> c_int;
    pub fn getpid() -> c_int;
    pub fn getuid() -> u32;
    pub fn flock(fd: c_int, op: c_int) -> c_int;
    pub fn close(fd: c_int) -> c_int;
    pub fn read(fd: c_int, buf: *mut c_void, count: usize) -> isize;
    pub fn sigemptyset(set: *mut SigSet) -> c_int;
    pub fn sigaddset(set: *mut SigSet, sig: c_int) -> c_int;
    pub fn sigprocmask(how: c_int, set: *const SigSet, old: *mut SigSet) -> c_int;
    pub fn signalfd(fd: c_int, mask: *const SigSet, flags: c_int) -> c_int;
}

#[link(name = "dbus-1")]
extern "C" {
    pub fn dbus_error_init(err: *mut DBusError);
    pub fn dbus_error_free(err: *mut DBusError);
    pub fn dbus_bus_get_private(bus_type: c_int, err: *mut DBusError) -> *mut DBusConnection;
    pub fn dbus_bus_request_name(c: *mut DBusConnection, name: *const c_char, flags: u32, err: *mut DBusError) -> c_int;
    pub fn dbus_bus_add_match(c: *mut DBusConnection, rule: *const c_char, err: *mut DBusError);
    pub fn dbus_connection_set_exit_on_disconnect(c: *mut DBusConnection, exit: u32);
    pub fn dbus_connection_close(c: *mut DBusConnection);
    pub fn dbus_connection_unref(c: *mut DBusConnection);
    pub fn dbus_connection_flush(c: *mut DBusConnection);
    pub fn dbus_connection_read_write(c: *mut DBusConnection, timeout_ms: c_int) -> u32;
    pub fn dbus_connection_dispatch(c: *mut DBusConnection) -> c_int;
    pub fn dbus_connection_get_unix_fd(c: *mut DBusConnection, fd: *mut c_int) -> u32;
    pub fn dbus_connection_send(c: *mut DBusConnection, msg: *mut DBusMessage, serial: *mut u32) -> u32;
    pub fn dbus_connection_send_with_reply_and_block(c: *mut DBusConnection, msg: *mut DBusMessage, timeout_ms: c_int, err: *mut DBusError) -> *mut DBusMessage;
    pub fn dbus_connection_register_object_path(c: *mut DBusConnection, path: *const c_char, vtable: *const DBusObjectPathVTable, user_data: *mut c_void) -> u32;
    pub fn dbus_connection_add_filter(c: *mut DBusConnection, func: MsgFn, user_data: *mut c_void, free_fn: Option<unsafe extern "C" fn(*mut c_void)>) -> u32;

    pub fn dbus_message_new_method_call(dest: *const c_char, path: *const c_char, iface: *const c_char, method: *const c_char) -> *mut DBusMessage;
    pub fn dbus_message_new_method_return(msg: *mut DBusMessage) -> *mut DBusMessage;
    pub fn dbus_message_new_error(msg: *mut DBusMessage, name: *const c_char, text: *const c_char) -> *mut DBusMessage;
    pub fn dbus_message_new_signal(path: *const c_char, iface: *const c_char, name: *const c_char) -> *mut DBusMessage;
    pub fn dbus_message_unref(msg: *mut DBusMessage);
    pub fn dbus_message_get_type(msg: *mut DBusMessage) -> c_int;
    pub fn dbus_message_get_interface(msg: *mut DBusMessage) -> *const c_char;
    pub fn dbus_message_is_method_call(msg: *mut DBusMessage, iface: *const c_char, method: *const c_char) -> u32;
    pub fn dbus_message_is_signal(msg: *mut DBusMessage, iface: *const c_char, name: *const c_char) -> u32;

    pub fn dbus_message_iter_init(msg: *mut DBusMessage, it: *mut DBusMessageIter) -> u32;
    pub fn dbus_message_iter_init_append(msg: *mut DBusMessage, it: *mut DBusMessageIter);
    pub fn dbus_message_iter_get_arg_type(it: *mut DBusMessageIter) -> c_int;
    pub fn dbus_message_iter_next(it: *mut DBusMessageIter) -> u32;
    pub fn dbus_message_iter_recurse(it: *mut DBusMessageIter, sub: *mut DBusMessageIter);
    pub fn dbus_message_iter_get_basic(it: *mut DBusMessageIter, val: *mut c_void);
    pub fn dbus_message_iter_append_basic(it: *mut DBusMessageIter, arg_type: c_int, val: *const c_void) -> u32;
    pub fn dbus_message_iter_open_container(it: *mut DBusMessageIter, ctype: c_int, sig: *const c_char, sub: *mut DBusMessageIter) -> u32;
    pub fn dbus_message_iter_close_container(it: *mut DBusMessageIter, sub: *mut DBusMessageIter) -> u32;
}

pub struct DbusErr(pub DBusError);

impl DbusErr {
    pub fn new() -> Self {
        let mut e = DBusError { name: null(), message: null(), _pad: [0; 6] };
        unsafe { dbus_error_init(&mut e); }
        Self(e)
    }

    pub fn reset(&mut self) {
        unsafe {
            dbus_error_free(&mut self.0);
            dbus_error_init(&mut self.0);
        }
    }
}

impl Drop for DbusErr {
    fn drop(&mut self) {
        unsafe { dbus_error_free(&mut self.0); }
    }
}

pub struct DbusMsg(pub *mut DBusMessage);

impl DbusMsg {
    pub fn new(p: *mut DBusMessage) -> Option<Self> {
        if p.is_null() { None } else { Some(Self(p)) }
    }
}

impl Drop for DbusMsg {
    fn drop(&mut self) {
        unsafe { dbus_message_unref(self.0); }
    }
}

pub struct FdGuard(pub c_int);

impl Drop for FdGuard {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { close(self.0); }
        }
    }
}

pub fn cstr(s: &str) -> CString {
    if !s.as_bytes().contains(&0) {
        return CString::new(s).unwrap();
    }
    let clean: Vec<u8> = s.bytes().filter(|&b| b != 0).collect();
    CString::new(clean).unwrap()
}

pub fn zero_iter() -> DBusMessageIter {
    DBusMessageIter { _opaque: [0; 16] }
}

pub unsafe fn iter_read_str(it: *mut DBusMessageIter) -> Option<String> {
    let t = dbus_message_iter_get_arg_type(it);
    if t != DBUS_TYPE_STRING && t != DBUS_TYPE_OBJECT_PATH {
        return None;
    }
    let mut p: *const c_char = null();
    dbus_message_iter_get_basic(it, &mut p as *mut *const c_char as *mut c_void);
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

pub unsafe fn append_str(it: *mut DBusMessageIter, s: &str) {
    let cs = cstr(s);
    let ptr = cs.as_ptr();
    dbus_message_iter_append_basic(it, DBUS_TYPE_STRING, &ptr as *const *const c_char as *const c_void);
}

pub unsafe fn append_var_str(it: *mut DBusMessageIter, s: &str) {
    let mut v = zero_iter();
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"s\0".as_ptr() as *const c_char, &mut v);
    append_str(&mut v, s);
    dbus_message_iter_close_container(it, &mut v);
}

pub unsafe fn append_var_obj(it: *mut DBusMessageIter, p: &str) {
    let mut v = zero_iter();
    let cs = cstr(p);
    let ptr = cs.as_ptr();
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"o\0".as_ptr() as *const c_char, &mut v);
    dbus_message_iter_append_basic(&mut v, DBUS_TYPE_OBJECT_PATH, &ptr as *const *const c_char as *const c_void);
    dbus_message_iter_close_container(it, &mut v);
}

pub unsafe fn append_var_bool(it: *mut DBusMessageIter, b: bool) {
    let mut v = zero_iter();
    let db: u32 = u32::from(b);
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"b\0".as_ptr() as *const c_char, &mut v);
    dbus_message_iter_append_basic(&mut v, DBUS_TYPE_BOOLEAN, &db as *const u32 as *const c_void);
    dbus_message_iter_close_container(it, &mut v);
}

pub unsafe fn append_var_u32(it: *mut DBusMessageIter, u: u32) {
    let mut v = zero_iter();
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"u\0".as_ptr() as *const c_char, &mut v);
    dbus_message_iter_append_basic(&mut v, DBUS_TYPE_UINT32, &u as *const u32 as *const c_void);
    dbus_message_iter_close_container(it, &mut v);
}

pub unsafe fn append_var_empty_pixmaps(it: *mut DBusMessageIter) {
    let mut v = zero_iter();
    let mut a = zero_iter();
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"a(iiay)\0".as_ptr() as *const c_char, &mut v);
    dbus_message_iter_open_container(&mut v, DBUS_TYPE_ARRAY, b"(iiay)\0".as_ptr() as *const c_char, &mut a);
    dbus_message_iter_close_container(&mut v, &mut a);
    dbus_message_iter_close_container(it, &mut v);
}

pub unsafe fn append_dict_str(dict: *mut DBusMessageIter, k: &str, val: &str) {
    let mut e = zero_iter();
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, null(), &mut e);
    append_str(&mut e, k);
    append_var_str(&mut e, val);
    dbus_message_iter_close_container(dict, &mut e);
}

pub unsafe fn append_dict_bool(dict: *mut DBusMessageIter, k: &str, val: bool) {
    let mut e = zero_iter();
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, null(), &mut e);
    append_str(&mut e, k);
    append_var_bool(&mut e, val);
    dbus_message_iter_close_container(dict, &mut e);
}

pub unsafe fn drain_conn(c: *mut DBusConnection) {
    if c.is_null() {
        return;
    }
    dbus_connection_read_write(c, 0);
    while dbus_connection_dispatch(c) == DBUS_DISPATCH_DATA_REMAINS {}
    dbus_connection_flush(c);
}

pub unsafe fn send_if_some(conn: *mut DBusConnection, msg: Option<DbusMsg>) {
    if let Some(m) = msg {
        dbus_connection_send(conn, m.0, null_mut());
    }
}