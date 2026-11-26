#include "sni.hpp"

#include <time.h>
#include <unistd.h>

#include <algorithm>
#include <cstring>

namespace {

struct DbusErr {
    DBusError raw{};
    DbusErr() { dbus_error_init(&raw); }
    ~DbusErr() { dbus_error_free(&raw); }
    DbusErr(const DbusErr&) = delete;
    DbusErr& operator=(const DbusErr&) = delete;
    void Reset() { dbus_error_free(&raw); dbus_error_init(&raw); }
};

struct DbusMsg {
    DBusMessage* p = nullptr;
    explicit DbusMsg(DBusMessage* m = nullptr) : p(m) {}
    ~DbusMsg() { if (p) dbus_message_unref(p); }
    DbusMsg(const DbusMsg&) = delete;
    DbusMsg& operator=(const DbusMsg&) = delete;
    explicit operator bool() const { return p != nullptr; }
};

constexpr const char* kSniXml =
    "<node><interface name='org.kde.StatusNotifierItem'>"
    "<property name='Category' type='s' access='read'/>"
    "<property name='Id' type='s' access='read'/>"
    "<property name='Title' type='s' access='read'/>"
    "<property name='Status' type='s' access='read'/>"
    "<property name='WindowId' type='u' access='read'/>"
    "<property name='IconName' type='s' access='read'/>"
    "<property name='OverlayIconName' type='s' access='read'/>"
    "<property name='IconPixmap' type='a(iiay)' access='read'/>"
    "<property name='OverlayIconPixmap' type='a(iiay)' access='read'/>"
    "<property name='AttentionIconName' type='s' access='read'/>"
    "<property name='AttentionIconPixmap' type='a(iiay)' access='read'/>"
    "<property name='AttentionMovieName' type='s' access='read'/>"
    "<property name='ToolTip' type='(sa(iiay)ss)' access='read'/>"
    "<property name='ItemIsMenu' type='b' access='read'/>"
    "<property name='Menu' type='o' access='read'/>"
    "<property name='XAyatanaLabel' type='s' access='read'/>"
    "<property name='XAyatanaLabelGuide' type='s' access='read'/>"
    "<property name='XAyatanaOrderingIndex' type='u' access='read'/>"
    "<method name='ContextMenu'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='Activate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='SecondaryActivate'><arg type='i' direction='in'/><arg type='i' direction='in'/></method>"
    "<method name='Scroll'><arg type='i' direction='in'/><arg type='s' direction='in'/></method>"
    "<signal name='NewTitle'/><signal name='NewIcon'/><signal name='NewOverlayIcon'/>"
    "<signal name='NewToolTip'/><signal name='NewStatus'><arg type='s'/></signal>"
    "<signal name='XAyatanaNewLabel'><arg type='s'/><arg type='s'/></signal>"
    "</interface><interface name='org.freedesktop.DBus.Properties'>"
    "<method name='Get'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>"
    "<method name='GetAll'><arg type='s' direction='in'/><arg type='a{sv}' direction='out'/></method>"
    "</interface><interface name='org.freedesktop.DBus.Introspectable'>"
    "<method name='Introspect'><arg type='s' direction='out'/></method></interface></node>";

constexpr const char* kMenuXml =
    "<node><interface name='com.canonical.dbusmenu'>"
    "<property name='Version' type='u' access='read'/>"
    "<property name='TextDirection' type='s' access='read'/>"
    "<property name='Status' type='s' access='read'/>"
    "<method name='GetLayout'><arg type='i' direction='in'/><arg type='i' direction='in'/><arg type='as' direction='in'/>"
    "<arg type='u' direction='out'/><arg type='(ia{sv}av)' direction='out'/></method>"
    "<method name='GetGroupProperties'><arg type='ai' direction='in'/><arg type='as' direction='in'/><arg type='a(ia{sv})' direction='out'/></method>"
    "<method name='Event'><arg type='i' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='in'/><arg type='u' direction='in'/></method>"
    "<method name='AboutToShow'><arg type='i' direction='in'/><arg type='b' direction='out'/></method>"
    "<signal name='LayoutUpdated'><arg type='u'/><arg type='i'/></signal>"
    "</interface><interface name='org.freedesktop.DBus.Properties'>"
    "<method name='Get'><arg type='s' direction='in'/><arg type='s' direction='in'/><arg type='v' direction='out'/></method>"
    "<method name='GetAll'><arg type='s' direction='in'/><arg type='a{sv}' direction='out'/></method>"
    "</interface><interface name='org.freedesktop.DBus.Introspectable'>"
    "<method name='Introspect'><arg type='s' direction='out'/></method></interface></node>";

void AppendVariantStr(DBusMessageIter* it, const char* s) {
    DBusMessageIter v;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "s", &v);
    dbus_message_iter_append_basic(&v, DBUS_TYPE_STRING, &s);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantObjPath(DBusMessageIter* it, const char* p) {
    DBusMessageIter v;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "o", &v);
    dbus_message_iter_append_basic(&v, DBUS_TYPE_OBJECT_PATH, &p);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantBool(DBusMessageIter* it, bool b) {
    dbus_bool_t db = b ? 1 : 0;
    DBusMessageIter v;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "b", &v);
    dbus_message_iter_append_basic(&v, DBUS_TYPE_BOOLEAN, &db);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantI32(DBusMessageIter* it, dbus_int32_t val) {
    DBusMessageIter v;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "i", &v);
    dbus_message_iter_append_basic(&v, DBUS_TYPE_INT32, &val);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantU32(DBusMessageIter* it, uint32_t u) {
    dbus_uint32_t du = u;
    DBusMessageIter v;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "u", &v);
    dbus_message_iter_append_basic(&v, DBUS_TYPE_UINT32, &du);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantEmptyPixmaps(DBusMessageIter* it) {
    DBusMessageIter v, a;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "a(iiay)", &v);
    dbus_message_iter_open_container(&v, DBUS_TYPE_ARRAY, "(iiay)", &a);
    dbus_message_iter_close_container(&v, &a);
    dbus_message_iter_close_container(it, &v);
}

