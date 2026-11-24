use crate::dbus_ffi::*;
use crate::device::{DeviceState, SLOT_SPECS};
use std::cell::RefCell;
use std::ffi::{c_char, c_int, c_void};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static RUNNING: AtomicBool = AtomicBool::new(true);
static HIDE_OFFLINE: AtomicBool = AtomicBool::new(false);

const SNI_XML: &str = concat!(
    "<node><interface name='org.kde.StatusNotifierItem'>",
    "<property name='Category' type='s' access='read'/>",
    "<property name='Id' type='s' access='read'/>",
    "<property name='Title' type='s' access='read'/>",
    "<property name='Status' type='s' access='read'/>",
    "<property name='WindowId' type='u' access='read'/>",
    "<property name='IconName' type='s' access='read'/>",
    "<property name='OverlayIconName' type='s' access='read'/>",
    "<property name='IconPixmap' type='a(iiay)' access='read'/>",
    "<property name='OverlayIconPixmap' type='a(iiay)' access='read'/>",
    "<property name='AttentionIconName' type='s' access='read'/>",
    "<property name='AttentionIconPixmap' type='a(iiay)' access='read'/>",
    "<property name='AttentionMovieName' type='s' access='read'/>",
    "<property name='ToolTip' type='(sa(iiay)ss)' access='read'/>",
    "<property name='ItemIsMenu' type='b' access='read'/>",
    "<property name='Menu' type='o' access='read'/>",
    "<method name='ContextMenu'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='Activate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='SecondaryActivate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='Scroll'><arg type='i' direction='in'/><arg type='s' direction='in'/></method>",
    "<signal name='NewTitle'/><signal name='NewIcon'/><signal name='NewOverlayIcon'/>",
    "<signal name='NewToolTip'/><signal name='NewStatus'><arg type='s'/></signal>",
    "</interface><interface name='org.freedesktop.DBus.Properties'>",
    "<method name='Get'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>",
    "<method name='GetAll'><arg type='s' direction='in'/><arg type='a{sv}' direction='out'/></method>",
    "</interface><interface name='org.freedesktop.DBus.Introspectable'>",
    "<method name='Introspect'><arg type='s' direction='out'/></method></interface></node>"
);

const MENU_XML: &str = concat!(
    "<node><interface name='com.canonical.dbusmenu'>",
    "<property name='Version' type='u' access='read'/>",
    "<property name='TextDirection' type='s' access='read'/>",
    "<property name='Status' type='s' access='read'/>",
    "<method name='GetLayout'><arg type='i' direction='in'/><arg type='i' direction='in'/><arg type='as' direction='in'/>",
    "<arg type='u' direction='out'/><arg type='(ia{sv}av)' direction='out'/></method>",
    "<method name='GetGroupProperties'><arg type='ai' direction='in'/><arg type='as' direction='in'/><arg type='a(ia{sv})' direction='out'/></method>",
    "<method name='Event'><arg type='i' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='in'/><arg type='u' direction='in'/></method>",
    "<method name='AboutToShow'><arg type='i' direction='in'/><arg type='b' direction='out'/></method>",
    "</interface><interface name='org.freedesktop.DBus.Properties'>",
    "<method name='Get'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>",
    "<method name='GetAll'><arg type='s' direction='in'/><arg type='a{sv}' direction='out'/></method>",
    "</interface><interface name='org.freedesktop.DBus.Introspectable'>",
    "<method name='Introspect'><arg type='s' direction='out'/></method></interface></node>"
);

#[derive(Clone)]
struct SlotSnapshot {
    id: &'static str,
    icon_name: &'static str,
    tip_title: String,
    tip_desc: String,
    overlay: String,
    status: String,
}

struct TraySlot {
    id: &'static str,
    label: &'static str,
    icon_name: &'static str,
    bus_name: String,
    conn: *mut DBusConnection,
    tip_title: String,
    tip_desc: String,
    overlay: String,
    status: String,
}

impl TraySlot {
    fn snapshot(&self) -> SlotSnapshot {
        SlotSnapshot {
            id: self.id,
            icon_name: self.icon_name,
            tip_title: self.tip_title.clone(),
            tip_desc: self.tip_desc.clone(),
            overlay: self.overlay.clone(),
            status: self.status.clone(),
        }
    }
}

