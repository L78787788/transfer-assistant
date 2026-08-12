#include "flutter_window.h"

#include <optional>

#include "flutter/generated_plugin_registrant.h"
#include "platform_channel.h"
#include "resource.h"

namespace {
constexpr UINT kTrayMessage = WM_APP + 1;
}

FlutterWindow::FlutterWindow(const flutter::DartProject& project)
    : project_(project) {}

FlutterWindow::~FlutterWindow() {}

bool FlutterWindow::OnCreate() {
  if (!Win32Window::OnCreate()) {
    return false;
  }

  RECT frame = GetClientArea();

  // The size here must match the window dimensions to avoid unnecessary surface
  // creation / destruction in the startup path.
  flutter_controller_ = std::make_unique<flutter::FlutterViewController>(
      frame.right - frame.left, frame.bottom - frame.top, project_);
  // Ensure that basic setup of the controller was successful.
  if (!flutter_controller_->engine() || !flutter_controller_->view()) {
    return false;
  }
  RegisterPlugins(flutter_controller_->engine());
  RegisterPlatformChannel(
      flutter_controller_->engine(), GetHandle(),
      [this](bool enabled) { SetBackgroundReceive(enabled); },
      [this](bool active) { SetTransferActive(active); });
  SetChildContent(flutter_controller_->view()->GetNativeWindow());

  flutter_controller_->engine()->SetNextFrameCallback([&]() {
    this->Show();
  });

  // Flutter can complete the first frame before the "show window" callback is
  // registered. The following call ensures a frame is pending to ensure the
  // window is shown. It is a no-op if the first frame hasn't completed yet.
  flutter_controller_->ForceRedraw();

  return true;
}

void FlutterWindow::OnDestroy() {
  RemoveTrayIcon();
  if (flutter_controller_) {
    flutter_controller_ = nullptr;
  }

  Win32Window::OnDestroy();
}

LRESULT
FlutterWindow::MessageHandler(HWND hwnd, UINT const message,
                              WPARAM const wparam,
                              LPARAM const lparam) noexcept {
  // Give Flutter, including plugins, an opportunity to handle window messages.
  if (flutter_controller_) {
    std::optional<LRESULT> result =
        flutter_controller_->HandleTopLevelWindowProc(hwnd, message, wparam,
                                                      lparam);
    if (result) {
      return *result;
    }
  }

  switch (message) {
    case WM_CLOSE:
      if (background_receive_ || transfer_active_) {
        ::ShowWindow(hwnd, SW_HIDE);
        return 0;
      }
      break;
    case kTrayMessage:
      if (lparam == WM_LBUTTONUP || lparam == WM_LBUTTONDBLCLK) {
        ::ShowWindow(hwnd, SW_RESTORE);
        ::SetForegroundWindow(hwnd);
        return 0;
      }
      if (lparam == WM_RBUTTONUP) {
        // Show right-click context menu.
        POINT cursor;
        ::GetCursorPos(&cursor);
        ::SetForegroundWindow(hwnd);
        HMENU menu = ::CreatePopupMenu();
        ::AppendMenuW(menu, MF_STRING, IDM_TRAY_EXIT, L"退出");
        ::TrackPopupMenu(menu, TPM_RIGHTALIGN | TPM_BOTTOMALIGN | TPM_RIGHTBUTTON,
                         cursor.x, cursor.y, 0, hwnd, nullptr);
        ::DestroyMenu(menu);
        return 0;
      }
      break;
    case WM_COMMAND:
      if (LOWORD(wparam) == IDM_TRAY_EXIT && HIWORD(wparam) == 0) {
        RemoveTrayIcon();
        ::PostQuitMessage(0);
        return 0;
      }
      break;
    case WM_FONTCHANGE:
      flutter_controller_->engine()->ReloadSystemFonts();
      break;
  }

  return Win32Window::MessageHandler(hwnd, message, wparam, lparam);
}

void FlutterWindow::SetBackgroundReceive(bool enabled) {
  if (background_receive_ == enabled) {
    return;
  }
  background_receive_ = enabled;
  UpdateTrayIcon();
}

void FlutterWindow::SetTransferActive(bool active) {
  if (transfer_active_ == active) {
    return;
  }
  transfer_active_ = active;
  UpdateTrayIcon();
}

void FlutterWindow::UpdateTrayIcon() {
  if (!background_receive_ && !transfer_active_) {
    RemoveTrayIcon();
    return;
  }
  if (tray_icon_.cbSize != 0) {
    return;
  }

  tray_icon_ = {};
  tray_icon_.cbSize = sizeof(NOTIFYICONDATA);
  tray_icon_.hWnd = GetHandle();
  tray_icon_.uID = 1;
  tray_icon_.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
  tray_icon_.uCallbackMessage = kTrayMessage;
  tray_icon_.hIcon = static_cast<HICON>(::LoadImage(
      ::GetModuleHandle(nullptr), MAKEINTRESOURCE(IDI_APP_ICON), IMAGE_ICON, 16,
      16, LR_DEFAULTCOLOR));
  wcscpy_s(tray_icon_.szTip, L"传输助手");
  ::Shell_NotifyIcon(NIM_ADD, &tray_icon_);
}

void FlutterWindow::RemoveTrayIcon() {
  if (tray_icon_.cbSize != 0) {
    ::Shell_NotifyIcon(NIM_DELETE, &tray_icon_);
    tray_icon_ = {};
  }
}
