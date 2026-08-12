# 传输助手 Flutter APP

此目录包含传输助手的 Flutter 界面和 Windows/Android 平台适配。项目架构、环境要求、检查命令和发布流程见仓库根目录的 `README.md`。

文件内容不得进入 Dart；界面只通过最小 JSON FFI 调用 `TransferCore` 并消费状态事件。Android 文件由 SAF 选择后将文件描述符交给 Rust，Windows 文件路径由 Rust 直接打开。
