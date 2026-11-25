#include "scanner.hpp"

#include <dirent.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>
#include <cctype>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

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

struct FdGuard {
    int fd = -1;
    explicit FdGuard(int f) : fd(f) {}
    ~FdGuard() { if (fd >= 0) close(fd); }
    FdGuard(const FdGuard&) = delete;
    FdGuard& operator=(const FdGuard&) = delete;
};

struct DirGuard {
    DIR* d = nullptr;
    explicit DirGuard(DIR* dir) : d(dir) {}
    ~DirGuard() { if (d) closedir(d); }
    DirGuard(const DirGuard&) = delete;
    DirGuard& operator=(const DirGuard&) = delete;
};

bool AllOnline(const std::array<DeviceState, 3>& out) {
    return out[0].online && out[1].online && out[2].online;
}

std::string ReadSmallFile(const std::string& path, size_t maxBytes = 255) {
    FdGuard f(open(path.c_str(), O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK));
    if (f.fd < 0) return {};
    struct stat st{};
    if (fstat(f.fd, &st) != 0 || !S_ISREG(st.st_mode)) return {};
    char buf[512];
    const size_t cap = std::min(maxBytes, sizeof(buf) - 1);
    const ssize_t n = read(f.fd, buf, cap);
    if (n <= 0) return {};
    buf[n] = '\0';
    return std::string(buf, static_cast<size_t>(n));
}

std::string ReadFirstLine(const std::string& path) {
    std::string raw = ReadSmallFile(path, 255);
    const size_t nl = raw.find('\n');
    if (nl != std::string::npos) raw.resize(nl);
    return Trim(std::move(raw));
}

bool ParsePercentStrict(const std::string& s, int& out) {
    if (s.empty()) return false;
    char* end = nullptr;
    const long v = std::strtol(s.c_str(), &end, 10);
    if (end == s.c_str() || *end != '\0' || v < 0 || v > 100) return false;
    out = static_cast<int>(v);
    return true;
}

bool HasNonZeroHex(const std::string& s) {
    for (char c : s) {
        if (std::isxdigit(static_cast<unsigned char>(c)) && c != '0') return true;
    }
    return false;
}

