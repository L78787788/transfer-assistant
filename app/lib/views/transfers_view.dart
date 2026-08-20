import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';
import '../theme/app_theme.dart';

class TransfersView extends StatefulWidget {
  const TransfersView({super.key, required this.controller});

  final AppController controller;

  @override
  State<TransfersView> createState() => _TransfersViewState();
}

class _TransfersViewState extends State<TransfersView> {
  @override
  Widget build(BuildContext context) {
    final transfers = widget.controller.transfers
        .where((transfer) => transfer.isActive)
        .toList(growable: false);

    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1080),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 8),
          child: Column(
            children: [
              PageHeader(
                title: '传输中',
                actions: [
                  FilledButton.tonalIcon(
                    onPressed: () => widget.controller.selectPage(2),
                    icon: const Icon(LucideIcons.history, size: 16),
                    label: const Text('传输历史'),
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
              const SizedBox(height: 8),
              Expanded(
                child: transfers.isEmpty
                    ? EmptyState(
                        title: '当前没有正在进行的传输',
                        description: '您可以前往「附近设备」选择在线联系人发送文件，或等待对端设备发起文件传输。',
                        action: Wrap(
                          spacing: 10,
                          runSpacing: 10,
                          alignment: WrapAlignment.center,
                          children: [
                            FilledButton.icon(
                              onPressed: () => widget.controller.selectPage(0),
                              icon: const Icon(LucideIcons.radio, size: 16),
                              label: const Text('发现附近设备'),
                              style: FilledButton.styleFrom(
                                backgroundColor:
                                    Theme.of(context).brightness ==
                                        Brightness.dark
                                    ? AppTheme.brand400
                                    : AppTheme.brand600,
                                foregroundColor: Colors.white,
                                minimumSize: const Size(140, 40),
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 16,
                                ),
                                shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(10),
                                ),
                              ),
                            ),
                            OutlinedButton.icon(
                              onPressed: () => widget.controller.selectPage(2),
                              icon: const Icon(LucideIcons.history, size: 16),
                              label: const Text('查看历史归档'),
                              style: OutlinedButton.styleFrom(
                                minimumSize: const Size(140, 40),
                                padding: const EdgeInsets.symmetric(
                                  horizontal: 16,
                                ),
                                shape: RoundedRectangleBorder(
                                  borderRadius: BorderRadius.circular(10),
                                ),
                              ),
                            ),
                          ],
                        ),
                      )
                    : ListView.separated(
                        padding: const EdgeInsets.only(bottom: 24, top: 4),
                        itemCount: transfers.length,
                        separatorBuilder: (_, _) => const SizedBox(height: 14),
                        itemBuilder: (_, index) => _TransferCard(
                          transfer: transfers[index],
                          onCommand: (command) => widget.controller
                              .commandTransfer(transfers[index], command),
                        ),
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _TransferCard extends StatefulWidget {
  const _TransferCard({required this.transfer, required this.onCommand});

  final TransferSnapshot transfer;
  final ValueChanged<TransferCommand> onCommand;

  @override
  State<_TransferCard> createState() => _TransferCardState();
}

class _TransferCardState extends State<_TransferCard> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final isIncoming = widget.transfer.direction == TransferDirection.incoming;
    final pct = (widget.transfer.progress * 100).toStringAsFixed(1);

    return GlassCard(
      padding: const EdgeInsets.all(18),
      borderRadius: 16,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              // 渐变 Tile 图标（下载极光青 / 上传品牌蓝）
              Container(
                width: 44,
                height: 44,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(12),
                  gradient: LinearGradient(
                    colors: isIncoming
                        ? [const Color(0xff06b6d4), const Color(0xff0284c7)]
                        : [AppTheme.brand400, AppTheme.brand600],
                    begin: Alignment.topLeft,
                    end: Alignment.bottomRight,
                  ),
                  boxShadow: [
                    BoxShadow(
                      color:
                          (isIncoming
                                  ? const Color(0xff06b6d4)
                                  : AppTheme.brand500)
                              .withValues(alpha: 0.3),
                      blurRadius: 10,
                      offset: const Offset(0, 3),
                    ),
                  ],
                ),
                child: Icon(
                  isIncoming ? LucideIcons.download : LucideIcons.upload,
                  color: Colors.white,
                  size: 22,
                ),
              ),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      isIncoming
                          ? '接收自「${widget.transfer.peerName}」'
                          : '发送至「${widget.transfer.peerName}」',
                      style: Theme.of(context).textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                        letterSpacing: -0.2,
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 3),
                    Text(
                      '${widget.transfer.itemCount} 个文件 · 总大小 ${formatBytes(widget.transfer.totalBytes)} · 4 路并行',
                      style: TextStyle(
                        color: colors.onSurfaceVariant,
                        fontSize: 12,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              StatusLabel(state: widget.transfer.state),
            ],
          ),
          const SizedBox(height: 18),

          // 8px 品牌渐变 + 白色流光 sweep 进度条
          LiquidProgressBar(progress: widget.transfer.progress, height: 8),
          const SizedBox(height: 12),

          // 进度数值与速度信息
          Row(
            children: [
              Text(
                '${formatBytes(widget.transfer.completedBytes)} / ${formatBytes(widget.transfer.totalBytes)}',
                style: const TextStyle(
                  fontFamily: 'monospace',
                  fontSize: 12,
                  fontWeight: FontWeight.w700,
                ),
              ),
              const SizedBox(width: 6),
              Text(
                '($pct%)',
                style: TextStyle(
                  fontSize: 12,
                  fontWeight: FontWeight.w600,
                  color: colors.onSurfaceVariant,
                ),
              ),
              const Spacer(),
              if (widget.transfer.bytesPerSecond > 0 &&
                  widget.transfer.state == TransferState.transferring) ...[
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 8,
                    vertical: 3,
                  ),
                  decoration: BoxDecoration(
                    color: colors.primary.withValues(
                      alpha: isDark ? 0.2 : 0.12,
                    ),
                    borderRadius: BorderRadius.circular(6),
                  ),
                  child: Text(
                    '${formatBytes(widget.transfer.bytesPerSecond)}/s',
                    style: TextStyle(
                      fontFamily: 'monospace',
                      fontSize: 12,
                      fontWeight: FontWeight.w700,
                      color: colors.primary,
                    ),
                  ),
                ),
                if (widget.transfer.etaText case final eta?) ...[
                  const SizedBox(width: 8),
                  Text(
                    '剩余 $eta',
                    style: TextStyle(
                      fontSize: 11,
                      fontWeight: FontWeight.w500,
                      color: colors.onSurfaceVariant,
                    ),
                  ),
                ],
              ],
            ],
          ),

          if (widget.transfer.error != null) ...[
            const SizedBox(height: 10),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
              decoration: BoxDecoration(
                color: colors.error.withValues(alpha: 0.12),
                borderRadius: BorderRadius.circular(8),
              ),
              child: Row(
                children: [
                  Icon(LucideIcons.circleAlert, size: 14, color: colors.error),
                  const SizedBox(width: 6),
                  Expanded(
                    child: Text(
                      '错误：${widget.transfer.error}',
                      style: TextStyle(
                        color: colors.error,
                        fontSize: 11,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ],

          const SizedBox(height: 14),
          const Divider(height: 1),
          const SizedBox(height: 12),

          // 操作按钮工具栏
          Row(
            children: [
              if (widget.transfer.itemCount > 1)
                InkWell(
                  borderRadius: BorderRadius.circular(6),
                  onTap: () => setState(() => _expanded = !_expanded),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 6,
                      vertical: 4,
                    ),
                    child: Row(
                      children: [
                        Icon(
                          _expanded
                              ? LucideIcons.chevronUp
                              : LucideIcons.chevronDown,
                          size: 14,
                          color: colors.onSurfaceVariant,
                        ),
                        const SizedBox(width: 4),
                        Text(
                          _expanded ? '收起文件清单' : '查看文件清单',
                          style: TextStyle(
                            fontSize: 12,
                            color: colors.onSurfaceVariant,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              const Spacer(),
              if (widget.transfer.state == TransferState.transferring)
                OutlinedButton.icon(
                  onPressed: () => widget.onCommand(TransferCommand.pause),
                  icon: const Icon(LucideIcons.pause, size: 14),
                  label: const Text('暂停'),
                  style: OutlinedButton.styleFrom(
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 8,
                    ),
                    textStyle: const TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(8),
                    ),
                  ),
                ),
              if (widget.transfer.state == TransferState.paused)
                FilledButton.icon(
                  onPressed: () => widget.onCommand(TransferCommand.resume),
                  icon: const Icon(LucideIcons.play, size: 14),
                  label: const Text('继续'),
                  style: FilledButton.styleFrom(
                    backgroundColor: isDark
                        ? AppTheme.brand400
                        : AppTheme.brand600,
                    foregroundColor: Colors.white,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 8,
                    ),
                    textStyle: const TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(8),
                    ),
                  ),
                ),
              if (widget.transfer.state == TransferState.interrupted)
                FilledButton.icon(
                  onPressed: () => widget.onCommand(TransferCommand.retry),
                  icon: const Icon(LucideIcons.rotateCw, size: 14),
                  label: const Text('重试'),
                  style: FilledButton.styleFrom(
                    backgroundColor: isDark
                        ? AppTheme.brand400
                        : AppTheme.brand600,
                    foregroundColor: Colors.white,
                    padding: const EdgeInsets.symmetric(
                      horizontal: 14,
                      vertical: 8,
                    ),
                    textStyle: const TextStyle(
                      fontSize: 12,
                      fontWeight: FontWeight.w600,
                    ),
                    shape: RoundedRectangleBorder(
                      borderRadius: BorderRadius.circular(8),
                    ),
                  ),
                ),
              const SizedBox(width: 8),
              OutlinedButton.icon(
                onPressed: () => widget.onCommand(TransferCommand.cancel),
                icon: const Icon(LucideIcons.x, size: 14),
                label: const Text('取消'),
                style: OutlinedButton.styleFrom(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 14,
                    vertical: 8,
                  ),
                  textStyle: const TextStyle(
                    fontSize: 12,
                    fontWeight: FontWeight.w600,
                  ),
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(8),
                  ),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
