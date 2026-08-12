#ifndef RUNNER_PLATFORM_CHANNEL_H_
#define RUNNER_PLATFORM_CHANNEL_H_

#include <flutter/flutter_engine.h>
#include <windows.h>

#include <functional>

void RegisterPlatformChannel(flutter::FlutterEngine* engine, HWND window,
                             std::function<void(bool)> set_background_receive,
                             std::function<void(bool)> set_transfer_active);

#endif  // RUNNER_PLATFORM_CHANNEL_H_
