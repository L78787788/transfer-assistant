import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';
import '../theme/app_theme.dart';

class NearbyView extends StatelessWidget {
  const NearbyView({super.key, required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    return _ChatStreamView(controller: controller);
  }
}

// =========================================================================
// 流式传输会话流 (Chat Stream · 纯净现代大厂质感)
// 先展示设备列表卡片，点击设备后进入该设备的专属流式传输会话
// =========================================================================
class _ChatStreamView extends StatefulWidget {
  const _ChatStreamView({required this.controller});

  final AppController controller;

  @override
  State<_ChatStreamView> createState() => _ChatStreamViewState();
}

class _ChatStreamViewState extends State<_ChatStreamView> {
  PeerSummary? _activeChatPeer;

  @override
  Widget build(BuildContext context) {
    // 如果当前进入了某个设备的会话窗口，渲染会话详情页
    if (_activeChatPeer != null) {
      // 检查该设备是否仍在列表中，若更新则保持引用
      final currentPeer = widget.controller.peers.firstWhere(
        (p) => p.id == _activeChatPeer!.id,
        orElse: () => _activeChatPeer!,
      );

      return _PeerChatSessionView(
        controller: widget.controller,
        peer: currentPeer,
        onBack: () => setState(() => _activeChatPeer = null),
      );
    }

    // 默认展示设备会话联系人列表
    return _ChatDeviceListView(
      controller: widget.controller,
      onSelectPeer: (peer) {
        setState(() => _activeChatPeer = peer);
      },
    );
  }
}

/// 一级视图：已发现设备会话联系人列表
class _ChatDeviceListView extends StatelessWidget {
  const _ChatDeviceListView({
    required this.controller,
    required this.onSelectPeer,
  });

