#include "history.hpp"

#include <errno.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include <algorithm>
#include <cstdio>
#include <cstdlib>

namespace {

constexpr size_t kMaxFileBytes = 16384;
constexpr size_t kMaxHistoryItems = 32;
constexpr int64_t kTimestampRefreshSec = 300;

struct FdGuard {
    int fd = -1;
    explicit FdGuard(int f = -1) : fd(f) {}
    ~FdGuard() { if (fd >= 0) close(fd); }
    FdGuard(const FdGuard&) = delete;
    FdGuard& operator=(const FdGuard&) = delete;
};

int64_t NowUnixSec() {
    timespec ts{};
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) return 0;
    return static_cast<int64_t>(ts.tv_sec);
}

bool ParseIntStrict(const std::string& s, int& out) {
    if (s.empty()) return false;
    char* end = nullptr;
    errno = 0;
    const long v = std::strtol(s.c_str(), &end, 10);
    if (errno != 0 || end == s.c_str() || *end != '\0') return false;
    out = static_cast<int>(v);
    return true;
}

bool ParseI64Strict(const std::string& s, int64_t& out) {
    if (s.empty()) return false;
    char* end = nullptr;
    errno = 0;
    const long long v = std::strtoll(s.c_str(), &end, 10);
    if (errno != 0 || end == s.c_str() || *end != '\0') return false;
    out = static_cast<int64_t>(v);
    return true;
}

bool WriteAll(int fd, const char* data, size_t len) {
    size_t done = 0;
    while (done < len) {
        const ssize_t n = write(fd, data + done, len - done);
        if (n < 0) {
            if (errno == EINTR) continue;
            return false;
        }
        if (n == 0) return false;
        done += static_cast<size_t>(n);
    }
    return true;
}

void EnsureDirRecursive(const std::string& dir) {
    if (dir.empty() || dir[0] != '/') return;
    std::string cur;
    cur.reserve(dir.size());
    size_t i = 0;
    while (i < dir.size()) {
        const size_t next = dir.find('/', i + 1);
        cur = dir.substr(0, next);
        if (!cur.empty()) {
            mkdir(cur.c_str(), 0700);
        }
        if (next == std::string::npos) break;
        i = next;
    }
}

bool MatchesActive(const HistoryItem& item, const std::vector<DeviceEntry>& activeItems) {
    const std::string itemLow = ToLower(item.name);
    for (const auto& act : activeItems) {
        if (!item.key.empty() && item.key == act.key) return true;
        if (!itemLow.empty() && itemLow == ToLower(act.name)) return true;
    }
    return false;
}

} // namespace

std::string DeviceHistory::GetStorageDir() {
    const char* xdg = std::getenv("XDG_STATE_HOME");
    if (xdg && xdg[0] == '/') {
        return std::string(xdg) + "/batt-tray";
    }
    const char* home = std::getenv("HOME");
    if (home && home[0] == '/') {
        return std::string(home) + "/.local/state/batt-tray";
    }
    return {};
}

std::string DeviceHistory::GetStoragePath() {
    const std::string dir = GetStorageDir();
    return dir.empty() ? std::string() : (dir + "/history.tsv");
}

void DeviceHistory::Load(bool demoMode) {
    items_.clear();
    demoMode_ = demoMode;
    if (demoMode_) {
        const int64_t now = NowUnixSec();
        items_.push_back({DeviceKind::Headphones, "sony wh-1000xm5", "Sony WH-1000XM5", 64, now - 7200});
        items_.push_back({DeviceKind::Mouse, "mx master 3s", "MX Master 3S", 91, now - 90000});
        return;
    }

    const std::string path = GetStoragePath();
    if (path.empty()) return;

    FdGuard f(open(path.c_str(), O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK));
    if (f.fd < 0) return;

    struct stat st{};
    if (fstat(f.fd, &st) != 0 || !S_ISREG(st.st_mode) || st.st_size <= 0 ||
        static_cast<size_t>(st.st_size) > kMaxFileBytes) {
        return;
    }

    std::string raw;
    raw.resize(static_cast<size_t>(st.st_size));
    size_t got = 0;
    while (got < raw.size()) {
        const ssize_t n = read(f.fd, raw.data() + got, raw.size() - got);
        if (n < 0) {
            if (errno == EINTR) continue;
            return;
        }
        if (n == 0) break;
        got += static_cast<size_t>(n);
    }
    raw.resize(got);

    size_t pos = 0;
    while (pos < raw.size() && items_.size() < kMaxHistoryItems) {
        const size_t nl = raw.find('\n', pos);
        std::string line = raw.substr(pos, nl == std::string::npos ? std::string::npos : nl - pos);
        pos = (nl == std::string::npos) ? raw.size() : nl + 1;
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.empty()) continue;

        const size_t c1 = line.find('\t');
        const size_t c2 = (c1 == std::string::npos) ? std::string::npos : line.find('\t', c1 + 1);
        const size_t c3 = (c2 == std::string::npos) ? std::string::npos : line.find('\t', c2 + 1);
        const size_t c4 = (c3 == std::string::npos) ? std::string::npos : line.find('\t', c3 + 1);
        if (c4 == std::string::npos || line.find('\t', c4 + 1) != std::string::npos) continue;

        int kindInt = -1;
        int pct = -1;
        int64_t ts = 0;
        if (!ParseIntStrict(line.substr(0, c1), kindInt) || kindInt < 0 || kindInt > 2) continue;
        if (!ParseIntStrict(line.substr(c3 + 1, c4 - c3 - 1), pct) || pct < 0 || pct > 100) continue;
        if (!ParseI64Strict(line.substr(c4 + 1), ts) || ts <= 0) continue;

        const std::string key = SanitizeDeviceText(line.substr(c1 + 1, c2 - c1 - 1), 64);
        const std::string name = SanitizeDeviceText(line.substr(c2 + 1, c3 - c2 - 1), 64);
        if (key.empty()) continue;

        items_.push_back({static_cast<DeviceKind>(kindInt), key, name, pct, ts});
    }
}

