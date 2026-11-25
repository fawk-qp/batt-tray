use crate::dbus_ffi::*;
use crate::device::{DeviceKind, DeviceState, SLOT_SPECS};
use crate::history::DeviceHistory;
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
    "<property name='XAyatanaLabel' type='s' access='read'/>",
    "<property name='XAyatanaLabelGuide' type='s' access='read'/>",
    "<property name='XAyatanaOrderingIndex' type='u' access='read'/>",
    "<method name='ContextMenu'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='Activate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='SecondaryActivate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>",
    "<method name='Scroll'><arg type='i' direction='in'/><arg type='s' direction='in'/></method>",
    "<signal name='NewTitle'/><signal name='NewIcon'/><signal name='NewOverlayIcon'/>",
    "<signal name='NewToolTip'/><signal name='NewStatus'><arg type='s'/></signal>",
    "<signal name='XAyatanaNewLabel'><arg type='s'/><arg type='s'/></signal>",
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
    "<signal name='LayoutUpdated'><arg type='u'/><arg type='i'/></signal>",
    "</interface><interface name='org.freedesktop.DBus.Properties'>",
    "<method name='Get'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>",
    "<method name='GetAll'><arg type='s' direction='in'/><arg type='a{sv}' direction='out'/></method>",
    "</interface><interface name='org.freedesktop.DBus.Introspectable'>",
    "<method name='Introspect'><arg type='s' direction='out'/></method></interface></node>"
);

#[derive(Clone, PartialEq, Eq)]
struct InactiveMenuRow {
    key: String,
    label: String,
}

#[derive(Clone)]
struct SlotSnapshot {
    id: &'static str,
    icon_name: &'static str,
    tip_title: String,
    tip_desc: String,
    label: String,
    menu_status: String,
    inactive_menu: Vec<InactiveMenuRow>,
    overlay: String,
    status: String,
    menu_revision: u32,
}

struct TraySlot {
    kind: DeviceKind,
    id: &'static str,
    label_prefix: &'static str,
    icon_name: &'static str,
    bus_name: String,
    conn: *mut DBusConnection,
    tip_title: String,
    tip_desc: String,
    label: String,
    menu_status: String,
    inactive_menu: Vec<InactiveMenuRow>,
    overlay: String,
    status: String,
    menu_revision: u32,
}

impl TraySlot {
    fn snapshot(&self) -> SlotSnapshot {
        SlotSnapshot {
            id: self.id,
            icon_name: self.icon_name,
            tip_title: self.tip_title.clone(),
            tip_desc: self.tip_desc.clone(),
            label: self.label.clone(),
            menu_status: self.menu_status.clone(),
            inactive_menu: self.inactive_menu.clone(),
            overlay: self.overlay.clone(),
            status: self.status.clone(),
            menu_revision: self.menu_revision,
        }
    }
}

struct SlotEmit {
    conn: *mut DBusConnection,
    tip_ch: bool,
    lbl_ch: bool,
    menu_ch: bool,
    ov_ch: bool,
    st_ch: bool,
    label: String,
    status: String,
    menu_rev: u32,
}

thread_local! {
    static SLOTS: RefCell<Vec<TraySlot>> = const { RefCell::new(Vec::new()) };
    static STATES: RefCell<[DeviceState; 3]> = RefCell::new(Default::default());
    static HISTORY: RefCell<DeviceHistory> = RefCell::new(DeviceHistory::default());
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

fn build_inactive_menu(kind: DeviceKind, label_prefix: &str, active: &DeviceState) -> Vec<InactiveMenuRow> {
    HISTORY.with(|h| {
        h.borrow()
            .get_inactive_for_kind(kind, &active.items)
            .into_iter()
            .take(10)
            .map(|it| {
                let d_name = if it.name.is_empty() { label_prefix } else { &it.name };
                let lbl = format!(
                    "○ {} — {}% ({})",
                    d_name,
                    it.percent,
                    DeviceHistory::format_relative_time(it.last_seen_unix)
                );
                InactiveMenuRow { key: it.key, label: lbl }
            })
            .collect()
    })
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
        "XAyatanaLabel" => append_var_str(it, &s.label),
        "XAyatanaLabelGuide" => append_var_str(it, ""),
        "XAyatanaOrderingIndex" => append_var_u32(it, 0),
        _ => return false,
    }
    true
}

