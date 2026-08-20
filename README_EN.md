# Transfer Assistant

<p align="center">
  <strong>Ultra-fast, Zero-Cloud, Native Cross-Platform LAN File Transfer for Windows 10/11 x64 & Android 9+ arm64</strong>
</p>

<p align="center">
  <a href="README.md"><strong>中文说明文档 (Chinese)</strong></a> •
  <a href="#-features">Features</a> •
  <a href="#-quick-download--installation">Download</a> •
  <a href="#-user-guide">User Guide</a> •
  <a href="#-architecture">Architecture</a> •
  <a href="#-development--build">Development</a> •
  <a href="#-license">License</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Rust-2024%20Edition-orange?logo=rust" alt="Rust 2024" />
  <img src="https://img.shields.io/badge/Flutter-3.44+-blue?logo=flutter" alt="Flutter 3.44+" />
  <img src="https://img.shields.io/badge/Security-TLS%201.3%20mTLS-green?logo=lock" alt="TLS 1.3" />
  <img src="https://img.shields.io/badge/Integrity-BLAKE3-blueviolet" alt="BLAKE3" />
  <img src="https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-lightgrey" alt="License" />
</p>

---

## 📖 Introduction

**Transfer Assistant** is a modern, high-performance, and secure cross-platform local area network (LAN) file transfer application.

It completely eliminates reliance on external cloud servers, internet relays, third-party accounts, or telemetry. Leveraging local **mDNS zero-configuration discovery** and **TLS 1.3 mutual certificate encryption (mTLS)**, Transfer Assistant achieves lightning-fast discovery and saturated physical line-rate throughput (100+ MB/s) between **Windows PCs** and **Android phones/tablets**.

### 🌟 Key Highlights

- ⚡ **Unlimited Batch Transfers**: Transfer unlimited files and complex nested directory trees in a single batch. Up to 4 parallel TCP data channels with work-stealing scheduling maximize network and disk I/O bandwidth.
- 🚀 **True Zero-Copy Architecture**: The native Rust core streams binary bytes directly between the OS filesystem and network sockets, bypassing Dart and Java VM heap memories completely to prevent garbage collection pauses and out-of-memory errors.
- 🛡️ **Financial-Grade Security (mTLS 1.3)**: Built with Ed25519 hardware credentials and mutual TLS 1.3 certificate verification. First-time pairings generate an identical deterministic **6-digit symmetric pairing code** on both screens to eliminate eavesdropping and Man-in-the-Middle (MITM) attacks.
- 🔄 **Rock-Solid Resumption**: 4 MiB chunking with BLAKE3 per-chunk hash verification. In the event of network disruption, Wi-Fi switching, or application restarts, the SQLite WAL bitmap engine ensures that **only missing chunks are transferred**.
- 💻 **Deep Windows Integration**: Full window drag-and-drop distribution, system tray minimization with live transfer rate tooltips, Windows Explorer context menu ("Send with Transfer Assistant"), and automatic dedicated private network firewall rule management.
- 📱 **Deep Android Integration**: Fully adapted for Android 9~15. Safely transfers Storage Access Framework (SAF) File Descriptor (FD) ownership to the native Rust engine. Employs foreground services, persistent progress notifications, Wi-Fi high-performance locks, and MulticastLocks for resilient background operations.
- 📋 **Instant Notes & Manual Direct IP**: Real-time cross-device note/clipboard sharing. When multicast is restricted by enterprise routers, direct connections via `IP:Port` or portable Wi-Fi Hotspots are fully supported.

---

## ✨ Feature Matrix

| Feature | Description |
|---|---|
| 🔍 **Auto Discovery & Direct IP** | Automatically discovers LAN peers using `_transassist._tcp.local.` mDNS; supports manual `IP:Port` and offline Wi-Fi AP hotspot mode. |
| 🛡️ **Mutual Security Pairing** | Displays synchronized 6-digit verification codes upon first connection; supports one-click device pinning and certificate fingerprint management. |
| ⚡ **Multi-Channel Parallelism** | Up to 4 concurrent TCP data pipelines with work-stealing queues and double-buffering prefetch for optimal throughput. |
| 📦 **Unlimited File Batching** | No upper limits on file counts; preserves multi-level folder structures and empty directories while preventing directory traversal attacks. |
| 🔄 **Stateful Resumption** | Real-time pause, resume, cancel, and retry; smoothly throttled speed metrics; automatic cleanup of `.part` files on cancellation. |
| 📱 **Android Foreground Keep-Alive** | SAF FD ownership transfer, continuous notification updates, Wi-Fi high-perf locks, and system share menu (`ACTION_SEND`) integration. |
| 💻 **Windows Desktop Native** | System tray minimization with progress tooltips, full-window drag-and-drop, and Explorer context menu integration. |
| 📋 **Instant Notes Stream** | Direct instant text/code snippet sharing across devices with one-click copy and editor integration. |

---

## 📥 Quick Download & Installation

