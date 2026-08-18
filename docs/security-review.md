# 传输助手 v1 安全审查报告

审查日期：2026-08-12
审查方式：Mimosa 深度静态扫描 + 人工聚焦代码审查

## 1. Mimosa 深度扫描

- 结果：**0 发现**（无已知漏洞、无风险假设）
- 覆盖：230 个依赖包，离线漏洞库匹配 0
- 封印：`sha256:fb4903d787dd1809900f9f5c0b396cde29fa429acb00fb66c5aa2dffee3d33d9`
- 边界：静态分析（未执行运行时行为验证）

## 2. 人工审查项

| 审查点 | 结论 | 证据 |
|---|---|---|
| TLS 双向证书与指纹固定 | ✅ | `wire.rs:127` validate_hello 强制证书指纹 == Hello 声明指纹；device_id == 指纹 hex；测试 `mutual_tls_exposes_peer_fingerprints` |
| 配对码基于双方握手记录 | ✅ | `wire.rs` pairing_code 基于双方指纹 + 排序后的 device_id，对称且确定性；测试 `pairing_code_is_symmetric_and_six_digits` |
| 网络长度在分配前限制 | ✅ | 控制帧 `MAX_CONTROL_FRAME_BYTES = 1 MiB`（protocol.rs:7），块头 `MAX_CHUNK_HEADER_BYTES = 64 KiB`（transfer.rs:13），均在读长度后先校验再分配；测试 `control_frames_round_trip_and_reject_oversized_input` |
| 路径穿越/非法名/保留名 | ✅ | `path_safety.rs` sanitize/suffix/unique；manifest.rs 拒绝 `..`；测试 `manifest_rejects_parent_directory_traversal`、`target_names_are_deterministic_and_never_overwrite` |
| JNI GlobalRef/FD 唯一所有权 | ✅ | 修复后 `AndroidStorageBridge.prepareTargets` 失败时关闭全部已 detach FD（adoptFd 包装）；Rust 侧 `File::from_raw_fd` 唯一持有；错误经 JSON 保留中文消息 |
| 暂停/取消/重试竞态 | ✅ | `JobControl.checkpoint` 统一检查点；接收端 `transfer_is_cancelled` 每块检查；测试 `cancel_during_concurrent_channels_leaves_no_partial_files`（3 轮并发通道取消） |
| 进度不超总大小 | ✅ | `core.rs:614` `saturating_add().min(total_bytes)`；重复块 `mark_complete` 幂等不重复累计；测试 `mark_complete_is_idempotent_and_never_inflates_counts` |
| 接收完成前不暴露最终文件 | ✅ | `incoming.rs` finalize_incoming：全部块校验后，Windows 先 sync_all 再原子 rename；Android 由 SAF 文档提供程序完成最终改名；重名拒绝覆盖 |
| 64 MiB 缓冲与四通道限制 | ✅ | `chunk.rs` 常量 + `run_outgoing` 通道数 `min(MAX_DATA_CHANNELS)`；不可随机访问降级单通道 |
| 未使用协议消息 | ✅ | `DataChannelHello`/`TransferControl` 保留在 proto（前向兼容）但代码不使用 |
| 源文件变化检测 | ✅ | 每块发送前 `verify_source_revision`（size:modified）；测试 `source_change_with_same_size_stops_the_transfer` |
| 损坏块拒绝 | ✅ | BLAKE3 先验证后落盘；测试 `corrupt_chunk_over_the_wire_is_rejected_without_write` |

## 3. 结论

未发现高优先级安全问题。静态扫描与自动化测试覆盖了全部核心安全路径；
真机场景（权限撤销、网络切换）仍需阶段 7 实机验证。