thread_local! {
    static SLOTS: RefCell<Vec<TraySlot>> = const { RefCell::new(Vec::new()) };
    static SCAN_DUE: RefCell<Option<Instant>> = const { RefCell::new(None) };
}

static SNI_VTABLE: DBusObjectPathVTable = DBusObjectPathVTable {
    unregister_function: None,
    message_function: Some(handle_sni),
    _pad: [0; 4],
};

static MENU_VTABLE: DBusObjectPathVTable = DBusObjectPathVTable {
    unregister_function: None,
    message_function: Some(handle_menu),
    _pad: [0; 4],
};

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

pub fn stop_running() {
    RUNNING.store(false, Ordering::Relaxed);
}

pub fn schedule_scan(delay: Duration) {
    let target = Instant::now() + delay;
    SCAN_DUE.with(|cell| {
        let mut due = cell.borrow_mut();
        match *due {
            Some(existing) if existing <= target => {}
            _ => *due = Some(target),
        }
    });
}

pub fn is_scan_due(now: Instant) -> bool {
    SCAN_DUE.with(|c| c.borrow().map_or(false, |t| now >= t))
}

pub fn clear_scan_due() {
    SCAN_DUE.with(|c| *c.borrow_mut() = None);
}

pub fn next_deadline(fallback: Instant) -> Instant {
    SCAN_DUE.with(|c| c.borrow().map_or(fallback, |t| t.min(fallback)))
}

unsafe fn append_var_tooltip(it: *mut DBusMessageIter, s: &SlotSnapshot) {
    let mut v = zero_iter();
    let mut st = zero_iter();
    let mut pix = zero_iter();
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, b"(sa(iiay)ss)\0".as_ptr() as *const c_char, &mut v);
    dbus_message_iter_open_container(&mut v, DBUS_TYPE_STRUCT, null(), &mut st);
    append_str(&mut st, s.icon_name);
    dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"(iiay)\0".as_ptr() as *const c_char, &mut pix);
    dbus_message_iter_close_container(&mut st, &mut pix);
    append_str(&mut st, &s.tip_title);
    append_str(&mut st, &s.tip_desc);
    dbus_message_iter_close_container(&mut v, &mut st);
    dbus_message_iter_close_container(it, &mut v);
}

unsafe fn append_sni_prop(it: *mut DBusMessageIter, s: &SlotSnapshot, prop: &str) -> bool {
    match prop {
        "Category" => append_var_str(it, "Hardware"),
        "Id" => append_var_str(it, s.id),
        "Title" => append_var_str(it, &s.tip_title),
        "Status" => append_var_str(it, &s.status),
        "WindowId" => append_var_u32(it, 0),
        "IconName" => append_var_str(it, s.icon_name),
        "OverlayIconName" => append_var_str(it, &s.overlay),
        "IconPixmap" | "OverlayIconPixmap" | "AttentionIconPixmap" => append_var_empty_pixmaps(it),
        "AttentionIconName" | "AttentionMovieName" => append_var_str(it, ""),
        "ToolTip" => append_var_tooltip(it, s),
        "ItemIsMenu" => append_var_bool(it, false),
        "Menu" => append_var_obj(it, "/MenuBar"),
        _ => return false,
    }
    true
}