void DeviceHistory::Save() const {
    if (demoMode_) return;
    const std::string dir = GetStorageDir();
    const std::string path = GetStoragePath();
    if (dir.empty() || path.empty()) return;

    EnsureDirRecursive(dir);

    struct stat dirSt{};
    if (lstat(dir.c_str(), &dirSt) != 0 || !S_ISDIR(dirSt.st_mode) ||
        dirSt.st_uid != static_cast<uid_t>(getuid()) || (dirSt.st_mode & 077) != 0) {
        return;
    }

    std::string payload;
    payload.reserve(items_.size() * 80);
    for (const auto& it : items_) {
        const std::string safeKey = SanitizeDeviceText(it.key, 64);
        const std::string safeName = SanitizeDeviceText(it.name, 64);
        if (safeKey.empty() || it.percent < 0 || it.percent > 100) continue;
        payload += std::to_string(static_cast<int>(it.kind));
        payload += '\t';
        payload += safeKey;
        payload += '\t';
        payload += safeName;
        payload += '\t';
        payload += std::to_string(it.percent);
        payload += '\t';
        payload += std::to_string(it.lastSeenUnix);
        payload += '\n';
    }

    const std::string tmpPath = path + ".tmp";
    FdGuard f(open(tmpPath.c_str(), O_WRONLY | O_CREAT | O_TRUNC | O_NOFOLLOW | O_CLOEXEC, 0600));
    if (f.fd < 0) return;

    struct stat tmpSt{};
    if (fstat(f.fd, &tmpSt) != 0 || !S_ISREG(tmpSt.st_mode)) {
        unlink(tmpPath.c_str());
        return;
    }

    if (!payload.empty() && !WriteAll(f.fd, payload.data(), payload.size())) {
        unlink(tmpPath.c_str());
        return;
    }

    close(f.fd);
    f.fd = -1;
    if (std::rename(tmpPath.c_str(), path.c_str()) != 0) {
        unlink(tmpPath.c_str());
    }
}

void DeviceHistory::SyncActive(const std::array<DeviceState, 3>& active) {
    const int64_t now = NowUnixSec();
    bool dirty = false;

    for (size_t k = 0; k < 3; ++k) {
        const DeviceKind kind = static_cast<DeviceKind>(k);
        for (const auto& d : active[k].items) {
            if (d.key.empty() || d.percent < 0 || d.percent > 100) continue;
            auto it = std::find_if(items_.begin(), items_.end(), [&](const HistoryItem& h) {
                return h.kind == kind && (h.key == d.key || (!d.name.empty() && ToLower(h.name) == ToLower(d.name)));
            });
            if (it != items_.end()) {
                if (it->percent != d.percent || (!d.name.empty() && it->name != d.name) || it->key != d.key) {
                    it->key = d.key;
                    if (!d.name.empty()) it->name = d.name;
                    it->percent = d.percent;
                    it->lastSeenUnix = now;
                    dirty = true;
                } else if (now - it->lastSeenUnix >= kTimestampRefreshSec) {
                    it->lastSeenUnix = now;
                    dirty = true;
                }
            } else {
                items_.push_back({kind, d.key, d.name, d.percent, now});
                dirty = true;
            }
        }
    }

    if (items_.size() > kMaxHistoryItems) {
        std::sort(items_.begin(), items_.end(), [](const HistoryItem& a, const HistoryItem& b) {
            return a.lastSeenUnix > b.lastSeenUnix;
        });
        items_.resize(kMaxHistoryItems);
        dirty = true;
    }

    if (dirty) Save();
}

std::vector<HistoryItem> DeviceHistory::GetInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems) const {
    std::vector<HistoryItem> out;
    for (const auto& it : items_) {
        if (it.kind != kind) continue;
        if (MatchesActive(it, activeItems)) continue;
        out.push_back(it);
    }
    std::sort(out.begin(), out.end(), [](const HistoryItem& a, const HistoryItem& b) {
        return a.lastSeenUnix > b.lastSeenUnix;
    });
    return out;
}

bool DeviceHistory::RemoveEntry(DeviceKind kind, const std::string& key) {
    const size_t before = items_.size();
    items_.erase(std::remove_if(items_.begin(), items_.end(), [&](const HistoryItem& it) {
        return it.kind == kind && it.key == key;
    }), items_.end());
    if (items_.size() != before) {
        Save();
        return true;
    }
    return false;
}

bool DeviceHistory::ClearInactiveForKind(DeviceKind kind, const std::vector<DeviceEntry>& activeItems) {
    const size_t before = items_.size();
    items_.erase(std::remove_if(items_.begin(), items_.end(), [&](const HistoryItem& it) {
        return it.kind == kind && !MatchesActive(it, activeItems);
    }), items_.end());
    if (items_.size() != before) {
        Save();
        return true;
    }
    return false;
}

std::string DeviceHistory::FormatRelativeTime(int64_t lastSeenUnix) {
    const int64_t now = NowUnixSec();
    if (lastSeenUnix <= 0 || now <= lastSeenUnix) return "только что";
    const int64_t diff = now - lastSeenUnix;
    if (diff < 60) return "только что";
    if (diff < 3600) return std::to_string(diff / 60) + " мин назад";
    if (diff < 86400) return std::to_string(diff / 3600) + " ч назад";
    if (diff < 172800) return "вчера";
    return std::to_string(diff / 86400) + " дн. назад";
}