Download pre-compiled release packages from [GitHub Releases](https://github.com/L78787788/transfer-assistant/releases/latest):

### 1. Windows (Windows 10 / 11 64-bit)
- Download `transfer-assistant-1.0.0-windows-x64-setup.exe`.
- Run the setup wizard (automatically configures Windows Firewall rules for private networks).

### 2. Android (Android 9.0+ arm64)
- Download `transfer-assistant-1.0.0-android-arm64.apk`.
- Install on your device and grant local network and notification permissions.

### Integrity Verification (SHA256SUMS)
```text
a6ee6cd61b30dbea8ff8267a6a63556471f518d9c1b29fc4fb61c696c6facc00  transfer-assistant-1.0.0-android-arm64.apk
47756466e00be08ebb8d7ce81420312cd9d7f58b262317d1a0cc4daec6f46878  transfer-assistant-1.0.0-windows-x64-setup.exe
```

---

## 🚀 User Guide

### Scenario 1: Automatic Discovery & Cross-Device Transfer

1. **Connect to the same network**: Connect both devices to the same Wi-Fi network or mobile hotspot.
2. **Launch Application**: Open Transfer Assistant on both devices. mDNS will discover peers automatically.
3. **Initiate Transfer**:
   - Click on the discovered target peer card;
   - Tap "Send Files" or "Send Folder", or drag files directly into the Windows app window.
4. **Confirm Pairing (First Time Only)**:
   - Check that the **6-digit pairing code** matches on both screens;
   - Tap "Accept" and optionally check "Trust this device" for automated future transfers.
5. **High-Speed Transfer**: The transfer runs across 4 parallel channels. Progress and real-time speed are updated smoothly in the notification area and tray.

```
┌──────────────┐                               ┌──────────────┐
│  Windows PC  │ ──── mDNS Zero-Config LAN ───► │ Android Phone│
│              │ ◄─── TLS 1.3 Mutual Auth ────► │              │
│ [TransferCore]│ ──── 4 Parallel Channels ────► │ [TransferCore]│
└──────────────┘                               └──────────────┘
```

---

### Scenario 2: Direct IP Connection Across Subnets

When router multicast is disabled:
1. Click the **「+」 (Direct Connect)** icon in the top app bar;
2. Enter the peer IP and port (e.g. `192.168.1.100:53317`);
3. Tap connect to establish the TLS channel immediately.

---

### Scenario 3: Instant Text Notes & Snippets

1. Open the chat/session view for the target device;
2. Type or paste text, links, or code snippets into the input field;
3. Send instantly; the receiving device can copy with a single tap.

---

### Scenario 4: Windows Explorer Context Menu

1. Enable **"Windows Explorer Context Menu"** in Settings;
2. Right-click any file or folder in Windows Explorer and select **"Send with Transfer Assistant"**;
3. Choose the target device to dispatch immediately.

---

## 🏗️ Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ Flutter UI (Material 3 / Glassmorphism / Responsive Layout)  │
│  ├─ Nearby & Discovery Stream                                │
│  ├─ Transfers Pipeline & Throttled Metrics                   │
│  ├─ History & Full-Text Search                               │
│  └─ Settings & Trusted Peers Repository                      │
└──────────────────────────────┬──────────────────────────────┘
                               │ JSON Commands / Events (Zero file data in Dart)
┌──────────────────────────────▼──────────────────────────────┐
│ C-ABI / Dart FFI Glue Layer                                 │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│ Rust TransferCore Engine                                    │
│  ├─ Tokio Asynchronous Multi-threaded Runtime (TCP 53317)   │
│  ├─ mDNS Discovery & Multi-NIC Interface Filtering          │
│  ├─ rustls TLS 1.3 Mutual Authentication & 6-Digit Code     │
│  ├─ Protobuf Control Wire Protocol (1 MiB Bounded Frames)   │
│  ├─ 4-Channel Concurrent Pipeline (Work-Stealing Pool)      │
│  ├─ 4 MiB BLAKE3 Chunk Integrity Verification (Pre-Commit)  │
│  ├─ SQLite WAL State & Resume Bitmap Persistence            │
│  └─ Path Sanitization & Defense against Traversal Attacks   │
└──────────────┬──────────────────────────────┬───────────────┘
               │                              │
┌──────────────▼──────────────┐ ┌─────────────▼───────────────┐
│ Windows C++ Native Layer    │ │ Android Kotlin/JNI Layer    │
│  ├─ Local Direct File I/O   │ │  ├─ SAF Tree & Document I/O │
│  ├─ System Tray & Tooltips  │ │  ├─ Safe FD Ownership Hand-off
│  ├─ Explorer Shell Extension│ │  ├─ Android Keystore Crypto │
│  └─ DPAPI Credential Store  │ │  └─ Foreground Service Lock │
└─────────────────────────────┘ └─────────────────────────────┘
```

---

## 🛠️ Development & Build

### Prerequisites
- **Flutter** 3.44+ / **Dart** 3.12+
- **Rust** 1.97+ with target `rustup target add aarch64-linux-android`
- **Android SDK**, **NDK 29.0+**, **JDK 17**
- **Visual Studio 2022** (C++ Desktop Development workload)
- **Inno Setup 6** (for Windows installer compilation)

### Running Checks & Tests
```powershell
# 1. Check Rust formatting, clippy and 36 unit/integration tests
cargo fmt --all -- --check
cargo clippy -p transfer_core --all-targets -- -D warnings
cargo test --workspace

# 2. Run Flutter static analysis and widget test suite
cd app
flutter analyze
flutter test
```

### Full Release Build (Windows Setup EXE + Android APK)
```powershell
# Run the automated end-to-end release pipeline
.\scripts\build-release.ps1 -Version 1.0.0
```
Artifacts and `SHA256SUMS.txt` will be output into the `dist/` folder.

### Direct Deployment to Connected Android Device
```powershell
# Cross-compiles the latest Rust core and deploys APK via ADB
.\scripts\deploy-to-android.ps1
```

---

## 📄 License

This project is dual-licensed under [MIT](LICENSE) OR [Apache-2.0](LICENSE).
