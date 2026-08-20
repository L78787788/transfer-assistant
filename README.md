# 传输助手 (Transfer Assistant)

<p align="center">
  <strong>面向 Windows 10/11 x64 与 Android 9+ arm64 的极速局域网原生跨端文件传输应用</strong>
</p>

<p align="center">
  <a href="README_EN.md"><strong>English Documentation</strong></a> •
  <a href="#-功能特性">功能特性</a> •
  <a href="#-快速下载安装">下载安装</a> •
  <a href="#-使用指南">使用指南</a> •
  <a href="#-核心技术架构">技术架构</a> •
  <a href="#-开发与构建">开发构建</a> •
  <a href="#-开源协议">开源协议</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024%20Edition-orange?logo=rust" alt="Rust 2024" />
  <img src="https://img.shields.io/badge/Flutter-3.44+-blue?logo=flutter" alt="Flutter 3.44+" />
  <img src="https://img.shields.io/badge/Security-TLS%201.3%20mTLS-green?logo=lock" alt="TLS 1.3" />
  <img src="https://img.shields.io/badge/Integrity-BLAKE3-blueviolet" alt="BLAKE3" />
  <img src="https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-lightgrey" alt="License" />
</p>

---

## 📖 项目介绍

**传输助手（Transfer Assistant）** 是一款专为局域网内极速、安全、跨端互传而设计的现代化原生应用。

它彻底摒弃了传统文件传输工具对外部云端、公网服务器、中继节点或登录账号的依赖，利用局域网 mDNS 自动发现与 TLS 1.3 双向证书加密，实现 **Windows 电脑** 与 **Android 手机/平板** 之间的秒级互联与全速物理吞吐。

### 🌟 核心优势与特色

- ⚡ **无上限海量极速传输**：单次传输支持任意数量的文件与多层嵌套文件夹树，**传输文件数量无上限**；4 通道并发 TCP 数据流水线配合 Work-Stealing 调度，传输吞吐直达局域网物理极限（100+ MB/s）。
- 🚀 **真·零内存拷贝**：Rust 原生内核直接读写底层文件系统与 Socket，大文件数据流**零经过 Dart/Java 虚拟机堆内存**，永不触发垃圾回收（GC）卡顿或内存溢出（OOM）。
- 🛡️ **金融级双向安全认证**：基于 Ed25519 硬件级凭据与 TLS 1.3 双向证书认证（mTLS），首次连接两端根据握手证书确定性生成一致的 **6 位对称安全验证码**，杜绝局域网窃听与中间人攻击。
- 🔄 **坚如磐石的断点续传**：4 MiB 块划分与 BLAKE3 逐块哈希校验（先校验后落盘）；网络中断、Wi-Fi 切换或进程重启后，自动基于 SQLite WAL 断点位图比对，**100% 只传输缺失数据块**。
- 💻 **Windows 深度原生集成**：支持全窗口文件/文件夹拖拽分发、系统托盘最小化与实时速率 Tooltip、资源管理器右键“使用互传发送”快捷菜单、安装时自动配置专用网络防火墙入站规则。
- 📱 **Android 深度系统适配**：严格适配 Android 9~15，支持 SAF（存储访问框架）安全转移文件描述符（FD）所有权，配备前台常驻进度通知、Wi-Fi 高性能锁与 MulticastLock 多播锁深度保活，支持系统分享面板一键直达。
- 📋 **即时便签与局域网直连**：设备间即时发送文本便签与代码段；当路由器禁用组播时，支持输入 `IP:端口` 跨子网直连与热点 AP 互通。

---

## ✨ 功能特性矩阵

