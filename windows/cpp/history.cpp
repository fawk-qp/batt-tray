#include "history.hpp"

#include <algorithm>
#include <cstdlib>
#include <cwchar>

namespace {

constexpr size_t kMaxFileBytes = 16384;
constexpr size_t kMaxHistoryItems = 32;
constexpr int64_t kTimestampRefreshSec = 300;

int64_t NowUnixSec() {
    FILETIME ft{};
    GetSystemTimeAsFileTime(&ft);
    const uint64_t ticks = (static_cast<uint64_t>(ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
    if (ticks < 116444736000000000ULL) return 0;
    return static_cast<int64_t>((ticks - 116444736000000000ULL) / 10000000ULL);
}

std::string WideToUtf8(const std::wstring& ws) {
    if (ws.empty()) return {};
    const int need = WideCharToMultiByte(CP_UTF8, 0, ws.data(), static_cast<int>(ws.size()), nullptr, 0, nullptr, nullptr);
    if (need <= 0) return {};
    std::string out(static_cast<size_t>(need), '\0');
    WideCharToMultiByte(CP_UTF8, 0, ws.data(), static_cast<int>(ws.size()), out.data(), need, nullptr, nullptr);
    return out;
}

std::wstring Utf8ToWide(const std::string& s) {
    if (s.empty()) return {};
    const int need = MultiByteToWideChar(CP_UTF8, 0, s.data(), static_cast<int>(s.size()), nullptr, 0);
    if (need <= 0) return {};
    std::wstring out(static_cast<size_t>(need), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, s.data(), static_cast<int>(s.size()), out.data(), need);
    return out;
}

bool IsReparsePoint(const std::wstring& path) {
    const DWORD attrs = GetFileAttributesW(path.c_str());
    return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
}

bool MatchesActive(const HistoryItem& item, const std::vector<DeviceEntry>& activeItems) {
    const std::wstring itemLow = ToLower(item.name);
    for (const auto& act : activeItems) {
        if (!item.key.empty() && item.key == act.key) return true;
        if (!itemLow.empty() && itemLow == ToLower(act.name)) return true;
    }
    return false;
}

} // namespace

std::wstring DeviceHistory::GetStorageDir() {
    wchar_t buf[MAX_PATH]{};
    DWORD len = GetEnvironmentVariableW(L"LOCALAPPDATA", buf, _countof(buf));
    if (len > 0 && len < _countof(buf)) {
        return std::wstring(buf, len) + L"\\BattTray";
    }
    len = GetEnvironmentVariableW(L"USERPROFILE", buf, _countof(buf));
    if (len > 0 && len < _countof(buf)) {
        return std::wstring(buf, len) + L"\\AppData\\Local\\BattTray";
    }
    return {};
}

std::wstring DeviceHistory::GetStoragePath() {
    const std::wstring dir = GetStorageDir();
    return dir.empty() ? std::wstring() : (dir + L"\\history.tsv");
}

void DeviceHistory::Load() {
    items_.clear();
    const std::wstring path = GetStoragePath();
    if (path.empty() || IsReparsePoint(path)) return;

    HANDLE h = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING,
                           FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT, nullptr);
    if (h == INVALID_HANDLE_VALUE) return;

    LARGE_INTEGER fsize{};
    if (!GetFileSizeEx(h, &fsize) || fsize.QuadPart <= 0 || fsize.QuadPart > static_cast<LONGLONG>(kMaxFileBytes)) {
        CloseHandle(h);
        return;
    }

    std::string raw(static_cast<size_t>(fsize.QuadPart), '\0');
    DWORD readBytes = 0;
    const BOOL ok = ReadFile(h, raw.data(), static_cast<DWORD>(raw.size()), &readBytes, nullptr);
    CloseHandle(h);
    if (!ok || readBytes == 0) return;
    raw.resize(readBytes);

    size_t pos = 0;
    while (pos < raw.size() && items_.size() < kMaxHistoryItems) {
        const size_t nl = raw.find('\n', pos);
        std::string line = raw.substr(pos, nl == std::string::npos ? std::string::npos : nl - pos);
        pos = (nl == std::string::npos) ? raw.size() : nl + 1;
        if (!line.empty() && line.back() == '\r') line.pop_back();
        if (line.empty()) continue;

        size_t c1 = line.find('\t');
        size_t c2 = (c1 == std::string::npos) ? std::string::npos : line.find('\t', c1 + 1);
        size_t c3 = (c2 == std::string::npos) ? std::string::npos : line.find('\t', c2 + 1);
        size_t c4 = (c3 == std::string::npos) ? std::string::npos : line.find('\t', c3 + 1);
        if (c4 == std::string::npos) continue;

        const int kindInt = std::atoi(line.substr(0, c1).c_str());
        if (kindInt < 0 || kindInt > 2) continue;

        const std::wstring key = SanitizeDeviceText(Utf8ToWide(line.substr(c1 + 1, c2 - c1 - 1)), 64);
        const std::wstring name = CleanDeviceName(Utf8ToWide(line.substr(c2 + 1, c3 - c2 - 1)));
        const int pct = std::atoi(line.substr(c3 + 1, c4 - c3 - 1).c_str());
        const int64_t ts = std::strtoll(line.substr(c4 + 1).c_str(), nullptr, 10);

        if (key.empty() || pct < 0 || pct > 100 || ts <= 0) continue;
        items_.push_back({static_cast<DeviceKind>(kindInt), key, name, pct, ts});
    }
}

void DeviceHistory::Save() const {
    const std::wstring dir = GetStorageDir();
    const std::wstring path = GetStoragePath();
    if (dir.empty() || path.empty()) return;

    CreateDirectoryW(dir.c_str(), nullptr);
    if (IsReparsePoint(dir) || IsReparsePoint(path)) return;

    std::string payload;
    payload.reserve(items_.size() * 80);
    for (const auto& it : items_) {
        const std::wstring safeKey = SanitizeDeviceText(it.key, 64);
        const std::wstring safeName = CleanDeviceName(it.name);
        if (safeKey.empty() || it.percent < 0 || it.percent > 100) continue;
        payload += std::to_string(static_cast<int>(it.kind));
        payload += '\t';
        payload += WideToUtf8(safeKey);
        payload += '\t';
        payload += WideToUtf8(safeName);
        payload += '\t';
        payload += std::to_string(it.percent);
        payload += '\t';
        payload += std::to_string(it.lastSeenUnix);
        payload += '\n';
    }

    const std::wstring tmpPath = path + L".tmp";
    if (IsReparsePoint(tmpPath)) return;

    HANDLE h = CreateFileW(tmpPath.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS,
                           FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT, nullptr);
    if (h == INVALID_HANDLE_VALUE) return;

    DWORD written = 0;
    const BOOL ok = payload.empty() || WriteFile(h, payload.data(), static_cast<DWORD>(payload.size()), &written, nullptr);
    CloseHandle(h);
    if (!ok || written != payload.size()) {
        DeleteFileW(tmpPath.c_str());
        return;
    }
    MoveFileExW(tmpPath.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING);
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

bool DeviceHistory::RemoveEntry(DeviceKind kind, const std::wstring& key) {
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

std::wstring DeviceHistory::FormatRelativeTime(int64_t lastSeenUnix) {
    const int64_t now = NowUnixSec();
    if (lastSeenUnix <= 0 || now <= lastSeenUnix) return L"только что";
    const int64_t diff = now - lastSeenUnix;
    if (diff < 60) return L"только что";
    if (diff < 3600) return std::to_wstring(diff / 60) + L" мин назад";
    if (diff < 86400) return std::to_wstring(diff / 3600) + L" ч назад";
    if (diff < 172800) return L"вчера";
    return std::to_wstring(diff / 86400) + L" дн. назад";
}