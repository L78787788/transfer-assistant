#include "platform_channel.h"

#include <flutter/method_channel.h>
#include <flutter/standard_method_codec.h>
#include <shlobj.h>
#include <shobjidl.h>
#include <wrl/client.h>

#include <string>
#include <vector>

#include "utils.h"

namespace {

using Microsoft::WRL::ComPtr;
using EncodableMap = flutter::EncodableMap;
using EncodableValue = flutter::EncodableValue;
using MethodResult = flutter::MethodResult<EncodableValue>;

std::string KnownFolder(REFKNOWNFOLDERID folder) {
  PWSTR path = nullptr;
  if (FAILED(::SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, nullptr, &path))) {
    return {};
  }
  std::string result = Utf8FromUtf16(path);
  ::CoTaskMemFree(path);
  return result;
}

std::string DisplayName(IShellItem* item) {
  PWSTR path = nullptr;
  if (FAILED(item->GetDisplayName(SIGDN_FILESYSPATH, &path))) {
    return {};
  }
  std::string result = Utf8FromUtf16(path);
  ::CoTaskMemFree(path);
  return result;
}

ComPtr<IFileOpenDialog> CreateOpenDialog(FILEOPENDIALOGOPTIONS extra_options) {
  ComPtr<IFileOpenDialog> dialog;
  if (FAILED(::CoCreateInstance(CLSID_FileOpenDialog, nullptr, CLSCTX_ALL,
                                IID_PPV_ARGS(&dialog)))) {
    return nullptr;
  }
  FILEOPENDIALOGOPTIONS options = 0;
  if (FAILED(dialog->GetOptions(&options)) ||
      FAILED(dialog->SetOptions(options | FOS_FORCEFILESYSTEM | extra_options))) {
    return nullptr;
  }
  return dialog;
}

void PickFiles(HWND window, std::unique_ptr<MethodResult> result) {
  auto dialog = CreateOpenDialog(FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST);
  if (!dialog) {
    result->Error("dialog", "无法创建文件选择窗口");
    return;
  }
  const HRESULT shown = dialog->Show(window);
  if (shown == HRESULT_FROM_WIN32(ERROR_CANCELLED)) {
    result->Success(EncodableValue(flutter::EncodableList{}));
    return;
  }
  if (FAILED(shown)) {
    result->Error("dialog", "文件选择窗口打开失败");
    return;
  }

  ComPtr<IShellItemArray> items;
  if (FAILED(dialog->GetResults(&items))) {
    result->Error("dialog", "无法读取所选文件");
    return;
  }
  DWORD count = 0;
  items->GetCount(&count);
  flutter::EncodableList values;
  for (DWORD index = 0; index < count; ++index) {
    ComPtr<IShellItem> item;
    if (SUCCEEDED(items->GetItemAt(index, &item))) {
      const std::string path = DisplayName(item.Get());
      if (!path.empty()) {
        const auto separator = path.find_last_of("\\/");
        const std::string name = separator == std::string::npos
                                     ? path
                                     : path.substr(separator + 1);
        values.emplace_back(EncodableMap{{EncodableValue("token"), EncodableValue(path)},
                                         {EncodableValue("displayName"), EncodableValue(name)}});
      }
    }
  }
  result->Success(EncodableValue(values));
}

std::string PickDirectory(HWND window) {
  auto dialog = CreateOpenDialog(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST);
  if (!dialog || FAILED(dialog->Show(window))) {
    return {};
  }
  ComPtr<IShellItem> item;
  return SUCCEEDED(dialog->GetResult(&item)) ? DisplayName(item.Get()) : std::string{};
}

