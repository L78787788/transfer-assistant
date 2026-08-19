#ifndef RUNNER_PLATFORM_CHANNEL_H_
#define RUNNER_PLATFORM_CHANNEL_H_

#include <flutter/flutter_engine.h>
#include <windows.h>

#include <functional>
#include <string>
#include <vector>

void RegisterPlatformChannel(flutter::FlutterEngine* engine, HWND window,
                             std::function<void(bool)> set_background_receive,
                             std::function<void(bool)> set_transfer_active,
                             std::function<void(const std::string&, const std::string&)> show_tray_notification,
                             std::function<void(const std::string&)> update_tray_status);

void NotifyFilesDropped(const std::vector<std::string>& paths);

#endif  // RUNNER_PLATFORM_CHANNEL_H_