unsafe fn append_menu_item_props(props: *mut DBusMessageIter, s: &SlotSnapshot, id: i32) {
    match id {
        0 => append_dict_str(props, "children-display", "submenu"),
        10 => {
            append_dict_str(props, "label", &s.menu_status);
            append_dict_bool(props, "enabled", false);
            append_dict_bool(props, "visible", true);
        }
        11 | 21 | 23 | 2 => {
            append_dict_str(props, "type", "separator");
            append_dict_bool(props, "visible", true);
        }
        20 => {
            append_dict_str(props, "label", "История (неактивные)");
            append_dict_str(props, "children-display", "submenu");
            append_dict_bool(props, "enabled", true);
            append_dict_bool(props, "visible", true);
        }
        22 => {
            append_dict_str(props, "label", "Очистить историю");
            append_dict_bool(props, "enabled", true);
            append_dict_bool(props, "visible", true);
        }
        100..=199 => {
            if let Some(row) = s.inactive_menu.get((id - 100) as usize) {
                append_dict_str(props, "label", &row.label);
                append_dict_str(props, "children-display", "submenu");
                append_dict_bool(props, "enabled", true);
                append_dict_bool(props, "visible", true);
            }
        }
        200..=299 => {
            append_dict_str(props, "label", "Удалить из истории");
            append_dict_bool(props, "enabled", true);
            append_dict_bool(props, "visible", true);
        }
        1 => {
            append_dict_str(props, "label", "Обновить сейчас");
            append_dict_bool(props, "enabled", true);
            append_dict_bool(props, "visible", true);
        }
        3 => {
            append_dict_str(props, "label", "Выход");
            append_dict_bool(props, "enabled", true);
            append_dict_bool(props, "visible", true);
        }
        _ => {}
    }
}

unsafe fn append_menu_item(parent: *mut DBusMessageIter, s: &SlotSnapshot, id: i32, as_var: bool) {
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
    append_menu_item_props(&mut props, s, id);
    dbus_message_iter_close_container(&mut st, &mut props);
    dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"v\0".as_ptr() as *const c_char, &mut ch);
    if id == 0 {
        append_menu_item(&mut ch, s, 10, true);
        append_menu_item(&mut ch, s, 11, true);
        if !s.inactive_menu.is_empty() {
            append_menu_item(&mut ch, s, 20, true);
            append_menu_item(&mut ch, s, 23, true);
        }
        append_menu_item(&mut ch, s, 1, true);
        append_menu_item(&mut ch, s, 2, true);
        append_menu_item(&mut ch, s, 3, true);
    } else if id == 20 {
        for j in 0..s.inactive_menu.len() {
            append_menu_item(&mut ch, s, 100 + j as i32, true);
        }
        append_menu_item(&mut ch, s, 21, true);
        append_menu_item(&mut ch, s, 22, true);
    } else if (100..200).contains(&id) {
        let idx = (id - 100) as usize;
        if idx < s.inactive_menu.len() {
            append_menu_item(&mut ch, s, 200 + idx as i32, true);
        }
    }
    dbus_message_iter_close_container(&mut st, &mut ch);
    dbus_message_iter_close_container(target, &mut st);
    if as_var {
        dbus_message_iter_close_container(parent, &mut v);
    }
}

unsafe fn handle_history_action(slot_idx: usize, item_id: i32) {
    let emit_info: Option<(*mut DBusConnection, u32)> = SLOTS.with(|s_cell| {
        let mut slots = s_cell.borrow_mut();
        let slot = slots.get_mut(slot_idx)?;
        let changed = if item_id == 22 {
            STATES.with(|st| {
                let active_items = &st.borrow()[slot_idx].items;
                HISTORY.with(|h| h.borrow_mut().clear_inactive_for_kind(slot.kind, active_items))
            })
        } else if (200..300).contains(&item_id) {
            let idx = (item_id - 200) as usize;
            if let Some(row) = slot.inactive_menu.get(idx) {
                let key = row.key.clone();
                HISTORY.with(|h| h.borrow_mut().remove_entry(slot.kind, &key))
            } else {
                false
            }
        } else {
            false
        };

        if changed {
            let next_menu = STATES.with(|st| {
                let active = &st.borrow()[slot_idx];
                build_inactive_menu(slot.kind, slot.label_prefix, active)
            });
            slot.inactive_menu = next_menu;
            slot.menu_revision = slot.menu_revision.wrapping_add(1).max(1);
            Some((slot.conn, slot.menu_revision))
        } else {
            None
        }
    });

    if let Some((conn, rev)) = emit_info {
        emit_layout_updated(conn, rev);
        dbus_connection_flush(conn);
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
                "XAyatanaLabel", "XAyatanaLabelGuide", "XAyatanaOrderingIndex",
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
        DbusMsg::new(dbus_message_new_method_return(msg))
    } else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };

    send_if_some(conn, rep);
    DBUS_HANDLER_RESULT_HANDLED
}