unsafe fn append_menu_item(parent: *mut DBusMessageIter, id: i32, as_var: bool) {
    let mut v = zero_iter();
    let mut st = zero_iter();
    let mut props = zero_iter();
    let mut ch = zero_iter();
    let target = if as_var {
        dbus_message_iter_open_container(parent, DBUS_TYPE_VARIANT, b"(ia{sv}av)\0".as_ptr() as *const c_char, &mut v);
        &mut v as *mut DBusMessageIter
    } else {
        parent
    };
    dbus_message_iter_open_container(target, DBUS_TYPE_STRUCT, null(), &mut st);
    dbus_message_iter_append_basic(&mut st, DBUS_TYPE_INT32, &id as *const i32 as *const c_void);
    dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"{sv}\0".as_ptr() as *const c_char, &mut props);
    match id {
        0 => append_dict_str(&mut props, "children-display", "submenu"),
        1 => {
            append_dict_str(&mut props, "label", "Обновить сейчас");
            append_dict_bool(&mut props, "enabled", true);
            append_dict_bool(&mut props, "visible", true);
        }
        2 => {
            append_dict_str(&mut props, "type", "separator");
            append_dict_bool(&mut props, "visible", true);
        }
        3 => {
            append_dict_str(&mut props, "label", "Выход");
            append_dict_bool(&mut props, "enabled", true);
            append_dict_bool(&mut props, "visible", true);
        }
        _ => {}
    }
    dbus_message_iter_close_container(&mut st, &mut props);
    dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"v\0".as_ptr() as *const c_char, &mut ch);
    if id == 0 {
        append_menu_item(&mut ch, 1, true);
        append_menu_item(&mut ch, 2, true);
        append_menu_item(&mut ch, 3, true);
    }
    dbus_message_iter_close_container(&mut st, &mut ch);
    dbus_message_iter_close_container(target, &mut st);
    if as_var {
        dbus_message_iter_close_container(parent, &mut v);
    }
}

unsafe extern "C" fn handle_sni(conn: *mut DBusConnection, msg: *mut DBusMessage, user_data: *mut c_void) -> c_int {
    let idx = user_data as usize;
    let snap = SLOTS.with(|s| s.borrow().get(idx).map(TraySlot::snapshot));
    let Some(snap) = snap else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };

    let rep = if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Introspectable\0".as_ptr() as *const c_char, b"Introspect\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            append_str(&mut it, SNI_XML);
            r
        })
    } else if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Properties\0".as_ptr() as *const c_char, b"Get\0".as_ptr() as *const c_char) != 0 {
        let mut ait = zero_iter();
        let mut prop = String::new();
        if dbus_message_iter_init(msg, &mut ait) != 0 && dbus_message_iter_next(&mut ait) != 0 {
            if let Some(p) = iter_read_str(&mut ait) {
                prop = p;
            }
        }
        DbusMsg::new(dbus_message_new_method_return(msg)).and_then(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            if append_sni_prop(&mut it, &snap, &prop) {
                Some(r)
            } else {
                drop(r);
                DbusMsg::new(dbus_message_new_error(
                    msg,
                    b"org.freedesktop.DBus.Error.UnknownProperty\0".as_ptr() as *const c_char,
                    b"Unknown\0".as_ptr() as *const c_char,
                ))
            }
        })
    } else if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Properties\0".as_ptr() as *const c_char, b"GetAll\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            let mut dict = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            dbus_message_iter_open_container(&mut it, DBUS_TYPE_ARRAY, b"{sv}\0".as_ptr() as *const c_char, &mut dict);
            for p in [
                "Category", "Id", "Title", "Status", "WindowId", "IconName", "OverlayIconName",
                "IconPixmap", "OverlayIconPixmap", "AttentionIconName", "AttentionIconPixmap",
                "AttentionMovieName", "ToolTip", "ItemIsMenu", "Menu",
            ] {
                let mut e = zero_iter();
                dbus_message_iter_open_container(&mut dict, DBUS_TYPE_DICT_ENTRY, null(), &mut e);
                append_str(&mut e, p);
                append_sni_prop(&mut e, &snap, p);
                dbus_message_iter_close_container(&mut dict, &mut e);
            }
            dbus_message_iter_close_container(&mut it, &mut dict);
            r
        })
    } else if dbus_message_is_method_call(msg, b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char, b"Activate\0".as_ptr() as *const c_char) != 0
        || dbus_message_is_method_call(msg, b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char, b"SecondaryActivate\0".as_ptr() as *const c_char) != 0
        || dbus_message_is_method_call(msg, b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char, b"ContextMenu\0".as_ptr() as *const c_char) != 0
        || dbus_message_is_method_call(msg, b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char, b"Scroll\0".as_ptr() as *const c_char) != 0
    {
        // LMB/scroll intentionally return empty reply; status is hover-only.
        DbusMsg::new(dbus_message_new_method_return(msg))
    } else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };

    send_if_some(conn, rep);
    DBUS_HANDLER_RESULT_HANDLED
}