void ScanUPower(DBusConnection* sys, std::array<DeviceState, 3>& out) {
    if (!sys) return;
    DbusMsg call(dbus_message_new_method_call(
        "org.freedesktop.UPower", "/org/freedesktop/UPower",
        "org.freedesktop.UPower", "EnumerateDevices"));
    if (!call) return;

    DbusErr err;
    DbusMsg rep(dbus_connection_send_with_reply_and_block(sys, call.p, 1500, &err.raw));
    if (!rep) return;

    std::vector<std::string> paths;
    DBusMessageIter it;
    if (dbus_message_iter_init(rep.p, &it) && dbus_message_iter_get_arg_type(&it) == DBUS_TYPE_ARRAY) {
        DBusMessageIter arr;
        dbus_message_iter_recurse(&it, &arr);
        while (dbus_message_iter_get_arg_type(&arr) == DBUS_TYPE_OBJECT_PATH) {
            const char* p = nullptr;
            dbus_message_iter_get_basic(&arr, &p);
            if (p) paths.emplace_back(p);
            dbus_message_iter_next(&arr);
        }
    }
    if (paths.size() > 64) paths.resize(64);

    for (const auto& path : paths) {
        DbusMsg req(dbus_message_new_method_call(
            "org.freedesktop.UPower", path.c_str(),
            "org.freedesktop.DBus.Properties", "GetAll"));
        if (!req) continue;
        const char* iface = "org.freedesktop.UPower.Device";
        if (!dbus_message_append_args(req.p, DBUS_TYPE_STRING, &iface, DBUS_TYPE_INVALID)) continue;

        err.Reset();
        DbusMsg pr(dbus_connection_send_with_reply_and_block(sys, req.p, 1000, &err.raw));
        if (!pr) continue;

        uint32_t devType = 0, state = 0;
        double pct = -1.0;
        bool isPresent = false, powerSupply = true;
        std::string model, nativePath, iconName;

        DBusMessageIter pit;
        if (dbus_message_iter_init(pr.p, &pit) && dbus_message_iter_get_arg_type(&pit) == DBUS_TYPE_ARRAY) {
            DBusMessageIter dict;
            dbus_message_iter_recurse(&pit, &dict);
            while (dbus_message_iter_get_arg_type(&dict) == DBUS_TYPE_DICT_ENTRY) {
                DBusMessageIter entry, var;
                dbus_message_iter_recurse(&dict, &entry);
                const char* key = nullptr;
                if (dbus_message_iter_get_arg_type(&entry) == DBUS_TYPE_STRING) {
                    dbus_message_iter_get_basic(&entry, &key);
                }
                if (key && dbus_message_iter_next(&entry) &&
                    dbus_message_iter_get_arg_type(&entry) == DBUS_TYPE_VARIANT) {
                    dbus_message_iter_recurse(&entry, &var);
                    const int t = dbus_message_iter_get_arg_type(&var);
                    if (std::strcmp(key, "Type") == 0 && t == DBUS_TYPE_UINT32) {
                        dbus_message_iter_get_basic(&var, &devType);
                    } else if (std::strcmp(key, "State") == 0 && t == DBUS_TYPE_UINT32) {
                        dbus_message_iter_get_basic(&var, &state);
                    } else if (std::strcmp(key, "Percentage") == 0 && t == DBUS_TYPE_DOUBLE) {
                        dbus_message_iter_get_basic(&var, &pct);
                    } else if (std::strcmp(key, "IsPresent") == 0 && t == DBUS_TYPE_BOOLEAN) {
                        dbus_bool_t b = 0;
                        dbus_message_iter_get_basic(&var, &b);
                        isPresent = (b != 0);
                    } else if (std::strcmp(key, "PowerSupply") == 0 && t == DBUS_TYPE_BOOLEAN) {
                        dbus_bool_t b = 1;
                        dbus_message_iter_get_basic(&var, &b);
                        powerSupply = (b != 0);
                    } else if (std::strcmp(key, "Model") == 0 && t == DBUS_TYPE_STRING) {
                        const char* s = nullptr;
                        dbus_message_iter_get_basic(&var, &s);
                        if (s) model = s;
                    } else if (std::strcmp(key, "NativePath") == 0 && t == DBUS_TYPE_STRING) {
                        const char* s = nullptr;
                        dbus_message_iter_get_basic(&var, &s);
                        if (s) nativePath = s;
                    } else if (std::strcmp(key, "IconName") == 0 && t == DBUS_TYPE_STRING) {
                        const char* s = nullptr;
                        dbus_message_iter_get_basic(&var, &s);
                        if (s) iconName = s;
                    }
                }
                dbus_message_iter_next(&dict);
            }
        }

        // PowerSupply == true marks laptop main battery or AC adapter; !(pct >= 0 && pct <= 100) also rejects NaN.
        if (powerSupply || !isPresent || !(pct >= 0.0 && pct <= 100.0)) continue;

        DeviceKind k = DeviceKind::Unknown;
        if (devType == 5) k = DeviceKind::Mouse;
        else if (devType == 6) k = DeviceKind::Keyboard;
        else if (devType == 17 || devType == 18) k = DeviceKind::Headphones;
        else if (devType == 0 || devType == 2 || devType == 26) k = ClassifyDevice(model, iconName);

        const int rounded = std::clamp(static_cast<int>(pct + 0.5), 0, 100);
        StoreCandidate(out, k, model.empty() ? nativePath : model, rounded, state == 1);
    }
}