  final AppController controller;
  final ValueChanged<PeerSummary> onSelectPeer;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final peers = controller.peers;

    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1080),
        child: ListView(
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
          children: [
            // 顶栏：标题与全局快捷操作
            Row(
              children: [
                Text(
                  '传输会话',
                  style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                        fontWeight: FontWeight.w800,
                        letterSpacing: -0.5,
                      ),
                ),
                const SizedBox(width: 10),
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 3),
                  decoration: BoxDecoration(
                    color: colors.primary.withValues(alpha: isDark ? 0.20 : 0.12),
                    borderRadius: BorderRadius.circular(999),
                    border: Border.all(
                      color: colors.primary.withValues(alpha: isDark ? 0.35 : 0.25),
                    ),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 6,
                        height: 6,
                        decoration: const BoxDecoration(
                          color: Color(0xff10b981),
                          shape: BoxShape.circle,
                        ),
                      ),
                      const SizedBox(width: 6),
                      Text(
                        '${peers.length} 台在线',
                        style: TextStyle(
                          fontSize: 11,
                          fontWeight: FontWeight.w700,
                          color: colors.primary,
                        ),
                      ),
                    ],
                  ),
                ),
                const Spacer(),
                IconButton.filledTonal(
                  onPressed: () => _showHotspotGuide(context, controller),
                  tooltip: '离线热点/AP模式直连',
                  icon: const Icon(LucideIcons.wifi, size: 18),
                  style: IconButton.styleFrom(
                    minimumSize: const Size(40, 40),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                  ),
                ),
                const SizedBox(width: 8),
                IconButton.filledTonal(
                  onPressed: () => _showManualConnect(context, controller),
                  tooltip: 'IP 直连',
                  icon: const Icon(LucideIcons.plus, size: 18),
                  style: IconButton.styleFrom(
                    minimumSize: const Size(40, 40),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                  ),
                ),
                const SizedBox(width: 8),
                IconButton.filledTonal(
                  onPressed: controller.isRefreshing ? null : controller.refreshPeers,
                  tooltip: '刷新设备列表',
                  style: IconButton.styleFrom(
                    minimumSize: const Size(40, 40),
                    shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                  ),
                  icon: controller.isRefreshing
                      ? const SizedBox.square(
                          dimension: 16,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : const Icon(LucideIcons.refreshCw, size: 18),
                ),
              ],
            ),
            const SizedBox(height: 14),

            // 顶部 Hero 卡片（品牌渐变背景 + 6px 水平微动效双向箭头）
            _NearbyHeroCard(controller: controller),
            const SizedBox(height: 18),

            if (controller.sharedTextPayload != null ||
                controller.sharedFileSources != null) ...[
              _ShareDraftBanner(controller: controller),
              const SizedBox(height: 14),
            ],

            // 设备列表
            if (peers.isEmpty)
              EmptyState(
                title: '正在扫描局域网设备...',
                description:
                    '请确保收发两端连接到同一个 Wi-Fi 或局域网，并均已启动「传输助手」。在没有局域网的环境下，可点击右上角开启「离线热点/AP 模式」。',
                action: FilledButton.icon(
                  onPressed: controller.refreshPeers,
                  icon: const Icon(LucideIcons.refreshCw, size: 16),
                  label: const Text('重新扫描'),
                ),
              )
            else
              ...peers.map((peer) {
                final peerTransfers = controller.transfers
                    .where((t) => t.peerName == peer.name)
                    .toList(growable: false);
                final latestTransfer =
                    peerTransfers.isNotEmpty ? peerTransfers.first : null;
                final isPhone = peer.deviceKind == DeviceKind.phone;

                return Container(
                  margin: const EdgeInsets.only(bottom: 10),
                  child: Material(
                    color: isDark ? const Color(0xff131c2e) : Colors.white,
                    borderRadius: BorderRadius.circular(16),
                    clipBehavior: Clip.antiAlias,
                    child: InkWell(
                      onTap: () => onSelectPeer(peer),
                      child: Container(
                        padding: const EdgeInsets.all(16),
                        decoration: BoxDecoration(
                          borderRadius: BorderRadius.circular(16),
                          border: Border.all(
                            color: peer.trusted
                                ? colors.primary.withValues(alpha: isDark ? 0.4 : 0.3)
                                : (isDark ? const Color(0xff1e2a44) : const Color(0xffe2e8f0)),
                            width: peer.trusted ? 1.5 : 1,
                          ),
                          boxShadow: [
                            BoxShadow(
                              color: Colors.black.withValues(alpha: isDark ? 0.2 : 0.04),
                              blurRadius: 10,
                              offset: const Offset(0, 2),
                            ),
                          ],
                        ),
                        child: Row(
                          children: [
                            // 设备头像 + 在线绿点
                            Stack(
                              children: [
                                Container(
                                  width: 48,
                                  height: 48,
                                  decoration: BoxDecoration(
                                    shape: BoxShape.circle,
                                    color: colors.primary.withValues(alpha: 0.12),
                                    border: Border.all(
                                      color: colors.primary.withValues(alpha: 0.3),
                                      width: 1.5,
                                    ),
                                  ),
                                  child: Icon(
                                    isPhone
                                        ? LucideIcons.smartphone
                                        : LucideIcons.monitor,
                                    color: colors.primary,
                                    size: 22,
                                  ),
                                ),
                                Positioned(
                                  right: 0,
                                  bottom: 0,
                                  child: Container(
                                    width: 12,
                                    height: 12,
                                    decoration: BoxDecoration(
                                      color: const Color(0xff10b981),
                                      shape: BoxShape.circle,
                                      border: Border.all(
                                        color: isDark
                                            ? const Color(0xff131c2e)
                                            : Colors.white,
                                        width: 2,
                                      ),
                                    ),
                                  ),
                                ),
                              ],
                            ),
                            const SizedBox(width: 14),
                            // 设备名称与 IP 摘要
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Row(
                                    children: [
                                      Text(
                                        peer.name,
                                        style: const TextStyle(
                                          fontSize: 15,
                                          fontWeight: FontWeight.w700,
                                          letterSpacing: -0.2,
                                        ),
                                      ),
                                      if (peer.trusted) ...[
                                        const SizedBox(width: 8),
                                        Container(
                                          padding: const EdgeInsets.symmetric(
                                            horizontal: 7,
                                            vertical: 2,
                                          ),
                                          decoration: BoxDecoration(
                                            color: colors.primary
                                                .withValues(alpha: isDark ? 0.2 : 0.12),
                                            borderRadius:
                                                BorderRadius.circular(6),
                                          ),
                                          child: Text(
                                            '已信任',
                                            style: TextStyle(
                                              fontSize: 10,
                                              fontWeight: FontWeight.w700,
                                              color: colors.primary,
                                            ),
                                          ),
                                        ),
                                      ],
                                    ],
                                  ),
                                  const SizedBox(height: 4),
                                  Row(
                                    children: [
                                      Text(
                                        peer.address,
                                        style: TextStyle(
                                          fontFamily: 'monospace',
                                          fontSize: 11,
                                          color: colors.onSurfaceVariant,
                                        ),
                                      ),
                                      if (latestTransfer != null) ...[
                                        const SizedBox(width: 8),
                                        Text(
                                          '· 上次传输: ${formatBytes(latestTransfer.totalBytes)}',
                                          style: TextStyle(
                                            fontSize: 11,
                                            color: colors.onSurfaceVariant,
                                          ),
                                        ),
                                      ],
                                    ],
                                  ),
                                ],
                              ),
                            ),
                            const SizedBox(width: 8),
                            // 进入传输会话按钮
                            FilledButton.tonalIcon(
                              onPressed: () => onSelectPeer(peer),
                              icon: const Icon(LucideIcons.arrowRight, size: 14),
                              label: const Text('发送'),
                              style: FilledButton.styleFrom(
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 14,
                                  vertical: 8,
                                ),
                                textStyle: const TextStyle(
                                  fontSize: 12,
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                );
              }),
          ],
        ),
      ),
    );
  }
}

