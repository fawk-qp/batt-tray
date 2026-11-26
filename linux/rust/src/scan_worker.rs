use crate::dbus_ffi::*;
use crate::device::DeviceState;
use crate::scanner;
use std::ffi::c_void;
use std::ptr::null_mut;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

struct SharedState {
    stop: bool,
    requested: bool,
    latest: Option<[DeviceState; 3]>,
}

pub struct ScanWorker {
    notify_fd: i32,
    shared: Arc<(Mutex<SharedState>, Condvar)>,
    handle: Option<JoinHandle<()>>,
}

impl ScanWorker {
    pub fn start(allow_airpods_ble: bool) -> Self {
        let notify_fd = unsafe {
            let fd = eventfd(0, 0);
            if fd >= 0 {
                fcntl(fd, F_SETFD, FD_CLOEXEC);
                fcntl(fd, F_SETFL, O_NONBLOCK);
            }
            fd
        };

        let shared = Arc::new((
            Mutex::new(SharedState {
                stop: false,
                requested: false,
                latest: None,
            }),
            Condvar::new(),
        ));

        let shared_bg = Arc::clone(&shared);
        let handle = thread::spawn(move || worker_loop(shared_bg, notify_fd, allow_airpods_ble));

        Self {
            notify_fd,
            shared,
            handle: Some(handle),
        }
    }

    pub fn notify_fd(&self) -> i32 {
        self.notify_fd
    }

    pub fn request_scan(&self) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut st) = lock.lock() {
            if !st.stop {
                st.requested = true;
                cvar.notify_one();
            }
        }
    }

    pub fn take_snapshot(&self) -> Option<[DeviceState; 3]> {
        if self.notify_fd >= 0 {
            let mut buf = 0u64;
            unsafe {
                while read(self.notify_fd, &mut buf as *mut u64 as *mut c_void, 8) > 0 {}
            }
        }
        let (lock, _) = &*self.shared;
        lock.lock().ok().and_then(|mut st| st.latest.take())
    }
}

impl Drop for ScanWorker {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.shared;
        if let Ok(mut st) = lock.lock() {
            st.stop = true;
            cvar.notify_one();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if self.notify_fd >= 0 {
            unsafe { close(self.notify_fd); }
        }
    }
}

fn worker_loop(shared: Arc<(Mutex<SharedState>, Condvar)>, notify_fd: i32, allow_airpods_ble: bool) {
    let mut bus = open_private_sys_bus();
    let (lock, cvar) = &*shared;

    loop {
        {
            let mut st = match lock.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            while !st.stop && !st.requested {
                st = match cvar.wait(st) {
                    Ok(g) => g,
                    Err(_) => return,
                };
            }
            if st.stop {
                break;
            }
            st.requested = false;
        }

        if !bus.is_null() && unsafe { dbus_connection_get_is_connected(bus) } == 0 {
            unsafe {
                dbus_connection_close(bus);
                dbus_connection_unref(bus);
            }
            bus = null_mut();
        }
        if bus.is_null() {
            bus = open_private_sys_bus();
        }

        let snap = scanner::scan_all_devices(bus, allow_airpods_ble);
        {
            let mut st = match lock.lock() {
                Ok(g) => g,
                Err(_) => break,
            };
            if st.stop {
                break;
            }
            st.latest = Some(snap);
        }

        if notify_fd >= 0 {
            let one = 1u64;
            unsafe {
                let _ = write(notify_fd, &one as *const u64 as *const c_void, 8);
            }
        }
    }

    if !bus.is_null() {
        unsafe {
            dbus_connection_close(bus);
            dbus_connection_unref(bus);
        }
    }
}

fn open_private_sys_bus() -> *mut DBusConnection {
    unsafe {
        let mut err = DBusError::default();
        dbus_error_init(&mut err);
        let conn = dbus_bus_get_private(DBUS_BUS_SYSTEM, &mut err);
        dbus_error_free(&mut err);
        if !conn.is_null() {
            dbus_connection_set_exit_on_disconnect(conn, 0);
        }
        conn
    }
}