void AppendVariantToolTip(DBusMessageIter* it, const SniManager::Slot& s) {
    DBusMessageIter v, st, pix;
    dbus_message_iter_open_container(it, DBUS_TYPE_VARIANT, "(sa(iiay)ss)", &v);
    dbus_message_iter_open_container(&v, DBUS_TYPE_STRUCT, nullptr, &st);
    const char* ic = s.spec->iconName;
    const char* tt = s.tipTitle.c_str();
    const char* td = s.tipDesc.c_str();
    dbus_message_iter_append_basic(&st, DBUS_TYPE_STRING, &ic);
    dbus_message_iter_open_container(&st, DBUS_TYPE_ARRAY, "(iiay)", &pix);
    dbus_message_iter_close_container(&st, &pix);
    dbus_message_iter_append_basic(&st, DBUS_TYPE_STRING, &tt);
    dbus_message_iter_append_basic(&st, DBUS_TYPE_STRING, &td);
    dbus_message_iter_close_container(&v, &st);
    dbus_message_iter_close_container(it, &v);
}

void AppendDictStr(DBusMessageIter* dict, const char* k, const char* val) {
    DBusMessageIter e;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
    dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &k);
    AppendVariantStr(&e, val);
    dbus_message_iter_close_container(dict, &e);
}

void AppendDictBool(DBusMessageIter* dict, const char* k, bool val) {
    DBusMessageIter e;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
    dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &k);
    AppendVariantBool(&e, val);
    dbus_message_iter_close_container(dict, &e);
}

void AppendDictI32(DBusMessageIter* dict, const char* k, dbus_int32_t val) {
    DBusMessageIter e;
    dbus_message_iter_open_container(dict, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
    dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &k);
    AppendVariantI32(&e, val);
    dbus_message_iter_close_container(dict, &e);
}

bool AppendSniProp(DBusMessageIter* it, const SniManager::Slot& s, const char* prop) {
    if (std::strcmp(prop, "Category") == 0)              { AppendVariantStr(it, "Hardware"); return true; }
    if (std::strcmp(prop, "Id") == 0)                    { AppendVariantStr(it, s.spec->id); return true; }
    if (std::strcmp(prop, "Title") == 0)                 { AppendVariantStr(it, s.tipTitle.c_str()); return true; }
    if (std::strcmp(prop, "Status") == 0)                { AppendVariantStr(it, s.status.c_str()); return true; }
    if (std::strcmp(prop, "WindowId") == 0)              { AppendVariantU32(it, 0); return true; }
    if (std::strcmp(prop, "IconName") == 0)              { AppendVariantStr(it, s.spec->iconName); return true; }
    if (std::strcmp(prop, "OverlayIconName") == 0)       { AppendVariantStr(it, s.overlay.c_str()); return true; }
    if (std::strcmp(prop, "IconPixmap") == 0)            { AppendVariantEmptyPixmaps(it); return true; }
    if (std::strcmp(prop, "OverlayIconPixmap") == 0)     { AppendVariantEmptyPixmaps(it); return true; }
    if (std::strcmp(prop, "AttentionIconName") == 0)     { AppendVariantStr(it, ""); return true; }
    if (std::strcmp(prop, "AttentionIconPixmap") == 0)   { AppendVariantEmptyPixmaps(it); return true; }
    if (std::strcmp(prop, "AttentionMovieName") == 0)    { AppendVariantStr(it, ""); return true; }
    if (std::strcmp(prop, "ToolTip") == 0)               { AppendVariantToolTip(it, s); return true; }
    if (std::strcmp(prop, "ItemIsMenu") == 0)            { AppendVariantBool(it, false); return true; }
    if (std::strcmp(prop, "Menu") == 0)                  { AppendVariantObjPath(it, "/MenuBar"); return true; }
    if (std::strcmp(prop, "XAyatanaLabel") == 0)         { AppendVariantStr(it, s.label.c_str()); return true; }
    if (std::strcmp(prop, "XAyatanaLabelGuide") == 0)    { AppendVariantStr(it, ""); return true; }
    if (std::strcmp(prop, "XAyatanaOrderingIndex") == 0) { AppendVariantU32(it, 0); return true; }
    return false;
}

