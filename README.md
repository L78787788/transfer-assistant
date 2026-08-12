# 传输助手

传输助手是面向 Windows 10/11 x64 与 Android 9+ arm64 的局域网原生文件传输应用。Flutter 只负责界面，Rust `TransferCore` 直接处理发现、TLS 连接、文件读写、BLAKE3 校验、断点位图和 SQLite 持久化，文件内容不会经过 Dart。

## 功能

- Windows 与 Android 设备任意两端互传文件、文件夹和批量任务。
- mDNS 自动发现，并支持 `IP:端口` 手动连接。
- TLS 1.3 双向加密、六位验证码配对和设备证书指纹固定。
- 4 MiB 数据块、最多四条并行数据通道、暂停、取消、重试和断点续传。
- 文件夹结构与空目录保留，拒绝路径穿越、符号链接和 Windows 重解析点。
- Android 使用 SAF 和文件描述符，Windows 直接访问文件并支持托盘后台接收。

## 环境

- Flutter 3.44.6 / Dart 3.12.2
- Rust 1.97.1，安装 `aarch64-linux-android` 目标
- Android SDK、NDK `29.0.14206865`、JDK 17
- Visual Studio 2022 C++ Build Tools
- Inno Setup 6（仅发布安装器需要）

检查环境：

```powershell
.\scripts\check-environment.ps1
```

## 开发检查

```powershell
cargo fmt --all -- --check
cargo clippy -p transfer_core --all-targets -- -D warnings
cargo test --workspace

Set-Location app
flutter analyze
flutter test
```

包含中文字符的 Windows 路径可能影响 Flutter/Gradle。仓库本机使用 ASCII 盘符映射（例如 `subst T: <仓库路径>`）运行 Flutter 命令；发布脚本会自动建立并清理映射。

## 发布

首次发布会生成并持久保存 Android 发布密钥：

```powershell
.\scripts\ensure-android-signing.ps1
.\scripts\build-release.ps1 -Version 1.0.0
```

`app/android/transassist-release.jks` 和 `app/android/key.properties` 已被 Git 忽略。两者必须一同离线备份，丢失后将无法用同一应用身份发布升级。发布物写入 `dist/`，同时生成 `SHA256SUMS.txt`。没有商业 Windows 代码签名证书时，Inno Setup EXE 保持未签名。

性能验收使用真实设备组合执行：

```powershell
.\scripts\benchmark-transfer.ps1 `
  -IperfServer 192.168.1.20 `
  -SourceDirectory D:\BenchSource `
  -ReceiveDirectory E:\BenchReceive `
  -PrepareFile
```

脚本给出链路、发送盘读取和接收盘写入三者瓶颈的 80% 门槛。用 APP 传输生成的 10 GiB 不可压缩文件三次，再以 `-TransferSeconds` 输入稳定阶段耗时进行判定。
