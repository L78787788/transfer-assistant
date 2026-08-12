import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';

class ErrorBanner extends StatelessWidget {
  const ErrorBanner({
    super.key,
    required this.message,
    required this.onDismiss,
  });

  final String message;
  final VoidCallback onDismiss;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: colors.errorContainer,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 10, 8, 10),
        child: Row(
          children: [
            Icon(
              LucideIcons.circleAlert,
              size: 18,
              color: colors.onErrorContainer,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Text(
                message,
                style: TextStyle(color: colors.onErrorContainer),
              ),
            ),
            IconButton(
              onPressed: onDismiss,
              tooltip: '关闭',
              icon: const Icon(LucideIcons.x),
            ),
          ],
        ),
      ),
    );
  }
}

class PageHeader extends StatelessWidget {
  const PageHeader({super.key, required this.title, this.actions = const []});

  final String title;
  final List<Widget> actions;

  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: 64,
      child: Row(
        children: [
          Expanded(
            child: Text(
              title,
              style: Theme.of(context).textTheme.headlineSmall,
            ),
          ),
          ...actions,
        ],
      ),
    );
  }
}

class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    required this.icon,
    required this.title,
    required this.action,
  });

  final IconData icon;
  final String title;
  final Widget action;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Center(
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 64, horizontal: 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 44, color: colors.onSurfaceVariant),
            const SizedBox(height: 16),
            Text(title, style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 20),
            action,
          ],
        ),
      ),
    );
  }
}

class StatusLabel extends StatelessWidget {
  const StatusLabel({super.key, required this.state});

  final TransferState state;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final (label, icon, color) = switch (state) {
      TransferState.preparing => (
        '准备中',
        LucideIcons.clock3,
        colors.onSurfaceVariant,
      ),
      TransferState.connecting => ('连接中', LucideIcons.network, colors.primary),
      TransferState.pairing => (
        '配对中',
        LucideIcons.shieldCheck,
        colors.secondary,
      ),
      TransferState.waitingForAcceptance => (
        '等待接收',
        LucideIcons.clock3,
        colors.secondary,
      ),
      TransferState.transferring => ('传输中', LucideIcons.radio, colors.primary),
      TransferState.paused => ('已暂停', LucideIcons.pause, colors.secondary),
      TransferState.interrupted => (
        '已中断',
        LucideIcons.circleAlert,
        colors.error,
      ),
      TransferState.verifying => (
        '校验中',
        LucideIcons.shieldCheck,
        colors.primary,
      ),
      TransferState.completed => (
        '已完成',
        LucideIcons.circleCheck,
        colors.primary,
      ),
      TransferState.failed => ('失败', LucideIcons.circleX, colors.error),
      TransferState.cancelled => (
        '已取消',
        LucideIcons.x,
        colors.onSurfaceVariant,
      ),
    };
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Icon(icon, size: 15, color: color),
        const SizedBox(width: 6),
        Text(
          label,
          style: Theme.of(
            context,
          ).textTheme.labelMedium?.copyWith(color: color),
        ),
      ],
    );
  }
}

String formatBytes(int bytes) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  var value = bytes.toDouble();
  var unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  final digits = value >= 100 || unit == 0 ? 0 : 1;
  return '${value.toStringAsFixed(digits)} ${units[unit]}';
}