void AppendMenuItemProps(DBusMessageIter* props, const SniManager::Slot& s, dbus_int32_t id) {
    if (id == 0) {
        AppendDictStr(props, "children-display", "submenu");
    } else if (id == 10) {
        AppendDictStr(props, "label", s.menuStatus.c_str());
        AppendDictBool(props, "enabled", false);
        AppendDictBool(props, "visible", true);
    } else if (id == 11 || id == 21 || id == 23 || id == 2) {
        AppendDictStr(props, "type", "separator");
        AppendDictBool(props, "visible", true);
    } else if (id == 20) {
        AppendDictStr(props, "label", "История (неактивные)");
        AppendDictStr(props, "children-display", "submenu");
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    } else if (id == 22) {
        AppendDictStr(props, "label", "Очистить историю");
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    } else if (id >= 100 && id < 200) {
        const size_t idx = static_cast<size_t>(id - 100);
        if (idx < s.inactiveMenu.size()) {
            AppendDictStr(props, "label", s.inactiveMenu[idx].label.c_str());
            AppendDictStr(props, "children-display", "submenu");
            AppendDictBool(props, "enabled", true);
            AppendDictBool(props, "visible", true);
        }
    } else if (id >= 200 && id < 300) {
        AppendDictStr(props, "label", "Удалить из истории");
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    } else if (id == 1) {
        AppendDictStr(props, "label", "Обновить сейчас");
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    } else if (id == 4) {
        AppendDictStr(props, "label", "Уведомлять при <15%");
        AppendDictStr(props, "toggle-type", "checkmark");
        AppendDictI32(props, "toggle-state", (s.owner && s.owner->IsNotifyEnabled()) ? 1 : 0);
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    } else if (id == 3) {
        AppendDictStr(props, "label", "Выход");
        AppendDictBool(props, "enabled", true);
        AppendDictBool(props, "visible", true);
    }
}

void AppendMenuItem(DBusMessageIter* parent, const SniManager::Slot& s, dbus_int32_t id, bool asVariant) {
    DBusMessageIter v, st, props, children;
    DBusMessageIter* target = parent;
    if (asVariant) {
        dbus_message_iter_open_container(parent, DBUS_TYPE_VARIANT, "(ia{sv}av)", &v);
        target = &v;
    }
    dbus_message_iter_open_container(target, DBUS_TYPE_STRUCT, nullptr, &st);
    dbus_message_iter_append_basic(&st, DBUS_TYPE_INT32, &id);
    dbus_message_iter_open_container(&st, DBUS_TYPE_ARRAY, "{sv}", &props);
    AppendMenuItemProps(&props, s, id);
    dbus_message_iter_close_container(&st, &props);
    dbus_message_iter_open_container(&st, DBUS_TYPE_ARRAY, "v", &children);
    if (id == 0) {
        AppendMenuItem(&children, s, 10, true);
        AppendMenuItem(&children, s, 11, true);
        if (!s.inactiveMenu.empty()) {
            AppendMenuItem(&children, s, 20, true);
            AppendMenuItem(&children, s, 23, true);
        }
        AppendMenuItem(&children, s, 1, true);
        AppendMenuItem(&children, s, 4, true);
        AppendMenuItem(&children, s, 2, true);
        AppendMenuItem(&children, s, 3, true);
    } else if (id == 20) {
        for (size_t j = 0; j < s.inactiveMenu.size(); ++j) {
            AppendMenuItem(&children, s, static_cast<dbus_int32_t>(100 + j), true);
        }
        AppendMenuItem(&children, s, 21, true);
        AppendMenuItem(&children, s, 22, true);
    } else if (id >= 100 && id < 200) {
        const size_t idx = static_cast<size_t>(id - 100);
        if (idx < s.inactiveMenu.size()) {
            AppendMenuItem(&children, s, static_cast<dbus_int32_t>(200 + idx), true);
        }
    }
    dbus_message_iter_close_container(&st, &children);
    dbus_message_iter_close_container(target, &st);
    if (asVariant) dbus_message_iter_close_container(parent, &v);
}

