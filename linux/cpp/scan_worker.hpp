#pragma once
#include "device.hpp"
#include "scanner.hpp"
#include <array>
#include <condition_variable>
#include <cstdint>
#include <dbus/dbus.h>
#include <mutex>
#include <sys/eventfd.h>
#include <thread>
#include <unistd.h>

class ScanWorker {
public:
    ScanWorker() {
        notifyFd_ = eventfd(0, EFD_NONBLOCK | EFD_CLOEXEC);
    }

    ~ScanWorker() {
        Stop();
        if (notifyFd_ >= 0) close(notifyFd_);
    }

    int NotifyFd() const { return notifyFd_; }

    void Start() {
        if (running_) return;
        running_ = true;
        worker_ = std::thread([this] { Run(); });
    }

    void RequestScan() {
        {
            std::lock_guard<std::mutex> lk(mu_);
            if (stop_) return;
            requested_ = true;
        }
        cv_.notify_one();
    }

    bool TakeSnapshot(std::array<DeviceState, 3>& out) {
        if (notifyFd_ >= 0) {
            uint64_t drained = 0;
            while (read(notifyFd_, &drained, sizeof(drained)) > 0) {}
        }
        std::lock_guard<std::mutex> lk(mu_);
        if (!hasResult_) return false;
        out = std::move(latest_);
        hasResult_ = false;
        return true;
    }

    void Stop() {
        if (!running_) return;
        {
            std::lock_guard<std::mutex> lk(mu_);
            stop_ = true;
        }
        cv_.notify_one();
        if (worker_.joinable()) worker_.join();
        running_ = false;
    }

private:
    void Run() {
        DBusError err;
        dbus_error_init(&err);
        DBusConnection* bus = dbus_bus_get_private(DBUS_BUS_SYSTEM, &err);
        dbus_error_free(&err);
        if (bus) {
            dbus_connection_set_exit_on_disconnect(bus, FALSE);
        }

        while (true) {
            {
                std::unique_lock<std::mutex> lk(mu_);
                cv_.wait(lk, [this] { return stop_ || requested_; });
                if (stop_) break;
                requested_ = false;
            }

            if (bus && !dbus_connection_get_is_connected(bus)) {
                dbus_connection_close(bus);
                dbus_connection_unref(bus);
                bus = nullptr;
            }
            if (!bus) {
                dbus_error_init(&err);
                bus = dbus_bus_get_private(DBUS_BUS_SYSTEM, &err);
                dbus_error_free(&err);
                if (bus) dbus_connection_set_exit_on_disconnect(bus, FALSE);
            }

            auto snap = ScanAllDevices(bus);
            {
                std::lock_guard<std::mutex> lk(mu_);
                if (stop_) break;
                latest_ = std::move(snap);
                hasResult_ = true;
            }

            if (notifyFd_ >= 0) {
                uint64_t one = 1;
                const ssize_t wr = write(notifyFd_, &one, sizeof(one));
                (void)wr;
            }
        }

        if (bus) {
            dbus_connection_close(bus);
            dbus_connection_unref(bus);
        }
    }

    int notifyFd_ = -1;
    bool running_ = false;
    std::mutex mu_;
    std::condition_variable cv_;
    bool stop_ = false;
    bool requested_ = false;
    bool hasResult_ = false;
    std::array<DeviceState, 3> latest_{};
    std::thread worker_;
};