unsafe extern "C" fn handle_menu(conn: *mut DBusConnection, msg: *mut DBusMessage, _: *mut c_void) -> c_int {
    let rep = if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Introspectable\0".as_ptr() as *const c_char, b"Introspect\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            append_str(&mut it, MENU_XML);
            r
        })
    } else if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Properties\0".as_ptr() as *const c_char, b"Get\0".as_ptr() as *const c_char) != 0 {
        let mut ait = zero_iter();
        let mut prop = String::new();
        if dbus_message_iter_init(msg, &mut ait) != 0 && dbus_message_iter_next(&mut ait) != 0 {
            if let Some(p) = iter_read_str(&mut ait) {
                prop = p;
            }
        }
        DbusMsg::new(dbus_message_new_method_return(msg)).and_then(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            match prop.as_str() {
                "Version" => { append_var_u32(&mut it, 3); Some(r) }
                "TextDirection" => { append_var_str(&mut it, "ltr"); Some(r) }
                "Status" => { append_var_str(&mut it, "normal"); Some(r) }
                _ => {
                    drop(r);
                    DbusMsg::new(dbus_message_new_error(
                        msg,
                        b"org.freedesktop.DBus.Error.UnknownProperty\0".as_ptr() as *const c_char,
                        b"Unknown\0".as_ptr() as *const c_char,
                    ))
                }
            }
        })
    } else if dbus_message_is_method_call(msg, b"org.freedesktop.DBus.Properties\0".as_ptr() as *const c_char, b"GetAll\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            let mut dict = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            dbus_message_iter_open_container(&mut it, DBUS_TYPE_ARRAY, b"{sv}\0".as_ptr() as *const c_char, &mut dict);
            let mut e = zero_iter();
            dbus_message_iter_open_container(&mut dict, DBUS_TYPE_DICT_ENTRY, null(), &mut e);
            append_str(&mut e, "Version");
            append_var_u32(&mut e, 3);
            dbus_message_iter_close_container(&mut dict, &mut e);
            append_dict_str(&mut dict, "TextDirection", "ltr");
            append_dict_str(&mut dict, "Status", "normal");
            dbus_message_iter_close_container(&mut it, &mut dict);
            r
        })
    } else if dbus_message_is_method_call(msg, b"com.canonical.dbusmenu\0".as_ptr() as *const c_char, b"GetLayout\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            let rev = 1u32;
            dbus_message_iter_append_basic(&mut it, DBUS_TYPE_UINT32, &rev as *const u32 as *const c_void);
            append_menu_item(&mut it, 0, false);
            r
        })
    } else if dbus_message_is_method_call(msg, b"com.canonical.dbusmenu\0".as_ptr() as *const c_char, b"GetGroupProperties\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            let mut arr = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            dbus_message_iter_open_container(&mut it, DBUS_TYPE_ARRAY, b"(ia{sv})\0".as_ptr() as *const c_char, &mut arr);
            for id in [1i32, 2, 3] {
                let mut st = zero_iter();
                let mut props = zero_iter();
                dbus_message_iter_open_container(&mut arr, DBUS_TYPE_STRUCT, null(), &mut st);
                dbus_message_iter_append_basic(&mut st, DBUS_TYPE_INT32, &id as *const i32 as *const c_void);
                dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"{sv}\0".as_ptr() as *const c_char, &mut props);
                if id == 1 {
                    append_dict_str(&mut props, "label", "Обновить сейчас");
                    append_dict_bool(&mut props, "enabled", true);
                    append_dict_bool(&mut props, "visible", true);
                } else if id == 2 {
                    append_dict_str(&mut props, "type", "separator");
                    append_dict_bool(&mut props, "visible", true);
                } else {
                    append_dict_str(&mut props, "label", "Выход");
                    append_dict_bool(&mut props, "enabled", true);
                    append_dict_bool(&mut props, "visible", true);
                }
                dbus_message_iter_close_container(&mut st, &mut props);
                dbus_message_iter_close_container(&mut arr, &mut st);
            }
            dbus_message_iter_close_container(&mut it, &mut arr);
            r
        })
    } else if dbus_message_is_method_call(msg, b"com.canonical.dbusmenu\0".as_ptr() as *const c_char, b"AboutToShow\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut it = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            let b = 0u32;
            dbus_message_iter_append_basic(&mut it, DBUS_TYPE_BOOLEAN, &b as *const u32 as *const c_void);
            r
        })
    } else if dbus_message_is_method_call(msg, b"com.canonical.dbusmenu\0".as_ptr() as *const c_char, b"Event\0".as_ptr() as *const c_char) != 0 {
        let mut it = zero_iter();
        if dbus_message_iter_init(msg, &mut it) != 0 && dbus_message_iter_get_arg_type(&mut it) == DBUS_TYPE_INT32 {
            let mut id = 0i32;
            dbus_message_iter_get_basic(&mut it, &mut id as *mut i32 as *mut c_void);
            if dbus_message_iter_next(&mut it) != 0 {
                if let Some(ev) = iter_read_str(&mut it) {
                    if ev == "clicked" {
                        if id == 1 {
                            schedule_scan(Duration::ZERO);
                        } else if id == 3 {
                            stop_running();
                        }
                    }
                }
            }
        }
        DbusMsg::new(dbus_message_new_method_return(msg))
    } else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };

    send_if_some(conn, rep);
    DBUS_HANDLER_RESULT_HANDLED
}