DBusHandlerResult HandleSni(DBusConnection* conn, DBusMessage* msg, void* data) {
    const auto& s = *static_cast<const SniManager::Slot*>(data);
    DbusMsg rep;

    if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Introspectable", "Introspect")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            const char* xml = kSniXml;
            dbus_message_append_args(rep.p, DBUS_TYPE_STRING, &xml, DBUS_TYPE_INVALID);
        }
    } else if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Properties", "Get")) {
        const char *iface = nullptr, *prop = nullptr;
        if (dbus_message_get_args(msg, nullptr, DBUS_TYPE_STRING, &iface,
                                  DBUS_TYPE_STRING, &prop, DBUS_TYPE_INVALID) && prop) {
            rep.p = dbus_message_new_method_return(msg);
            if (rep) {
                DBusMessageIter it;
                dbus_message_iter_init_append(rep.p, &it);
                if (!AppendSniProp(&it, s, prop)) {
                    dbus_message_unref(rep.p);
                    rep.p = dbus_message_new_error(msg, "org.freedesktop.DBus.Error.UnknownProperty", "Unknown");
                }
            }
        }
    } else if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Properties", "GetAll")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            DBusMessageIter it, dict;
            dbus_message_iter_init_append(rep.p, &it);
            dbus_message_iter_open_container(&it, DBUS_TYPE_ARRAY, "{sv}", &dict);
            static constexpr const char* kProps[] = {
                "Category", "Id", "Title", "Status", "WindowId", "IconName", "OverlayIconName",
                "IconPixmap", "OverlayIconPixmap", "AttentionIconName", "AttentionIconPixmap",
                "AttentionMovieName", "ToolTip", "ItemIsMenu", "Menu",
                "XAyatanaLabel", "XAyatanaLabelGuide", "XAyatanaOrderingIndex"
            };
            for (const char* p : kProps) {
                DBusMessageIter e;
                dbus_message_iter_open_container(&dict, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
                dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &p);
                AppendSniProp(&e, s, p);
                dbus_message_iter_close_container(&dict, &e);
            }
            dbus_message_iter_close_container(&it, &dict);
        }
    } else if (dbus_message_is_method_call(msg, "org.kde.StatusNotifierItem", "Activate") ||
               dbus_message_is_method_call(msg, "org.kde.StatusNotifierItem", "SecondaryActivate") ||
               dbus_message_is_method_call(msg, "org.kde.StatusNotifierItem", "ContextMenu") ||
               dbus_message_is_method_call(msg, "org.kde.StatusNotifierItem", "Scroll")) {
        rep.p = dbus_message_new_method_return(msg);
    } else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    if (rep) dbus_connection_send(conn, rep.p, nullptr);
    return DBUS_HANDLER_RESULT_HANDLED;
}

