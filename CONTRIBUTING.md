# 开发约定

- Rust 代码必须通过 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings` 和 `cargo test --all`。
- Dart 代码必须通过 `dart format --output=none --set-exit-if-changed .`、`flutter analyze` 和 `flutter test`。
- 测试从公开接口观察行为，不依赖私有实现或数据库内部结构。
- 文件数据只能由 Rust 和平台存储适配器处理，禁止通过 Dart FFI 传递文件块。
- 所有网络输入必须设置长度上限并在访问文件系统之前完成校验。
- 用户可见文案使用简体中文；标识符、协议字段和提交信息使用英文。