unsafe fn register_watcher(conn: *mut DBusConnection, bus_name: &str) {
    let Some(m) = DbusMsg::new(dbus_message_new_method_call(
        b"org.kde.StatusNotifierWatcher\0".as_ptr() as *const c_char,
        b"/StatusNotifierWatcher\0".as_ptr() as *const c_char,
        b"org.kde.StatusNotifierWatcher\0".as_ptr() as *const c_char,
        b"RegisterStatusNotifierItem\0".as_ptr() as *const c_char,
    )) else {
        return;
    };
    let mut it = zero_iter();
    dbus_message_iter_init_append(m.0, &mut it);
    append_str(&mut it, bus_name);
    dbus_connection_send(conn, m.0, null_mut());
    dbus_connection_flush(conn);
}

unsafe fn emit_sig(conn: *mut DBusConnection, name: &str, arg: Option<&str>) {
    let cn = cstr(name);
    let Some(sig) = DbusMsg::new(dbus_message_new_signal(
        b"/StatusNotifierItem\0".as_ptr() as *const c_char,
        b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char,
        cn.as_ptr(),
    )) else {
        return;
    };
    if let Some(a) = arg {
        let mut it = zero_iter();
        dbus_message_iter_init_append(sig.0, &mut it);
        append_str(&mut it, a);
    }
    dbus_connection_send(conn, sig.0, null_mut());
}

fn compute_slot_update(slot: &mut TraySlot, dev: &DeviceState) -> (bool, bool, bool, String) {
    let (new_title, new_desc, new_ov, new_st) = if dev.online && dev.percent >= 0 {
        let ch = if dev.charging { " (зарядка)" } else { "" };
        let ov = if !dev.charging && dev.percent <= 20 { "battery-caution-symbolic" } else { "" };
        (format!("{}: {}%{}", slot.label, dev.percent, ch), dev.name.clone(), ov.to_string(), "Active".to_string())
    } else {
        let st = if HIDE_OFFLINE.load(Ordering::Relaxed) { "Passive" } else { "Active" };
        (format!("{}: не подключено", slot.label), String::new(), String::new(), st.to_string())
    };

    let tip_ch = new_title != slot.tip_title || new_desc != slot.tip_desc;
    let ov_ch = new_ov != slot.overlay;
    let st_ch = new_st != slot.status;

    slot.tip_title = new_title;
    slot.tip_desc = new_desc;
    slot.overlay = new_ov;
    slot.status = new_st.clone();
    (tip_ch, ov_ch, st_ch, new_st)
}