DBusHandlerResult HandleMenu(DBusConnection* conn, DBusMessage* msg, void* data) {
    auto* slot = static_cast<SniManager::Slot*>(data);
    if (!slot) return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    DbusMsg rep;

    if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Introspectable", "Introspect")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            const char* xml = kMenuXml;
            dbus_message_append_args(rep.p, DBUS_TYPE_STRING, &xml, DBUS_TYPE_INVALID);
        }
    } else if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Properties", "Get")) {
        const char *iface = nullptr, *prop = nullptr;
        if (dbus_message_get_args(msg, nullptr, DBUS_TYPE_STRING, &iface,
                                  DBUS_TYPE_STRING, &prop, DBUS_TYPE_INVALID) && prop) {
            rep.p = dbus_message_new_method_return(msg);
            if (rep) {
                DBusMessageIter it;
                dbus_message_iter_init_append(rep.p, &it);
                if (std::strcmp(prop, "Version") == 0) AppendVariantU32(&it, 3);
                else if (std::strcmp(prop, "TextDirection") == 0) AppendVariantStr(&it, "ltr");
                else if (std::strcmp(prop, "Status") == 0) AppendVariantStr(&it, "normal");
                else {
                    dbus_message_unref(rep.p);
                    rep.p = dbus_message_new_error(msg, "org.freedesktop.DBus.Error.UnknownProperty", "Unknown");
                }
            }
        }
    } else if (dbus_message_is_method_call(msg, "org.freedesktop.DBus.Properties", "GetAll")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            DBusMessageIter it, dict, e;
            dbus_message_iter_init_append(rep.p, &it);
            dbus_message_iter_open_container(&it, DBUS_TYPE_ARRAY, "{sv}", &dict);
            const char* k = "Version";
            dbus_message_iter_open_container(&dict, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
            dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &k);
            AppendVariantU32(&e, 3);
            dbus_message_iter_close_container(&dict, &e);
            AppendDictStr(&dict, "TextDirection", "ltr");
            AppendDictStr(&dict, "Status", "normal");
            dbus_message_iter_close_container(&it, &dict);
        }
    } else if (dbus_message_is_method_call(msg, "com.canonical.dbusmenu", "GetLayout")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            DBusMessageIter it;
            dbus_message_iter_init_append(rep.p, &it);
            dbus_uint32_t rev = slot->menuRevision;
            dbus_message_iter_append_basic(&it, DBUS_TYPE_UINT32, &rev);
            AppendMenuItem(&it, *slot, 0, false);
        }
    } else if (dbus_message_is_method_call(msg, "com.canonical.dbusmenu", "GetGroupProperties")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            std::vector<dbus_int32_t> ids = {10, 11};
            if (!slot->inactiveMenu.empty()) {
                ids.push_back(20);
                for (size_t j = 0; j < slot->inactiveMenu.size(); ++j) {
                    ids.push_back(static_cast<dbus_int32_t>(100 + j));
                    ids.push_back(static_cast<dbus_int32_t>(200 + j));
                }
                ids.push_back(21);
                ids.push_back(22);
                ids.push_back(23);
            }
            ids.push_back(1);
            ids.push_back(4);
            ids.push_back(2);
            ids.push_back(3);

            DBusMessageIter it, arr;
            dbus_message_iter_init_append(rep.p, &it);
            dbus_message_iter_open_container(&it, DBUS_TYPE_ARRAY, "(ia{sv})", &arr);
            for (dbus_int32_t id : ids) {
                DBusMessageIter st, props;
                dbus_message_iter_open_container(&arr, DBUS_TYPE_STRUCT, nullptr, &st);
                dbus_message_iter_append_basic(&st, DBUS_TYPE_INT32, &id);
                dbus_message_iter_open_container(&st, DBUS_TYPE_ARRAY, "{sv}", &props);
                AppendMenuItemProps(&props, *slot, id);
                dbus_message_iter_close_container(&st, &props);
                dbus_message_iter_close_container(&arr, &st);
            }
            dbus_message_iter_close_container(&it, &arr);
        }
    } else if (dbus_message_is_method_call(msg, "com.canonical.dbusmenu", "AboutToShow")) {
        rep.p = dbus_message_new_method_return(msg);
        if (rep) {
            dbus_bool_t needUpdate = 0;
            dbus_message_append_args(rep.p, DBUS_TYPE_BOOLEAN, &needUpdate, DBUS_TYPE_INVALID);
        }
    } else if (dbus_message_is_method_call(msg, "com.canonical.dbusmenu", "Event")) {
        DBusMessageIter it;
        if (dbus_message_iter_init(msg, &it) && dbus_message_iter_get_arg_type(&it) == DBUS_TYPE_INT32) {
            dbus_int32_t id = 0;
            dbus_message_iter_get_basic(&it, &id);
            if (dbus_message_iter_next(&it) && dbus_message_iter_get_arg_type(&it) == DBUS_TYPE_STRING) {
                const char* ev = nullptr;
                dbus_message_iter_get_basic(&it, &ev);
                if (ev && std::strcmp(ev, "clicked") == 0 && slot->owner) {
                    if (id == 1 && slot->owner->Control()) {
                        slot->owner->Control()->scanDueMs = 0;
                    } else if (id == 4) {
                        slot->owner->ToggleNotify();
                    } else if (id == 3 && slot->owner->Control()) {
                        slot->owner->Control()->running = false;
                    } else if (id == 22 || (id >= 200 && id < 300)) {
                        slot->owner->HandleHistoryAction(slot->index, id);
                    }
                }
            }
        }
        rep.p = dbus_message_new_method_return(msg);
    } else {
        return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
    }

    if (rep) dbus_connection_send(conn, rep.p, nullptr);
    return DBUS_HANDLER_RESULT_HANDLED;
}

DBusHandlerResult WatcherRestartFilter(DBusConnection*, DBusMessage* msg, void* data) {
    if (dbus_message_is_signal(msg, "org.freedesktop.DBus", "NameOwnerChanged")) {
        const char *name = nullptr, *oldOwner = nullptr, *newOwner = nullptr;
        if (dbus_message_get_args(msg, nullptr, DBUS_TYPE_STRING, &name,
                                  DBUS_TYPE_STRING, &oldOwner, DBUS_TYPE_STRING, &newOwner, DBUS_TYPE_INVALID)) {
            if (name && std::strcmp(name, "org.kde.StatusNotifierWatcher") == 0 && newOwner && newOwner[0] != '\0') {
                static_cast<SniManager*>(data)->RegisterAllWatchers();
            }
        }
    }
    return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
}

