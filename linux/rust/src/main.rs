mod dbus_ffi;
mod device;
mod scanner;
mod sni;

use dbus_ffi::*;
use std::ffi::{c_char, c_int, c_void, CStr};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::ptr::null_mut;
use std::time::{Duration, Instant};

const POLL_FALLBACK: Duration = Duration::from_secs(60);
const DEBOUNCE: Duration = Duration::from_millis(800);

fn acquire_lock() -> Result<Option<File>, ()> {
    let path = match std::env::var("XDG_RUNTIME_DIR") {
        Ok(dir) if dir.starts_with('/') => format!("{}/batt-tray.lock", dir),
        _ => format!("/tmp/batt-tray-{}.lock", unsafe { getuid() }),
    };
    let file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW | O_CLOEXEC)
        .open(&path)
    {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
        return Err(());
    }
    Ok(Some(file))
}

unsafe extern "C" fn sys_filter(_: *mut DBusConnection, msg: *mut DBusMessage, _: *mut c_void) -> c_int {
    if dbus_message_get_type(msg) == DBUS_MESSAGE_TYPE_SIGNAL {
        let p = dbus_message_get_interface(msg);
        if !p.is_null() {
            let iface = CStr::from_ptr(p).to_string_lossy();
            if iface == "org.freedesktop.UPower" || iface == "org.freedesktop.DBus.Properties" {
                sni::schedule_scan(DEBOUNCE);
            }
        }
    }
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED
}

fn main() {
    let hide_offline = std::env::args().skip(1).any(|a| a == "--hide-offline");
    let Ok(_lock_file) = acquire_lock() else {
        return;
    };

    unsafe {
        let mut sig_mask = SigSet { _val: [0; 16] };
        sigemptyset(&mut sig_mask);
        sigaddset(&mut sig_mask, SIGINT);
        sigaddset(&mut sig_mask, SIGTERM);
        sigprocmask(SIG_BLOCK, &sig_mask, null_mut());
        let sig_fd = FdGuard(signalfd(-1, &sig_mask, SFD_NONBLOCK | SFD_CLOEXEC));

        let mut err = DbusErr::new();
        let sys_bus = dbus_bus_get_private(DBUS_BUS_SYSTEM, &mut err.0);
        if !sys_bus.is_null() {
            dbus_connection_set_exit_on_disconnect(sys_bus, 0);
            dbus_bus_add_match(sys_bus, b"type='signal',sender='org.freedesktop.UPower'\0".as_ptr() as *const c_char, null_mut());
            dbus_bus_add_match(sys_bus, b"type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties'\0".as_ptr() as *const c_char, null_mut());
            dbus_connection_add_filter(sys_bus, sys_filter, null_mut(), None);
        }

        let Some(slot_conns) = sni::init_slots(hide_offline) else {
            if !sys_bus.is_null() {
                dbus_connection_close(sys_bus);
                dbus_connection_unref(sys_bus);
            }
            return;
        };

        let mut next_periodic = Instant::now();
        sni::schedule_scan(Duration::ZERO);

        while sni::is_running() {
            let now = Instant::now();
            if sni::is_scan_due(now) || now >= next_periodic {
                sni::clear_scan_due();
                next_periodic = now + POLL_FALLBACK;
                let found = scanner::scan_all_devices(sys_bus);
                sni::update_all_slots(&found);
            }

            let conns = [slot_conns[0], slot_conns[1], slot_conns[2], sys_bus];
            let mut fds = [PollFd { fd: -1, events: POLLIN, revents: 0 }; 5];
            let mut nfds = 0usize;
            let mut sig_idx = None;

            if sig_fd.0 >= 0 {
                sig_idx = Some(nfds);
                fds[nfds] = PollFd { fd: sig_fd.0, events: POLLIN, revents: 0 };
                nfds += 1;
            }
            for &c in &conns {
                let mut fd = -1;
                if !c.is_null() && dbus_connection_get_unix_fd(c, &mut fd) != 0 && fd >= 0 {
                    fds[nfds] = PollFd { fd, events: POLLIN, revents: 0 };
                    nfds += 1;
                }
            }

            let deadline = sni::next_deadline(next_periodic);
            let timeout_ms = deadline.saturating_duration_since(Instant::now()).as_millis().min(60_000) as c_int;

            let rc = poll(fds.as_mut_ptr(), nfds, timeout_ms);
            if rc > 0 {
                if let Some(si) = sig_idx {
                    if (fds[si].revents & POLLIN) != 0 {
                        let mut sink = [0u8; 128];
                        while read(sig_fd.0, sink.as_mut_ptr() as *mut c_void, sink.len()) > 0 {}
                        sni::stop_running();
                        break;
                    }
                }
                for &c in &conns {
                    drain_conn(c);
                }
            }
        }

        sni::shutdown_slots();
        if !sys_bus.is_null() {
            dbus_connection_close(sys_bus);
            dbus_connection_unref(sys_bus);
        }
    }
}