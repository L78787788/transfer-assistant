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

}  // namespace

void RegisterPlatformChannel(flutter::FlutterEngine* engine, HWND window,
                             std::function<void(bool)> set_background_receive,
                             std::function<void(bool)> set_transfer_active) {
  auto channel = std::make_unique<flutter::MethodChannel<EncodableValue>>(
      engine->messenger(), "transassist/platform",
      &flutter::StandardMethodCodec::GetInstance());
  channel->SetMethodCallHandler(
      [window, set_background_receive = std::move(set_background_receive),
       set_transfer_active = std::move(set_transfer_active)](
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
        } else {
          result->NotImplemented();
        }
      });
}