void RegisterWatcher(SniManager::Slot& s) {
    if (!s.conn) return;
    DbusMsg m(dbus_message_new_method_call(
        "org.kde.StatusNotifierWatcher", "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher", "RegisterStatusNotifierItem"));
    if (!m) return;
    const char* bn = s.busName.c_str();
    dbus_message_append_args(m.p, DBUS_TYPE_STRING, &bn, DBUS_TYPE_INVALID);
    dbus_connection_send(s.conn, m.p, nullptr);
    dbus_connection_flush(s.conn);
}

void EmitSniSignal(SniManager::Slot& s, const char* sigName, const char* argStr = nullptr) {
    if (!s.conn) return;
    DbusMsg sig(dbus_message_new_signal("/StatusNotifierItem", "org.kde.StatusNotifierItem", sigName));
    if (!sig) return;
    if (argStr) dbus_message_append_args(sig.p, DBUS_TYPE_STRING, &argStr, DBUS_TYPE_INVALID);
    dbus_connection_send(s.conn, sig.p, nullptr);
}

void EmitLabelSignal(SniManager::Slot& s) {
    if (!s.conn) return;
    DbusMsg sig(dbus_message_new_signal("/StatusNotifierItem", "org.kde.StatusNotifierItem", "XAyatanaNewLabel"));
    if (!sig) return;
    const char* lbl = s.label.c_str();
    const char* guide = "";
    dbus_message_append_args(sig.p, DBUS_TYPE_STRING, &lbl, DBUS_TYPE_STRING, &guide, DBUS_TYPE_INVALID);
    dbus_connection_send(s.conn, sig.p, nullptr);
}

void EmitMenuLayoutUpdated(SniManager::Slot& s) {
    if (!s.conn) return;
    DbusMsg sig(dbus_message_new_signal("/MenuBar", "com.canonical.dbusmenu", "LayoutUpdated"));
    if (!sig) return;
    dbus_uint32_t rev = s.menuRevision;
    dbus_int32_t parent = 0;
    dbus_message_append_args(sig.p, DBUS_TYPE_UINT32, &rev, DBUS_TYPE_INT32, &parent, DBUS_TYPE_INVALID);
    dbus_connection_send(s.conn, sig.p, nullptr);
}

void SendDesktopNotification(DBusConnection* conn, const std::string& name, int percent, bool critical) {
    if (!conn) return;
    DbusMsg msg(dbus_message_new_method_call(
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "Notify"));
    if (!msg) return;
    dbus_message_set_no_reply(msg.p, TRUE);

    const char* appName = "batt-tray";
    dbus_uint32_t replacesId = 0;
    const char* appIcon = critical ? "battery-caution" : "battery-low";
    const std::string summary = std::string(critical ? "Критический заряд: " : "Низкий заряд: ") + name;
    const std::string body = "Осталось " + std::to_string(percent) + "% заряда";
    const char* summaryPtr = summary.c_str();
    const char* bodyPtr = body.c_str();
    dbus_int32_t expireMs = 6000;

    DBusMessageIter it, actions, hints;
    dbus_message_iter_init_append(msg.p, &it);
    dbus_message_iter_append_basic(&it, DBUS_TYPE_STRING, &appName);
    dbus_message_iter_append_basic(&it, DBUS_TYPE_UINT32, &replacesId);
    dbus_message_iter_append_basic(&it, DBUS_TYPE_STRING, &appIcon);
    dbus_message_iter_append_basic(&it, DBUS_TYPE_STRING, &summaryPtr);
    dbus_message_iter_append_basic(&it, DBUS_TYPE_STRING, &bodyPtr);

    dbus_message_iter_open_container(&it, DBUS_TYPE_ARRAY, "s", &actions);
    dbus_message_iter_close_container(&it, &actions);

    dbus_message_iter_open_container(&it, DBUS_TYPE_ARRAY, "{sv}", &hints);
    {
        DBusMessageIter e, v;
        const char* k = "urgency";
        uint8_t urg = critical ? 2u : 1u;
        dbus_message_iter_open_container(&hints, DBUS_TYPE_DICT_ENTRY, nullptr, &e);
        dbus_message_iter_append_basic(&e, DBUS_TYPE_STRING, &k);
        dbus_message_iter_open_container(&e, DBUS_TYPE_VARIANT, "y", &v);
        dbus_message_iter_append_basic(&v, DBUS_TYPE_BYTE, &urg);
        dbus_message_iter_close_container(&e, &v);
        dbus_message_iter_close_container(&hints, &e);
    }
    AppendDictStr(&hints, "category", "device.battery");
    dbus_message_iter_close_container(&it, &hints);

    dbus_message_iter_append_basic(&it, DBUS_TYPE_INT32, &expireMs);

    dbus_connection_send(conn, msg.p, nullptr);
    dbus_connection_flush(conn);
}

} // namespace