| 功能模块 | 特性描述 |
|---|---|
| 🔍 **自动发现与 IP 直连** | 基于 `_transassist._tcp.local.` mDNS 协议自动扫描局域网在线设备；支持输入 `IP:端口` 手动直连及离线热点 AP 模式。 |
| 🛡️ **双向安全配对** | 首次连接两端同时浮现一致的 6 位安全验证码，支持一键信任设备并持久化固定证书指纹（可随时管理或清空）。 |
| ⚡ **多通道并发传输** | 最多 4 条并行 TCP 数据通道，配合 Work-Stealing 任务池与预取双缓冲流水线，充分利用多核与带宽。 |
| 📦 **无上限海量文件支持** | 单次传输条目数量无上限，完美保留深层多层嵌套目录与空目录；严格拦截路径穿越、绝对路径与 Windows 重解析点。 |
| 🔄 **任务控制与断点恢复** | 支持实时暂停、继续、取消与重试；传输进度每秒测速并平滑节流更新，取消时自动清理 `.part` 临时文件。 |
| 📱 **Android 深度保活** | 支持 SAF 文件描述符安全所有权转移，配备前台服务、常驻进度通知与 Wi-Fi 高性能锁，支持系统分享菜单接收。 |
| 💻 **Windows 深度集成** | 原生托盘最小化、托盘 Tooltip 实时显示传输百分比与速率、资源管理器右键快捷菜单集成。 |
| 📋 **即时便签流** | 支持设备间直接发送即时文本便签与长文本；支持在 Windows 窗口任意区域拖拽文件直接分发。 |

---

## 📥 快速下载安装

