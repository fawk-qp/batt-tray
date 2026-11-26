#include "scan_worker.hpp"
#include "scanner.hpp"
#include "sni.hpp"

#include <fcntl.h>
#include <poll.h>
#include <signal.h>
#include <sys/file.h>
#include <sys/prctl.h>
#include <sys/signalfd.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <string>

namespace {

constexpr int64_t kPollFallbackMs = 60000;
constexpr int64_t kDebounceMs = 800;

struct FdGuard {
    int fd = -1;
    explicit FdGuard(int f = -1) : fd(f) {}
    ~FdGuard() { if (fd >= 0) close(fd); }
    FdGuard(const FdGuard&) = delete;
    FdGuard& operator=(const FdGuard&) = delete;
};

int AcquireInstanceLock() {
    const char* xdg = std::getenv("XDG_RUNTIME_DIR");
    std::string lockPath;
    if (xdg && xdg[0] == '/') {
        lockPath = std::string(xdg) + "/batt-tray.lock";
    } else {
        const char* cache = std::getenv("XDG_CACHE_HOME");
        std::string base;
        if (cache && cache[0] == '/') {
            base = cache;
        } else {
            const char* home = std::getenv("HOME");
            base = (home && home[0] == '/') ? std::string(home) + "/.cache" : std::string();
        }
        if (base.empty()) return -1;
        const std::string dir = base + "/batt-tray";
        mkdir(dir.c_str(), 0700);
        lockPath = dir + "/batt-tray.lock";
    }
    const int fd = open(lockPath.c_str(), O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0) return -1;
    struct stat st{};
    if (fstat(fd, &st) != 0 || !S_ISREG(st.st_mode) || st.st_uid != static_cast<uid_t>(getuid())) {
        close(fd);
        return -1;
    }
    if (flock(fd, LOCK_EX | LOCK_NB) != 0) {
        close(fd);
        return -2;
    }
    return fd;
}

std::array<DeviceState, 3> BuildDemoStates() {
    std::array<DeviceState, 3> demo{};
    demo[0] = {"Logitech G Pro X Superlight", 78, true, false,
               {{"logitech g pro x superlight", "Logitech G Pro X Superlight", 78, false}}};
    demo[1] = {"Keychron K2", 42, true, true,
               {{"keychron k2", "Keychron K2", 42, true}}};
    demo[2] = {"HUAWEI FreeBuds Pro 2", 18, true, false,
               {{"huawei freebuds pro 2", "HUAWEI FreeBuds Pro 2", 18, false}}};
    return demo;
}

DBusHandlerResult SystemSignalFilter(DBusConnection*, DBusMessage* msg, void* data) {
    if (dbus_message_get_type(msg) == DBUS_MESSAGE_TYPE_SIGNAL) {
        const char* iface = dbus_message_get_interface(msg);
        if (iface && (std::strcmp(iface, "org.freedesktop.UPower") == 0 ||
                      std::strcmp(iface, "org.freedesktop.DBus.Properties") == 0)) {
            static_cast<ControlState*>(data)->ScheduleScan(kDebounceMs);
        }
    }
    return DBUS_HANDLER_RESULT_NOT_YET_HANDLED;
}

} // namespace