bool SetContextMenu(bool enable) {
  wchar_t exe_path[MAX_PATH];
  ::GetModuleFileNameW(nullptr, exe_path, MAX_PATH);
  std::wstring command = L"\"" + std::wstring(exe_path) + L"\" \"%1\"";

  const wchar_t* file_key = L"Software\\Classes\\*\\shell\\TransferAssistant";
  const wchar_t* dir_key = L"Software\\Classes\\Directory\\shell\\TransferAssistant";

  if (enable) {
    HKEY key;
    if (::RegCreateKeyExW(HKEY_CURRENT_USER, file_key, 0, nullptr, 0, KEY_WRITE, nullptr, &key, nullptr) == ERROR_SUCCESS) {
      const wchar_t* title = L"使用传输助手发送";
      ::RegSetValueExW(key, nullptr, 0, REG_SZ, reinterpret_cast<const BYTE*>(title), static_cast<DWORD>((wcslen(title) + 1) * sizeof(wchar_t)));
      ::RegSetValueExW(key, L"Icon", 0, REG_SZ, reinterpret_cast<const BYTE*>(exe_path), static_cast<DWORD>((wcslen(exe_path) + 1) * sizeof(wchar_t)));
      HKEY cmd_key;
      if (::RegCreateKeyExW(key, L"command", 0, nullptr, 0, KEY_WRITE, nullptr, &cmd_key, nullptr) == ERROR_SUCCESS) {
        ::RegSetValueExW(cmd_key, nullptr, 0, REG_SZ, reinterpret_cast<const BYTE*>(command.c_str()), static_cast<DWORD>((command.length() + 1) * sizeof(wchar_t)));
        ::RegCloseKey(cmd_key);
      }
      ::RegCloseKey(key);
    }
    if (::RegCreateKeyExW(HKEY_CURRENT_USER, dir_key, 0, nullptr, 0, KEY_WRITE, nullptr, &key, nullptr) == ERROR_SUCCESS) {
      const wchar_t* title = L"使用传输助手发送";
      ::RegSetValueExW(key, nullptr, 0, REG_SZ, reinterpret_cast<const BYTE*>(title), static_cast<DWORD>((wcslen(title) + 1) * sizeof(wchar_t)));
      ::RegSetValueExW(key, L"Icon", 0, REG_SZ, reinterpret_cast<const BYTE*>(exe_path), static_cast<DWORD>((wcslen(exe_path) + 1) * sizeof(wchar_t)));
      HKEY cmd_key;
      if (::RegCreateKeyExW(key, L"command", 0, nullptr, 0, KEY_WRITE, nullptr, &cmd_key, nullptr) == ERROR_SUCCESS) {
        ::RegSetValueExW(cmd_key, nullptr, 0, REG_SZ, reinterpret_cast<const BYTE*>(command.c_str()), static_cast<DWORD>((command.length() + 1) * sizeof(wchar_t)));
        ::RegCloseKey(cmd_key);
      }
      ::RegCloseKey(key);
    }
  } else {
    ::RegDeleteTreeW(HKEY_CURRENT_USER, file_key);
    ::RegDeleteTreeW(HKEY_CURRENT_USER, dir_key);
  }
  return true;
}

bool IsContextMenuEnabled() {
  HKEY key;
  if (::RegOpenKeyExW(HKEY_CURRENT_USER, L"Software\\Classes\\*\\shell\\TransferAssistant", 0, KEY_READ, &key) == ERROR_SUCCESS) {
    ::RegCloseKey(key);
    return true;
  }
  return false;
}

static std::unique_ptr<flutter::MethodChannel<EncodableValue>> g_platform_channel;

}  // namespace