void ScanBlueZ(DBusConnection* sys, std::array<DeviceState, 3>& out) {
    if (!sys || AllOnline(out)) return;
    DbusMsg call(dbus_message_new_method_call(
        "org.bluez", "/", "org.freedesktop.DBus.ObjectManager", "GetManagedObjects"));
    if (!call) return;

    DbusErr err;
    DbusMsg rep(dbus_connection_send_with_reply_and_block(sys, call.p, 1500, &err.raw));
    if (!rep) return;

    DBusMessageIter it;
    if (!dbus_message_iter_init(rep.p, &it) || dbus_message_iter_get_arg_type(&it) != DBUS_TYPE_ARRAY) return;

    DBusMessageIter objs;
    dbus_message_iter_recurse(&it, &objs);
    while (dbus_message_iter_get_arg_type(&objs) == DBUS_TYPE_DICT_ENTRY) {
        DBusMessageIter oe, ifaces;
        dbus_message_iter_recurse(&objs, &oe);
        if (dbus_message_iter_next(&oe) && dbus_message_iter_get_arg_type(&oe) == DBUS_TYPE_ARRAY) {
            dbus_message_iter_recurse(&oe, &ifaces);
            bool connected = false, hasBat = false;
            uint8_t batPct = 0;
            uint32_t cod = 0;
            std::string name, icon;

            while (dbus_message_iter_get_arg_type(&ifaces) == DBUS_TYPE_DICT_ENTRY) {
                DBusMessageIter ie, props;
                dbus_message_iter_recurse(&ifaces, &ie);
                const char* ifName = nullptr;
                if (dbus_message_iter_get_arg_type(&ie) == DBUS_TYPE_STRING) {
                    dbus_message_iter_get_basic(&ie, &ifName);
                }
                const bool isDev = ifName && std::strcmp(ifName, "org.bluez.Device1") == 0;
                const bool isBat = ifName && std::strcmp(ifName, "org.bluez.Battery1") == 0;
                if ((isDev || isBat) && dbus_message_iter_next(&ie) &&
                    dbus_message_iter_get_arg_type(&ie) == DBUS_TYPE_ARRAY) {
                    dbus_message_iter_recurse(&ie, &props);
                    while (dbus_message_iter_get_arg_type(&props) == DBUS_TYPE_DICT_ENTRY) {
                        DBusMessageIter pe, pv;
                        dbus_message_iter_recurse(&props, &pe);
                        const char* pk = nullptr;
                        if (dbus_message_iter_get_arg_type(&pe) == DBUS_TYPE_STRING) {
                            dbus_message_iter_get_basic(&pe, &pk);
                        }
                        if (pk && dbus_message_iter_next(&pe) &&
                            dbus_message_iter_get_arg_type(&pe) == DBUS_TYPE_VARIANT) {
                            dbus_message_iter_recurse(&pe, &pv);
                            const int pt = dbus_message_iter_get_arg_type(&pv);
                            if (isBat && std::strcmp(pk, "Percentage") == 0 && pt == DBUS_TYPE_BYTE) {
                                dbus_message_iter_get_basic(&pv, &batPct);
                                hasBat = true;
                            } else if (isDev && std::strcmp(pk, "Connected") == 0 && pt == DBUS_TYPE_BOOLEAN) {
                                dbus_bool_t b = 0;
                                dbus_message_iter_get_basic(&pv, &b);
                                connected = (b != 0);
                            } else if (isDev && std::strcmp(pk, "Name") == 0 && pt == DBUS_TYPE_STRING) {
                                const char* s = nullptr;
                                dbus_message_iter_get_basic(&pv, &s);
                                if (s) name = s;
                            } else if (isDev && std::strcmp(pk, "Icon") == 0 && pt == DBUS_TYPE_STRING) {
                                const char* s = nullptr;
                                dbus_message_iter_get_basic(&pv, &s);
                                if (s) icon = s;
                            } else if (isDev && std::strcmp(pk, "Class") == 0 && pt == DBUS_TYPE_UINT32) {
                                dbus_message_iter_get_basic(&pv, &cod);
                            }
                        }
                        dbus_message_iter_next(&props);
                    }
                }
                dbus_message_iter_next(&ifaces);
            }
            if (connected && hasBat && batPct <= 100) {
                StoreCandidate(out, ClassifyDevice(name, icon, cod), name, batPct, false);
            }
        }
        dbus_message_iter_next(&objs);
    }
}

void ScanSysfs(std::array<DeviceState, 3>& out) {
    if (AllOnline(out)) return;
    DirGuard dir(opendir("/sys/class/power_supply"));
    if (!dir.d) return;

    while (dirent* ent = readdir(dir.d)) {
        if (ent->d_name[0] == '.') continue;
        const std::string base = std::string("/sys/class/power_supply/") + ent->d_name;
        if (ReadFirstLine(base + "/scope") != "Device") continue;

        int pct = -1;
        if (!ParsePercentStrict(ReadFirstLine(base + "/capacity"), pct)) continue;

        const bool charging = (ReadFirstLine(base + "/status") == "Charging");
        std::string name = ReadFirstLine(base + "/model_name");

        if (name.empty()) {
            const std::string uev = ReadSmallFile(base + "/device/uevent", 511);
            size_t pos = uev.find("HID_NAME=");
            if (pos != std::string::npos) {
                pos += 9;
                const size_t end = uev.find('\n', pos);
                name = Trim(uev.substr(pos, end == std::string::npos ? std::string::npos : end - pos));
            }
        }

        // Kernel input capability heuristic: EV_REL indicates mouse, LEDs indicate keyboard.
        bool hasRel = false, hasLed = false;
        DirGuard inp(opendir((base + "/device/input").c_str()));
        if (inp.d) {
            while (dirent* ie = readdir(inp.d)) {
                if (std::strncmp(ie->d_name, "input", 5) != 0) continue;
                const std::string ip = base + "/device/input/" + ie->d_name;
                if (name.empty()) name = ReadFirstLine(ip + "/name");
                if (HasNonZeroHex(ReadFirstLine(ip + "/capabilities/rel"))) hasRel = true;
                if (HasNonZeroHex(ReadFirstLine(ip + "/capabilities/led"))) hasLed = true;
            }
        }

        DeviceKind k = ClassifyDevice(name);
        if (k == DeviceKind::Unknown) {
            if (hasRel && !hasLed) k = DeviceKind::Mouse;
            else if (hasLed && !hasRel) k = DeviceKind::Keyboard;
        }
        StoreCandidate(out, k, name.empty() ? ent->d_name : name, pct, charging);
    }
}

} // namespace

std::array<DeviceState, 3> ScanAllDevices(DBusConnection* sysBus) {
    std::array<DeviceState, 3> out{};
    ScanUPower(sysBus, out);
    ScanBlueZ(sysBus, out);
    ScanSysfs(out);
    return out;
}