int main(int argc, char** argv) {
    umask(0077);
    prctl(PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0);
    prctl(PR_SET_DUMPABLE, 0, 0, 0, 0);
    dbus_threads_init_default();

    bool hideOffline = false;
    bool demoMode = false;
    bool notifyEnabled = true;
    bool allowAirpodsBle = false;
    for (int i = 1; i < argc; ++i) {
        if (std::strcmp(argv[i], "--hide-offline") == 0) hideOffline = true;
        else if (std::strcmp(argv[i], "--demo") == 0) demoMode = true;
        else if (std::strcmp(argv[i], "--no-notify") == 0) notifyEnabled = false;
        else if (std::strcmp(argv[i], "--airpods-ble") == 0) allowAirpodsBle = true;
    }

    FdGuard lockFd(AcquireInstanceLock());
    if (lockFd.fd == -2) return 0;

    sigset_t sigMask;
    sigemptyset(&sigMask);
    sigaddset(&sigMask, SIGINT);
    sigaddset(&sigMask, SIGTERM);
    sigprocmask(SIG_BLOCK, &sigMask, nullptr);
    FdGuard sigFd(signalfd(-1, &sigMask, SFD_NONBLOCK | SFD_CLOEXEC));

    ControlState ctrl{};

    DBusConnection* sysBus = nullptr;
    if (!demoMode) {
        DBusError err;
        dbus_error_init(&err);
        sysBus = dbus_bus_get_private(DBUS_BUS_SYSTEM, &err);
        dbus_error_free(&err);
        if (sysBus) {
            dbus_connection_set_exit_on_disconnect(sysBus, FALSE);
            dbus_bus_add_match(sysBus, "type='signal',sender='org.freedesktop.UPower'", nullptr);
            dbus_bus_add_match(sysBus, "type='signal',sender='org.bluez',interface='org.freedesktop.DBus.Properties'", nullptr);
            dbus_connection_add_filter(sysBus, SystemSignalFilter, &ctrl, nullptr);
        }
    }

    SniManager sni;
    if (!sni.Init(hideOffline, demoMode, notifyEnabled, &ctrl)) {
        if (sysBus) {
            dbus_connection_close(sysBus);
            dbus_connection_unref(sysBus);
        }
        return 1;
    }

    ScanWorker worker;
    if (!demoMode) {
        worker.Start(allowAirpodsBle);
    }

    int64_t nextPeriodicMs = NowMs() + kPollFallbackMs;
    while (ctrl.running) {
        const int64_t now = NowMs();
        if ((ctrl.scanDueMs >= 0 && now >= ctrl.scanDueMs) || now >= nextPeriodicMs) {
            ctrl.scanDueMs = -1;
            nextPeriodicMs = now + kPollFallbackMs;
            if (demoMode) {
                sni.UpdateAll(BuildDemoStates());
            } else {
                worker.RequestScan();
            }
        }

        pollfd fds[6]{};
        int nfds = 0;
        int sigIdx = -1;
        int workerIdx = -1;
        if (sigFd.fd >= 0) {
            sigIdx = nfds;
            fds[nfds].fd = sigFd.fd;
            fds[nfds].events = POLLIN;
            ++nfds;
        }
        if (!demoMode && worker.NotifyFd() >= 0) {
            workerIdx = nfds;
            fds[nfds].fd = worker.NotifyFd();
            fds[nfds].events = POLLIN;
            ++nfds;
        }

        const auto sniConns = sni.Connections();
        DBusConnection* allConns[4] = {sniConns[0], sniConns[1], sniConns[2], sysBus};
        for (DBusConnection* c : allConns) {
            int fd = -1;
            if (c && dbus_connection_get_unix_fd(c, &fd) && fd >= 0) {
                fds[nfds].fd = fd;
                fds[nfds].events = POLLIN;
                ++nfds;
            }
        }

        const int64_t deadline = (ctrl.scanDueMs >= 0) ? std::min(ctrl.scanDueMs, nextPeriodicMs) : nextPeriodicMs;
        const int timeoutMs = static_cast<int>(std::clamp<int64_t>(deadline - NowMs(), 0, kPollFallbackMs));

        const int rc = poll(fds, static_cast<nfds_t>(nfds), timeoutMs);
        if (rc > 0) {
            if (sigIdx >= 0 && (fds[sigIdx].revents & POLLIN)) {
                signalfd_siginfo info{};
                while (read(sigFd.fd, &info, sizeof(info)) > 0) {}
                ctrl.running = false;
                break;
            }
            if (workerIdx >= 0 && (fds[workerIdx].revents & POLLIN)) {
                std::array<DeviceState, 3> snap{};
                if (worker.TakeSnapshot(snap)) {
                    sni.UpdateAll(snap);
                }
            }
            sni.DrainAll();
            DrainDbusConn(sysBus);
        }
    }

    worker.Stop();
    sni.Shutdown();
    if (sysBus) {
        dbus_connection_close(sysBus);
        dbus_connection_unref(sysBus);
    }
    return 0;
}