int64_t NowMs() {
    timespec ts{};
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return static_cast<int64_t>(ts.tv_sec) * 1000 + ts.tv_nsec / 1000000;
}

void ControlState::ScheduleScan(int64_t delayMs) {
    const int64_t target = NowMs() + delayMs;
    if (scanDueMs < 0 || target < scanDueMs) scanDueMs = target;
}

void DrainDbusConn(DBusConnection* conn) {
    if (!conn) return;
    dbus_connection_read_write(conn, 0);
    while (dbus_connection_dispatch(conn) == DBUS_DISPATCH_DATA_REMAINS) {}
    dbus_connection_flush(conn);
}

SniManager::~SniManager() {
    Shutdown();
}

bool SniManager::Init(bool hideOffline, bool demoMode, bool notifyEnabled, ControlState* ctrl) {
    hideOffline_ = hideOffline;
    demoMode_ = demoMode;
    notifyEnabled_ = notifyEnabled;
    ctrl_ = ctrl;
    history_.Load(demoMode);

    static DBusObjectPathVTable sniVt{nullptr, HandleSni, nullptr, nullptr, nullptr, nullptr};
    static DBusObjectPathVTable menuVt{nullptr, HandleMenu, nullptr, nullptr, nullptr, nullptr};

    const auto& specs = GetSlotSpecs();
    const pid_t pid = getpid();
    DbusErr err;

    for (size_t i = 0; i < 3; ++i) {
        slots_[i].index = i;
        slots_[i].spec = &specs[i];
        slots_[i].owner = this;
        err.Reset();
        slots_[i].conn = dbus_bus_get_private(DBUS_BUS_SESSION, &err.raw);
        if (!slots_[i].conn) {
            Shutdown();
            return false;
        }
        dbus_connection_set_exit_on_disconnect(slots_[i].conn, FALSE);

        slots_[i].busName = "org.kde.StatusNotifierItem-" + std::to_string(pid) + "-" + std::to_string(i + 1);
        err.Reset();
        dbus_bus_request_name(slots_[i].conn, slots_[i].busName.c_str(), DBUS_NAME_FLAG_DO_NOT_QUEUE, &err.raw);

        dbus_connection_register_object_path(slots_[i].conn, "/StatusNotifierItem", &sniVt, &slots_[i]);
        dbus_connection_register_object_path(slots_[i].conn, "/MenuBar", &menuVt, &slots_[i]);

        if (i == 0) {
            dbus_bus_add_match(slots_[i].conn,
                "type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged',arg0='org.kde.StatusNotifierWatcher'",
                nullptr);
            dbus_connection_add_filter(slots_[i].conn, WatcherRestartFilter, this, nullptr);
        }

        UpdateSlot(slots_[i], {});
        RegisterWatcher(slots_[i]);
    }
    return true;
}

std::vector<InactiveMenuRow> SniManager::BuildInactiveMenu(size_t slotIdx) const {
    std::vector<InactiveMenuRow> rows;
    const auto& spec = *slots_[slotIdx].spec;
    const auto inactive = history_.GetInactiveForKind(spec.kind, states_[slotIdx].items);
    const size_t limit = std::min<size_t>(inactive.size(), 10);
    rows.reserve(limit);
    for (size_t j = 0; j < limit; ++j) {
        const std::string& dName = inactive[j].name.empty() ? std::string(spec.label) : inactive[j].name;
        const std::string lbl = "○ " + dName + " — " + std::to_string(inactive[j].percent) +
                                "% (" + DeviceHistory::FormatRelativeTime(inactive[j].lastSeenUnix) + ")";
        rows.push_back({inactive[j].key, lbl});
    }
    return rows;
}

void SniManager::UpdateAll(const std::array<DeviceState, 3>& states) {
    states_ = states;
    history_.SyncActive(states_);
    for (size_t i = 0; i < 3; ++i) {
        UpdateSlot(slots_[i], states_[i]);
    }

    const uint64_t nowSec = static_cast<uint64_t>(NowMs() / 1000);
    alertTracker_.PruneOffline(nowSec);
    if (!demoMode_ && notifyEnabled_) {
        for (size_t i = 0; i < 3; ++i) {
            if (!states_[i].online) continue;
            for (const auto& d : states_[i].items) {
                const std::string devName = d.name.empty() ? std::string(slots_[i].spec->label) : d.name;
                const AlertLevel lvl = alertTracker_.Evaluate(static_cast<int>(i), devName, d.percent, d.charging, nowSec);
                if (lvl == AlertLevel::None) continue;
                SendDesktopNotification(slots_[i].conn, devName, d.percent, lvl == AlertLevel::Critical5);
            }
        }
    }
}