unsafe extern "C" fn handle_menu(conn: *mut DBusConnection, msg: *mut DBusMessage, user_data: *mut c_void) -> c_int {
    let idx = user_data as usize;
    let snap = SLOTS.with(|s| s.borrow().get(idx).map(TraySlot::snapshot));
    let Some(snap) = snap else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    };

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
            let rev = snap.menu_revision;
            dbus_message_iter_append_basic(&mut it, DBUS_TYPE_UINT32, &rev as *const u32 as *const c_void);
            append_menu_item(&mut it, &snap, 0, false);
            r
        })
    } else if dbus_message_is_method_call(msg, b"com.canonical.dbusmenu\0".as_ptr() as *const c_char, b"GetGroupProperties\0".as_ptr() as *const c_char) != 0 {
        DbusMsg::new(dbus_message_new_method_return(msg)).map(|r| {
            let mut ids: Vec<i32> = vec![10, 11];
            if !snap.inactive_menu.is_empty() {
                ids.push(20);
                for j in 0..snap.inactive_menu.len() {
                    ids.push(100 + j as i32);
                    ids.push(200 + j as i32);
                }
                ids.push(21);
                ids.push(22);
                ids.push(23);
            }
            ids.extend_from_slice(&[1, 2, 3]);

            let mut it = zero_iter();
            let mut arr = zero_iter();
            dbus_message_iter_init_append(r.0, &mut it);
            dbus_message_iter_open_container(&mut it, DBUS_TYPE_ARRAY, b"(ia{sv})\0".as_ptr() as *const c_char, &mut arr);
            for id in ids {
                let mut st = zero_iter();
                let mut props = zero_iter();
                dbus_message_iter_open_container(&mut arr, DBUS_TYPE_STRUCT, null(), &mut st);
                dbus_message_iter_append_basic(&mut st, DBUS_TYPE_INT32, &id as *const i32 as *const c_void);
                dbus_message_iter_open_container(&mut st, DBUS_TYPE_ARRAY, b"{sv}\0".as_ptr() as *const c_char, &mut props);
                append_menu_item_props(&mut props, &snap, id);
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
                        } else if id == 22 || (200..300).contains(&id) {
                            handle_history_action(idx, id);
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

unsafe fn emit_label_sig(conn: *mut DBusConnection, label: &str) {
    let Some(sig) = DbusMsg::new(dbus_message_new_signal(
        b"/StatusNotifierItem\0".as_ptr() as *const c_char,
        b"org.kde.StatusNotifierItem\0".as_ptr() as *const c_char,
        b"XAyatanaNewLabel\0".as_ptr() as *const c_char,
    )) else {
        return;
    };
    let mut it = zero_iter();
    dbus_message_iter_init_append(sig.0, &mut it);
    append_str(&mut it, label);
    append_str(&mut it, "");
    dbus_connection_send(conn, sig.0, null_mut());
}

unsafe fn emit_layout_updated(conn: *mut DBusConnection, rev: u32) {
    let Some(sig) = DbusMsg::new(dbus_message_new_signal(
        b"/MenuBar\0".as_ptr() as *const c_char,
        b"com.canonical.dbusmenu\0".as_ptr() as *const c_char,
        b"LayoutUpdated\0".as_ptr() as *const c_char,
    )) else {
        return;
    };
    let mut it = zero_iter();
    dbus_message_iter_init_append(sig.0, &mut it);
    let parent = 0i32;
    dbus_message_iter_append_basic(&mut it, DBUS_TYPE_UINT32, &rev as *const u32 as *const c_void);
    dbus_message_iter_append_basic(&mut it, DBUS_TYPE_INT32, &parent as *const i32 as *const c_void);
    dbus_connection_send(conn, sig.0, null_mut());
}

fn compute_slot_update(slot: &mut TraySlot, dev: &DeviceState) -> SlotEmit {
    let (new_title, new_desc, new_lbl, new_menu, new_ov, new_st) = if dev.online && dev.percent >= 0 {
        let pct_str = format!("{}%", dev.percent);
        let ch_long = if dev.charging { " (зарядка)" } else { "" };
        let ch_short = if dev.charging { " ⚡" } else { "" };
        let title = format!("{}: {}{}", slot.label_prefix, pct_str, ch_long);
        let lbl = format!("{}{}", pct_str, ch_short);
        let menu = if dev.name.is_empty() {
            title.clone()
        } else {
            format!("{} — {}{}", dev.name, pct_str, ch_short)
        };
        let ov = if !dev.charging && dev.percent <= 20 { "battery-caution-symbolic" } else { "" };
        (title, dev.name.clone(), lbl, menu, ov.to_string(), "Active".to_string())
    } else {
        let title = format!("{}: не подключено", slot.label_prefix);
        let st = if HIDE_OFFLINE.load(Ordering::Relaxed) { "Passive" } else { "Active" };
        (title.clone(), String::new(), String::new(), title, String::new(), st.to_string())
    };

    let new_inactive = build_inactive_menu(slot.kind, slot.label_prefix, dev);

    let tip_ch = new_title != slot.tip_title || new_desc != slot.tip_desc;
    let lbl_ch = new_lbl != slot.label;
    let menu_ch = new_menu != slot.menu_status || new_inactive != slot.inactive_menu;
    let ov_ch = new_ov != slot.overlay;
    let st_ch = new_st != slot.status;

    slot.tip_title = new_title;
    slot.tip_desc = new_desc;
    slot.label = new_lbl.clone();
    slot.menu_status = new_menu;
    slot.inactive_menu = new_inactive;
    slot.overlay = new_ov;
    slot.status = new_st.clone();
    if menu_ch {
        slot.menu_revision = slot.menu_revision.wrapping_add(1).max(1);
    }

    SlotEmit {
        conn: slot.conn,
        tip_ch,
        lbl_ch,
        menu_ch,
        ov_ch,
        st_ch,
        label: new_lbl,
        status: new_st,
        menu_rev: slot.menu_revision,
    }
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

pub unsafe fn init_slots(hide_offline: bool, demo_mode: bool) -> Option<[*mut DBusConnection; 3]> {
    HIDE_OFFLINE.store(hide_offline, Ordering::Relaxed);
    HISTORY.with(|h| h.borrow_mut().load(demo_mode));

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
            kind: spec.kind,
            id: spec.id,
            label_prefix: spec.label,
            icon_name: spec.icon_name,
            bus_name,
            conn,
            tip_title: String::new(),
            tip_desc: String::new(),
            label: String::new(),
            menu_status: String::new(),
            inactive_menu: Vec::new(),
            overlay: String::new(),
            status: "Active".to_string(),
            menu_revision: 1,
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
    STATES.with(|st| *st.borrow_mut() = found.clone());
    HISTORY.with(|h| h.borrow_mut().sync_active(found));

    let updates: Vec<SlotEmit> = SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        (0..3)
            .map(|i| compute_slot_update(&mut slots[i], &found[i]))
            .collect()
    });

    for u in updates {
        let any = u.tip_ch || u.lbl_ch || u.menu_ch || u.ov_ch || u.st_ch;
        if u.tip_ch {
            emit_sig(u.conn, "NewTitle", None);
            emit_sig(u.conn, "NewToolTip", None);
        }
        if u.lbl_ch {
            emit_label_sig(u.conn, &u.label);
        }
        if u.menu_ch {
            emit_layout_updated(u.conn, u.menu_rev);
        }
        if u.ov_ch {
            emit_sig(u.conn, "NewOverlayIcon", None);
            emit_sig(u.conn, "NewIcon", None);
        }
        if u.st_ch {
            emit_sig(u.conn, "NewStatus", Some(&u.status));
        }
        if any {
            dbus_connection_flush(u.conn);
        }
    }
}

pub unsafe fn shutdown_slots() {
    let slots: Vec<TraySlot> = SLOTS.with(|s| std::mem::take(&mut *s.borrow_mut()));
    for s in slots {
        dbus_connection_close(s.conn);
        dbus_connection_unref(s.conn);
    }
}