unsafe extern "C" fn watcher_filter(_: *mut DBusConnection, msg: *mut DBusMessage, _: *mut c_void) -> c_int {
    if dbus_message_is_signal(msg, b"org.freedesktop.DBus\0".as_ptr() as *const c_char, b"NameOwnerChanged\0".as_ptr() as *const c_char) != 0 {
        let mut it = zero_iter();
        if dbus_message_iter_init(msg, &mut it) != 0 {
            let name = iter_read_str(&mut it).unwrap_or_default();
            if dbus_message_iter_next(&mut it) != 0 && dbus_message_iter_next(&mut it) != 0 {
                let new_owner = iter_read_str(&mut it).unwrap_or_default();
                if name == "org.kde.StatusNotifierWatcher" && !new_owner.is_empty() {
                    let targets: Vec<(*mut DBusConnection, String)> = SLOTS.with(|s| {
                        s.borrow().iter().map(|slot| (slot.conn, slot.bus_name.clone())).collect()
                    });
                    for (c, bn) in targets {
                        register_watcher(c, &bn);
                    }
                }
            }
        }
    }
    DBUS_HANDLER_RESULT_NOT_YET_HANDLED
}

pub unsafe fn init_slots(hide_offline: bool) -> Option<[*mut DBusConnection; 3]> {
    HIDE_OFFLINE.store(hide_offline, Ordering::Relaxed);
    let pid = getpid();
    let mut err = DbusErr::new();
    let mut init_slots: Vec<TraySlot> = Vec::with_capacity(3);

    for (i, spec) in SLOT_SPECS.iter().enumerate() {
        err.reset();
        let conn = dbus_bus_get_private(DBUS_BUS_SESSION, &mut err.0);
        if conn.is_null() {
            for s in init_slots {
                dbus_connection_close(s.conn);
                dbus_connection_unref(s.conn);
            }
            return None;
        }
        dbus_connection_set_exit_on_disconnect(conn, 0);

        let bus_name = format!("org.kde.StatusNotifierItem-{}-{}", pid, i + 1);
        let cbn = cstr(&bus_name);
        err.reset();
        dbus_bus_request_name(conn, cbn.as_ptr(), DBUS_NAME_FLAG_DO_NOT_QUEUE, &mut err.0);

        dbus_connection_register_object_path(conn, b"/StatusNotifierItem\0".as_ptr() as *const c_char, &SNI_VTABLE, i as *mut c_void);
        dbus_connection_register_object_path(conn, b"/MenuBar\0".as_ptr() as *const c_char, &MENU_VTABLE, i as *mut c_void);

        if i == 0 {
            dbus_bus_add_match(
                conn,
                b"type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.StatusNotifierWatcher'\0".as_ptr() as *const c_char,
                null_mut(),
            );
            dbus_connection_add_filter(conn, watcher_filter, null_mut(), None);
        }

        let mut slot = TraySlot {
            id: spec.id,
            label: spec.label,
            icon_name: spec.icon_name,
            bus_name,
            conn,
            tip_title: String::new(),
            tip_desc: String::new(),
            overlay: String::new(),
            status: "Active".to_string(),
        };
        let _ = compute_slot_update(&mut slot, &DeviceState::default());
        register_watcher(slot.conn, &slot.bus_name);
        init_slots.push(slot);
    }

    let conns = [init_slots[0].conn, init_slots[1].conn, init_slots[2].conn];
    SLOTS.with(|s| *s.borrow_mut() = init_slots);
    Some(conns)
}

pub unsafe fn update_all_slots(found: &[DeviceState; 3]) {
    let updates: Vec<(*mut DBusConnection, bool, bool, bool, String)> = SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        (0..3)
            .map(|i| {
                let (tip_ch, ov_ch, st_ch, st) = compute_slot_update(&mut slots[i], &found[i]);
                (slots[i].conn, tip_ch, ov_ch, st_ch, st)
            })
            .collect()
    });

    for (conn, tip_ch, ov_ch, st_ch, st) in updates {
        if tip_ch {
            emit_sig(conn, "NewTitle", None);
            emit_sig(conn, "NewToolTip", None);
        }
        if ov_ch {
            emit_sig(conn, "NewOverlayIcon", None);
            emit_sig(conn, "NewIcon", None);
        }
        if st_ch {
            emit_sig(conn, "NewStatus", Some(&st));
        }
        dbus_connection_flush(conn);
    }
}

pub unsafe fn shutdown_slots() {
    let slots = SLOTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    for s in slots {
        dbus_connection_close(s.conn);
        dbus_connection_unref(s.conn);
    }
}