void SniManager::ToggleNotify() {
    notifyEnabled_ = !notifyEnabled_;
    if (!notifyEnabled_) {
        alertTracker_.Clear();
    }
    for (auto& s : slots_) {
        ++s.menuRevision;
        EmitMenuLayoutUpdated(s);
        if (s.conn) dbus_connection_flush(s.conn);
    }
}

void SniManager::HandleHistoryAction(size_t slotIdx, dbus_int32_t itemId) {
    if (slotIdx >= 3) return;
    auto& s = slots_[slotIdx];
    bool changed = false;

    if (itemId == 22) {
        changed = history_.ClearInactiveForKind(s.spec->kind, states_[slotIdx].items);
    } else if (itemId >= 200 && itemId < 300) {
        const size_t idx = static_cast<size_t>(itemId - 200);
        if (idx < s.inactiveMenu.size()) {
            changed = history_.RemoveEntry(s.spec->kind, s.inactiveMenu[idx].key);
        }
    }

    if (changed) {
        s.inactiveMenu = BuildInactiveMenu(slotIdx);
        ++s.menuRevision;
        EmitMenuLayoutUpdated(s);
        if (s.conn) dbus_connection_flush(s.conn);
    }
}

void SniManager::UpdateSlot(Slot& s, const DeviceState& d) {
    std::string newTitle, newDesc, newLabel, newMenuStatus, newOverlay, newStatus;
    if (d.online && d.percent >= 0) {
        const std::string pctStr = std::to_string(d.percent) + "%";
        newTitle = std::string(s.spec->label) + ": " + pctStr + (d.charging ? " (зарядка)" : "");
        newDesc = d.name;
        newLabel = pctStr + (d.charging ? " ⚡" : "");
        newMenuStatus = d.name.empty() ? newTitle : (d.name + " — " + pctStr + (d.charging ? " ⚡" : ""));
        newOverlay = (!d.charging && d.percent <= 20) ? "battery-caution-symbolic" : "";
        newStatus = "Active";
    } else {
        newTitle = std::string(s.spec->label) + ": не подключено";
        newMenuStatus = newTitle;
        newStatus = hideOffline_ ? "Passive" : "Active";
    }

    auto newInactive = BuildInactiveMenu(s.index);

    const bool tipChanged = (newTitle != s.tipTitle || newDesc != s.tipDesc);
    const bool lblChanged = (newLabel != s.label);
    const bool menuChanged = (newMenuStatus != s.menuStatus || !(newInactive == s.inactiveMenu));
    const bool ovChanged = (newOverlay != s.overlay);
    const bool stChanged = (newStatus != s.status);

    s.tipTitle = std::move(newTitle);
    s.tipDesc = std::move(newDesc);
    s.label = std::move(newLabel);
    s.menuStatus = std::move(newMenuStatus);
    s.inactiveMenu = std::move(newInactive);
    s.overlay = std::move(newOverlay);
    s.status = std::move(newStatus);

    if (tipChanged) {
        EmitSniSignal(s, "NewTitle");
        EmitSniSignal(s, "NewToolTip");
    }
    if (lblChanged) {
        EmitLabelSignal(s);
    }
    if (menuChanged) {
        ++s.menuRevision;
        EmitMenuLayoutUpdated(s);
    }
    if (ovChanged) {
        EmitSniSignal(s, "NewOverlayIcon");
        EmitSniSignal(s, "NewIcon");
    }
    if (stChanged) {
        EmitSniSignal(s, "NewStatus", s.status.c_str());
    }
    if (s.conn && (tipChanged || lblChanged || menuChanged || ovChanged || stChanged)) {
        dbus_connection_flush(s.conn);
    }
}

void SniManager::RegisterAllWatchers() {
    for (auto& s : slots_) RegisterWatcher(s);
}

void SniManager::DrainAll() {
    for (auto& s : slots_) DrainDbusConn(s.conn);
}

void SniManager::Shutdown() {
    for (auto& s : slots_) {
        if (s.conn) {
            dbus_connection_close(s.conn);
            dbus_connection_unref(s.conn);
            s.conn = nullptr;
        }
    }
}

std::array<DBusConnection*, 3> SniManager::Connections() const {
    return {slots_[0].conn, slots_[1].conn, slots_[2].conn};
}