前往 [GitHub Releases](https://github.com/L78787788/transfer-assistant/releases/latest) 下载最新正式版安装包：

### 1. Windows 端 (Windows 10 / 11 64位)
- 下载 `transfer-assistant-1.0.0-windows-x64-setup.exe`。
- 双击安装程序，按照指引安装即可（安装程序会自动配置专用网络防火墙入站规则）。

### 2. Android 端 (Android 9.0+ arm64)
- 下载 `transfer-assistant-1.0.0-android-arm64.apk`。
- 在 Android 设备上安装并授予必要的本地网络与通知权限。

### 校验和比对 (SHA256SUMS)
```text
a6ee6cd61b30dbea8ff8267a6a63556471f518d9c1b29fc4fb61c696c6facc00  transfer-assistant-1.0.0-android-arm64.apk
47756466e00be08ebb8d7ce81420312cd9d7f58b262317d1a0cc4daec6f46878  transfer-assistant-1.0.0-windows-x64-setup.exe
```

---

## 🚀 使用指南

### 场景一：自动发现与跨端互传文件 / 文件夹

1. **接入同一网络**：将手机和电脑连接到同一个 Wi-Fi（或电脑连接手机开启的移动热点）。
2. **打开应用**：在两端分别启动「传输助手」，应用会自动通过 mDNS 发现局域网内的对方设备。
3. **发起传输**：
   - 在主界面「附近设备」中点击目标设备卡片；
   - 点击底部的「发送文件」或「发送文件夹」，选择要发送的内容（支持无上限批量文件）；
   - 或在电脑端直接将任意文件/文件夹拖拽进窗口。
4. **配对确认（仅首次）**：
   - 首次连接时，两端屏幕会同时浮现相同的 **6 位安全配对码**；
   - 核对两端数字一致后点击「确认接收」，勾选「记住此设备」后下次免配对。
5. **极速传输**：传输将在后台以 4 路并行通道高速进行，通知栏/托盘将实时显示传输速度与进度。

```
┌──────────────┐                               ┌──────────────┐
│  Windows 电脑 │ ──── mDNS 局域网无感发现 ──── │  Android 手机 │
│              │ ◄─── TLS 1.3 双向证书认证 ───► │              │
│  [传输助手]   │ ──── 4路并行通道 (BLAKE3) ──► │  [传输助手]   │
└──────────────┘                               └──────────────┘
```

---

### 场景二：跨网段或手动 IP 直连

当路由器禁用了 mDNS 多播组播广播时：
1. 点击顶栏右上角的 **「+」号（手动直连）** 按钮；
2. 输入对端设备显示的 IP 地址与端口号（例如 `192.168.1.100:53317`）；
3. 点击连接即可立即建立直连传输。

---

### 场景三：即时文本便签发送

1. 进入目标设备的专属会话或便签输入框；
2. 粘贴或输入文字、链接、代码段；
3. 点击发送，对端将瞬间接收到该文本便签，并支持一键复制或在系统文本编辑器中打开。

---

### 场景四：Windows 资源管理器右键快速发送

1. 在「设置」中开启 **「Windows 资源管理器右键菜单」** 开关；
2. 在任意文件或文件夹上点击鼠标右键，选择 **「使用互传发送」**；
3. 传输助手将自动载入所选文件，点击目标设备即可直接发出。

---

## 🏗️ 核心技术架构

```
┌─────────────────────────────────────────────────────────────┐
│ Flutter UI (Material 3 / Glassmorphism / 响应式布局)         │
│  ├─ 附近设备 (Nearby & Discovery)                            │
│  ├─ 传输任务流转 (Transfers Pipeline)                         │
│  ├─ 历史记录与全文搜索 (History & Search)                    │
│  └─ 系统设置与设备信任 (Settings & Pinned Peers)             │
└──────────────────────────────┬──────────────────────────────┘
                               │ JSON 命令 / 响应 / 事件通知 (零数据流经 Dart)
┌──────────────────────────────▼──────────────────────────────┐
│ C-ABI / Dart FFI 胶水层                                     │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│ Rust TransferCore 原生内核                                   │
│  ├─ Tokio 异步多线程运行时 (TCP 53317 / 自动端口避让)         │
│  ├─ mDNS 局域网服务发现与多网卡智能过滤                      │
│  ├─ rustls TLS 1.3 双向证书加密 (mTLS) & 6位对称配对码       │
│  ├─ Protobuf 控制协议 (1 MiB 有界缓冲帧)                     │
│  ├─ 4 通道并行传输流水线 (Work-Stealing 任务池 + 预取双缓冲)  │
│  ├─ 4 MiB BLAKE3 逐块哈希校验 (先校验后落盘)                 │
│  ├─ SQLite WAL 状态与断点位图持久化                          │
│  └─ 路径安全清洗 (严格拦截 ..、绝对路径、Windows 保留设备名)   │
└──────────────┬──────────────────────────────┬───────────────┘
               │                              │
┌──────────────▼──────────────┐ ┌─────────────▼───────────────┐
│ Windows C++ 原生适配        │ │ Android Kotlin/JNI 原生适配  │
│  ├─ 本地 Direct I/O 文件流   │ │  ├─ SAF 文档流与目录树递归  │
│  ├─ 系统托盘与实时速率提示   │ │  ├─ FD 所有权安全转移与重开 │
│  ├─ 资源管理器右键菜单扩展   │ │  ├─ Keystore 硬件级凭据包装 │
│  └─ DPAPI 本地密钥保护       │ │  └─ 前台服务与系统锁保活   │
└─────────────────────────────┘ └─────────────────────────────┘
```

---

## 🛠️ 开发与构建指南

### 前置环境要求
- **Flutter** 3.44+ / **Dart** 3.12+
- **Rust** 1.97+，安装 Android 交叉编译目标：`rustup target add aarch64-linux-android`
- **Android SDK**、**NDK 29.0+**、**JDK 17**
- **Visual Studio 2022**（包含 C++ 桌面开发工作载荷）
- **Inno Setup 6**（仅打包 Windows 安装包时需要）

### 本地测试与检查
```powershell
# 1. 检查 Rust 代码规范与全量测试套件 (36 个测试)
cargo fmt --all -- --check
cargo clippy -p transfer_core --all-targets -- -D warnings
cargo test --workspace

# 2. 检查 Flutter 端 (可在虚拟盘符 T:\app 下执行)
cd app
flutter analyze
flutter test
```

### 全量打包发布 (Windows Setup EXE + Android arm64 APK)
```powershell
# 运行一键全量自动化发布流水线
.\scripts\build-release.ps1 -Version 1.0.0
```
构建产物将输出至 `dist/` 目录并自动生成 `SHA256SUMS.txt` 校验和。

### 一键部署到 Android 设备
```powershell
# 通过 ADB 自动编译最新内核并部署到连接的手机
.\scripts\deploy-to-android.ps1
```

---

## 📄 开源协议

本项目采用 [MIT](LICENSE) OR [Apache-2.0](LICENSE) 双重开源许可证。