/// 顶部 Hero 卡片（规范规格：两端即达 · 一键互传 + 说明 + CTA + 6px 水平微动效大双向箭头）
class _NearbyHeroCard extends StatelessWidget {
  const _NearbyHeroCard({required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(20),
        gradient: const LinearGradient(
          colors: [AppTheme.brand400, AppTheme.brand600],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        ),
        boxShadow: const [
          BoxShadow(
            color: Color(0x400ea5e9),
            blurRadius: 20,
            offset: Offset(0, 6),
          ),
        ],
      ),
      clipBehavior: Clip.antiAlias,
      child: Stack(
        children: [
          // 右侧背景装饰动效图形（IgnorePointer 彻底杜绝遮挡任何点击事件）
          const Positioned(
            right: -12,
            bottom: -12,
            child: IgnorePointer(
              child: Opacity(
                opacity: 0.18,
                child: BrandHeroGraphic(size: 110),
              ),
            ),
          ),
          // 卡片主要交互与文本内容区（独占 100% 宽度，排版规整舒展）
          Padding(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                const Text(
                  '两端即达 · 一键互传',
                  style: TextStyle(
                    fontSize: 18,
                    fontWeight: FontWeight.w800,
                    color: Colors.white,
                    letterSpacing: -0.3,
                  ),
                ),
                const SizedBox(height: 5),
                Text(
                  '基于局域网 TLS 1.3 双向加密与 4 路多通道并发引擎，无需外网流量即可全速互传。',
                  style: TextStyle(
                    fontSize: 12,
                    color: Colors.white.withValues(alpha: 0.92),
                    height: 1.45,
                  ),
                ),
                const SizedBox(height: 16),
                // 两个并排完整体积触控按钮（平分宽度，全域可点）
                Row(
                  children: [
                    Expanded(
                      child: FilledButton.icon(
                        onPressed: () => _showHotspotGuide(context, controller),
                        icon: const Icon(LucideIcons.wifi, size: 15),
                        label: const Text('离线热点向导'),
                        style: FilledButton.styleFrom(
                          backgroundColor: Colors.white,
                          foregroundColor: AppTheme.brand700,
                          minimumSize: const Size(0, 42),
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          textStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
                          elevation: 0,
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: OutlinedButton.icon(
                        onPressed: () => _showManualConnect(context, controller),
                        icon: const Icon(LucideIcons.plus, size: 15),
                        label: const Text('IP 直连'),
                        style: OutlinedButton.styleFrom(
                          foregroundColor: Colors.white,
                          backgroundColor: Colors.white.withValues(alpha: 0.12),
                          side: BorderSide(color: Colors.white.withValues(alpha: 0.65), width: 1.2),
                          minimumSize: const Size(0, 42),
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          textStyle: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
                          shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
                        ),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// 二级视图：与选定设备的专属传输对话会话界面
class _PeerChatSessionView extends StatefulWidget {
  const _PeerChatSessionView({
    required this.controller,
    required this.peer,
    required this.onBack,
  });

  final AppController controller;
  final PeerSummary peer;
  final VoidCallback onBack;

  @override
  State<_PeerChatSessionView> createState() => _PeerChatSessionViewState();
}

class _PeerChatSessionViewState extends State<_PeerChatSessionView> {
  final _textController = TextEditingController();

  @override
  void dispose() {
    _textController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isPhone = widget.peer.deviceKind == DeviceKind.phone;

    return Column(
      children: [
        // 会话顶栏：返回按钮与对方信息
        Container(
          padding: const EdgeInsets.fromLTRB(8, 6, 16, 8),
          decoration: BoxDecoration(
            color: colors.surface,
            border: Border(bottom: BorderSide(color: colors.outline.withValues(alpha: 0.25))),
          ),
          child: Row(
            children: [
              IconButton(
                onPressed: widget.onBack,
                tooltip: '返回设备列表',
                icon: const Icon(LucideIcons.arrowLeft, size: 20),
              ),
              const SizedBox(width: 4),
              Icon(
                isPhone ? LucideIcons.smartphone : LucideIcons.monitor,
                color: colors.primary,
                size: 20,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      widget.peer.name,
                      style: const TextStyle(
                        fontSize: 14,
                        fontWeight: FontWeight.w700,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    Text(
                      '${widget.peer.address} · 局域网传输中',
                      style: TextStyle(
                        fontSize: 10,
                        color: colors.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
              if (widget.controller.sharedFileSources != null ||
                  widget.controller.sharedTextPayload != null)
                FilledButton.tonalIcon(
                  onPressed: () {
                    if (widget.controller.sharedFileSources != null) {
                      final sources = widget.controller.sharedFileSources!;
                      widget.controller.clearSharedDraft();
                      widget.controller.sendSourcesToPeer(widget.peer, sources);
                    } else if (widget.controller.sharedTextPayload != null) {
                      final text = widget.controller.sharedTextPayload!;
                      widget.controller.clearSharedDraft();
                      widget.controller.sendTextMessage(widget.peer.id, text);
                    }
                  },
                  icon: const Icon(LucideIcons.send, size: 14),
                  label: const Text('发送暂存草稿'),
                  style: FilledButton.styleFrom(visualDensity: VisualDensity.compact),
                ),
            ],
          ),
        ),

        // 中间传输时间线气泡列表
        Expanded(
          child: _ChatBubbleTimeline(
            controller: widget.controller,
            peer: widget.peer,
          ),
        ),

        // 底部发送工具栏与文字输入框
        Container(
          padding: const EdgeInsets.fromLTRB(14, 8, 14, 12),
          decoration: BoxDecoration(
            color: colors.surface,
            border: Border(top: BorderSide(color: colors.outline.withValues(alpha: 0.25))),
          ),
          child: Row(
            children: [
              IconButton(
                onPressed: () => widget.controller.sendToPeer(widget.peer, directory: false),
                tooltip: '发送文件',
                icon: const Icon(LucideIcons.paperclip, size: 20),
              ),
              IconButton(
                onPressed: () => widget.controller.sendToPeer(widget.peer, directory: true),
                tooltip: '发送整个文件夹',
                icon: const Icon(LucideIcons.folder, size: 20),
              ),
              const SizedBox(width: 4),
              Expanded(
                child: TextField(
                  controller: _textController,
                  decoration: InputDecoration(
                    hintText: '输入便签文字直接发送...',
                    isDense: true,
                    contentPadding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.circular(22),
                    ),
                  ),
                  textInputAction: TextInputAction.send,
                  onSubmitted: (val) {
                    final t = val.trim();
                    if (t.isNotEmpty) {
                      widget.controller.sendTextMessage(widget.peer.id, t);
                      _textController.clear();
                    }
                  },
                ),
              ),
              const SizedBox(width: 8),
              IconButton.filled(
                onPressed: () {
                  final t = _textController.text.trim();
                  if (t.isNotEmpty) {
                    widget.controller.sendTextMessage(widget.peer.id, t);
                    _textController.clear();
                  }
                },
                icon: const Icon(LucideIcons.send, size: 16),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

class _ChatBubbleTimeline extends StatelessWidget {
  const _ChatBubbleTimeline({required this.controller, required this.peer});

  final AppController controller;
  final PeerSummary peer;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final transfers = controller.transfers
        .where((t) => t.peerName == peer.name)
        .toList(growable: false);

    if (transfers.isEmpty) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(LucideIcons.messageSquareDashed, size: 44, color: colors.onSurfaceVariant),
              const SizedBox(height: 12),
              Text(
                '已与「${peer.name}」建立会话',
                style: const TextStyle(fontSize: 14, fontWeight: FontWeight.w700),
              ),
              const SizedBox(height: 4),
              Text(
                '在下方发送文件、文件夹或输入便签，内容将以气泡沉淀在此处',
                style: TextStyle(fontSize: 12, color: colors.onSurfaceVariant),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      );
    }

    return ListView.separated(
      padding: const EdgeInsets.all(16),
      itemCount: transfers.length,
      separatorBuilder: (_, _) => const SizedBox(height: 12),
      itemBuilder: (context, idx) {
        final t = transfers[idx];
        final isOutgoing = t.direction == TransferDirection.outgoing;

        return Align(
          alignment: isOutgoing ? Alignment.centerRight : Alignment.centerLeft,
          child: Container(
            constraints: const BoxConstraints(maxWidth: 320),
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: isOutgoing
                  ? colors.primary.withValues(alpha: 0.15)
                  : colors.surface,
              borderRadius: BorderRadius.circular(16),
              border: Border.all(
                color: isOutgoing
                    ? colors.primary.withValues(alpha: 0.4)
                    : colors.outline.withValues(alpha: 0.3),
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      t.state == TransferState.completed
                          ? LucideIcons.checkCircle
                          : (t.isActive ? LucideIcons.refreshCw : LucideIcons.file),
                      size: 16,
                      color: colors.primary,
                    ),
                    const SizedBox(width: 8),
                    Flexible(
                      child: Text(
                        '${t.itemCount} 个文件 · ${formatBytes(t.totalBytes)}',
                        style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w700),
                      ),
                    ),
                  ],
                ),
                if (t.isActive) ...[
                  const SizedBox(height: 8),
                  LiquidProgressBar(progress: t.progress, height: 4),
                  const SizedBox(height: 4),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Text(
                        '${formatBytes(t.bytesPerSecond)}/s',
                        style: TextStyle(fontSize: 10, color: colors.onSurfaceVariant),
                      ),
                      Text(
                        '${(t.progress * 100).toStringAsFixed(0)}%',
                        style: TextStyle(fontSize: 10, fontWeight: FontWeight.w700, color: colors.primary),
                      ),
                    ],
                  ),
                ],
              ],
            ),
          ),
        );
      },
    );
  }
}

// 辅助共用方法
Future<void> _showHotspotGuide(BuildContext context, AppController controller) async {
  final colors = Theme.of(context).colorScheme;
  await showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 24),
      titlePadding: const EdgeInsets.fromLTRB(20, 20, 20, 12),
      contentPadding: const EdgeInsets.symmetric(horizontal: 20),
      actionsPadding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      title: Row(
        children: [
          Container(
            padding: const EdgeInsets.all(6),
            decoration: BoxDecoration(
              color: colors.primary.withValues(alpha: 0.12),
              borderRadius: BorderRadius.circular(8),
            ),
            child: Icon(LucideIcons.wifi, size: 20, color: colors.primary),
          ),
          const SizedBox(width: 10),
          const Text(
            '离线热点 / AP 直连向导',
            style: TextStyle(fontSize: 17, fontWeight: FontWeight.w800),
          ),
        ],
      ),
      content: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              '在户外、高铁、会议室等没有公共 Wi-Fi 的环境下，可使用设备自带的热点功能建立高速离线互传通道：',
              style: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    color: colors.onSurfaceVariant,
                    height: 1.45,
                  ),
            ),
            const SizedBox(height: 14),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: colors.surfaceContainerHighest.withValues(alpha: 0.5),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: colors.outline.withValues(alpha: 0.2)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(LucideIcons.smartphone, size: 16, color: colors.primary),
                      const SizedBox(width: 6),
                      Text(
                        '方式一：手机开启个人热点（推荐）',
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w700,
                          color: colors.primary,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  const Text(
                    '1. 手机打开「个人热点」（可关闭移动蜂窝数据，不消耗流量）；\n2. 电脑 Wi-Fi 搜索并连接该热点；\n3. 双方打开《传输助手》，即可秒级自动发现并满速互传。',
                    style: TextStyle(fontSize: 12, height: 1.45),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 10),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: colors.surfaceContainerHighest.withValues(alpha: 0.5),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: colors.outline.withValues(alpha: 0.2)),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(LucideIcons.monitor, size: 16, color: colors.secondary),
                      const SizedBox(width: 6),
                      Text(
                        '方式二：电脑开启移动热点',
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w700,
                          color: colors.secondary,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 6),
                  const Text(
                    '1. Windows 点击任务栏右下角网络图标，开启「移动热点」；\n2. 手机 Wi-Fi 连接电脑热点；\n3. 打开《传输助手》即刻互联。',
                    style: TextStyle(fontSize: 12, height: 1.45),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 10),
            Row(
              children: [
                Icon(LucideIcons.info, size: 14, color: colors.onSurfaceVariant),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(
                    '若热点环境隔离了广播，可点击下方「手动输入 IP」进行直连。',
                    style: TextStyle(
                      fontSize: 11,
                      color: colors.onSurfaceVariant,
                    ),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        Row(
          children: [
            Expanded(
              child: OutlinedButton(
                onPressed: () {
                  Navigator.pop(context);
                  _showManualConnect(context, controller);
                },
                style: OutlinedButton.styleFrom(
                  minimumSize: const Size(0, 40),
                  padding: EdgeInsets.zero,
                  shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
                ),
                child: const Text('手动输入 IP'),
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: FilledButton(
                onPressed: () => Navigator.pop(context),
                style: FilledButton.styleFrom(
                  minimumSize: const Size(0, 40),
                  padding: EdgeInsets.zero,
                  backgroundColor: Theme.of(context).brightness == Brightness.dark
                      ? AppTheme.brand400
                      : AppTheme.brand600,
                  shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(10)),
                ),
                child: const Text('我知道了'),
              ),
            ),
          ],
        ),
      ],
    ),
  );
}

Future<void> _showManualConnect(BuildContext context, AppController controller) async {
  final c = TextEditingController();
  final addr = await showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('手动直连目标 IP'),
      content: TextField(
        controller: c,
        autofocus: true,
        decoration: const InputDecoration(hintText: '例如 192.168.1.100:53317'),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.pop(context), child: const Text('取消')),
        FilledButton(onPressed: () => Navigator.pop(context, c.text.trim()), child: const Text('连接')),
      ],
    ),
  );
  if (addr != null && addr.isNotEmpty) {
    controller.connectAddress(addr);
  }
}

class _ShareDraftBanner extends StatelessWidget {
  const _ShareDraftBanner({required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isText = controller.sharedTextPayload != null;
    final summary = isText
        ? '已加载文本便签（${controller.sharedTextPayload!.length} 字）'
        : '已加载 ${controller.sharedFileSources!.length} 个文件草稿';

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: colors.primary.withValues(alpha: 0.12),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: colors.primary.withValues(alpha: 0.3)),
      ),
      child: Row(
        children: [
          Icon(isText ? LucideIcons.fileText : LucideIcons.files, size: 16, color: colors.primary),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              summary,
              style: const TextStyle(fontSize: 12, fontWeight: FontWeight.w600),
            ),
          ),
          IconButton(
            onPressed: controller.clearSharedDraft,
            icon: const Icon(LucideIcons.x, size: 16),
            visualDensity: VisualDensity.compact,
          ),
        ],
      ),
    );
  }
}