void RegisterPlatformChannel(flutter::FlutterEngine* engine, HWND window,
                             std::function<void(bool)> set_background_receive,
                             std::function<void(bool)> set_transfer_active,
                             std::function<void(const std::string&, const std::string&)> show_tray_notification,
                             std::function<void(const std::string&)> update_tray_status) {
  g_platform_channel = std::make_unique<flutter::MethodChannel<EncodableValue>>(
      engine->messenger(), "transassist/platform",
      &flutter::StandardMethodCodec::GetInstance());
  g_platform_channel->SetMethodCallHandler(
      [window, set_background_receive = std::move(set_background_receive),
       set_transfer_active = std::move(set_transfer_active),
       show_tray_notification = std::move(show_tray_notification),
       update_tray_status = std::move(update_tray_status)](
          const flutter::MethodCall<EncodableValue>& call,
          std::unique_ptr<MethodResult> result) {
        if (call.method_name() == "paths") {
          const std::string data = KnownFolder(FOLDERID_LocalAppData) + "\\传输助手";
          const std::string receive = KnownFolder(FOLDERID_Downloads) + "\\传输助手";
          result->Success(EncodableValue(EncodableMap{
              {EncodableValue("dataDirectory"), EncodableValue(data)},
              {EncodableValue("receiveDirectory"), EncodableValue(receive)},
          }));
        } else if (call.method_name() == "pickFiles") {
          PickFiles(window, std::move(result));
        } else if (call.method_name() == "pickDirectory") {
          const std::string path = PickDirectory(window);
          flutter::EncodableList values;
          if (!path.empty()) {
            const auto separator = path.find_last_of("\\/");
            const std::string name = separator == std::string::npos
                                         ? path
                                         : path.substr(separator + 1);
            values.emplace_back(EncodableMap{{EncodableValue("token"), EncodableValue(path)},
                                             {EncodableValue("displayName"), EncodableValue(name)}});
          }
          result->Success(EncodableValue(values));
        } else if (call.method_name() == "chooseReceiveDirectory") {
          const std::string path = PickDirectory(window);
          if (path.empty()) {
            result->Success();
          } else {
            result->Success(EncodableValue(path));
          }
        } else if (call.method_name() == "setBackgroundReceive") {
          const bool* enabled = std::get_if<bool>(call.arguments());
          if (enabled == nullptr) {
            result->Error("argument", "后台接收参数无效");
          } else {
            set_background_receive(*enabled);
            result->Success();
          }
        } else if (call.method_name() == "setTransferActive") {
          const bool* active = std::get_if<bool>(call.arguments());
          if (active == nullptr) {
            result->Error("argument", "传输活动参数无效");
          } else {
            set_transfer_active(*active);
            result->Success();
          }
        } else if (call.method_name() == "showNotification") {
          const auto* args = std::get_if<EncodableMap>(call.arguments());
          if (args != nullptr) {
            std::string title;
            std::string body;
            auto title_it = args->find(EncodableValue("title"));
            if (title_it != args->end() && std::holds_alternative<std::string>(title_it->second)) {
              title = std::get<std::string>(title_it->second);
            }
            auto body_it = args->find(EncodableValue("body"));
            if (body_it != args->end() && std::holds_alternative<std::string>(body_it->second)) {
              body = std::get<std::string>(body_it->second);
            }
            if (show_tray_notification) {
              show_tray_notification(title, body);
            }
          }
          result->Success();
        } else if (call.method_name() == "updateNotificationProgress") {
          const auto* args = std::get_if<EncodableMap>(call.arguments());
          if (args != nullptr && update_tray_status) {
            std::string title = "文件";
            std::string speed = "0 B/s";
            int percent = 0;
            bool active = false;

            auto title_it = args->find(EncodableValue("title"));
            if (title_it != args->end() && std::holds_alternative<std::string>(title_it->second)) {
              title = std::get<std::string>(title_it->second);
            }
            auto speed_it = args->find(EncodableValue("speed"));
            if (speed_it != args->end() && std::holds_alternative<std::string>(speed_it->second)) {
              speed = std::get<std::string>(speed_it->second);
            }
            auto percent_it = args->find(EncodableValue("percent"));
            if (percent_it != args->end() && std::holds_alternative<int>(percent_it->second)) {
              percent = std::get<int>(percent_it->second);
            }
            auto active_it = args->find(EncodableValue("active"));
            if (active_it != args->end() && std::holds_alternative<bool>(active_it->second)) {
              active = std::get<bool>(active_it->second);
            }

            if (active) {
              std::string status = "传输助手 · 传输中 " + std::to_string(percent) + "% (" + speed + ")";
              update_tray_status(status);
            } else {
              update_tray_status("传输助手");
            }
          }
          result->Success();
        } else if (call.method_name() == "setContextMenuEnabled") {
          const bool* enabled = std::get_if<bool>(call.arguments());
          if (enabled != nullptr) {
            SetContextMenu(*enabled);
            result->Success(EncodableValue(*enabled));
          } else {
            result->Error("argument", "参数无效");
          }
        } else if (call.method_name() == "isContextMenuEnabled") {
          result->Success(EncodableValue(IsContextMenuEnabled()));
        } else {
          result->NotImplemented();
        }
      });
}

void NotifyFilesDropped(const std::vector<std::string>& paths) {
  if (!g_platform_channel || paths.empty()) {
    return;
  }
  flutter::EncodableList list;
  for (const auto& p : paths) {
    const auto separator = p.find_last_of("\\/");
    const std::string name = separator == std::string::npos ? p : p.substr(separator + 1);
    list.emplace_back(EncodableMap{
        {EncodableValue("token"), EncodableValue(p)},
        {EncodableValue("displayName"), EncodableValue(name)},
    });
  }
  g_platform_channel->InvokeMethod("onFilesDropped", std::make_unique<EncodableValue>(list));
}
