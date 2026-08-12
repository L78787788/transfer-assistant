import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';

class TransfersView extends StatefulWidget {
  const TransfersView({super.key, required this.controller});

  final AppController controller;

  @override
  State<TransfersView> createState() => _TransfersViewState();
}

class _TransfersViewState extends State<TransfersView> {
  var activeOnly = true;

  @override
  Widget build(BuildContext context) {
    final transfers = widget.controller.transfers
        .where((transfer) => transfer.isActive == activeOnly)
        .toList(growable: false);
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1080),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20),
          child: Column(
            children: [
              PageHeader(
                title: '传输任务',
                actions: [
                  SegmentedButton<bool>(
                    segments: const [
                      ButtonSegment(value: true, label: Text('进行中')),
                      ButtonSegment(value: false, label: Text('历史')),
                    ],
                    selected: {activeOnly},
                    showSelectedIcon: false,
                    onSelectionChanged: (value) =>
                        setState(() => activeOnly = value.first),
                  ),
                ],
              ),
              Expanded(
                child: transfers.isEmpty
                    ? EmptyState(
                        icon: activeOnly
                            ? LucideIcons.radio
                            : LucideIcons.history,
                        title: activeOnly ? '没有进行中的任务' : '暂无传输历史',
                        action: TextButton(
                          onPressed: () => widget.controller.selectPage(0),
                          child: const Text('查看附近设备'),
                        ),
                      )
                    : ListView.separated(
                        padding: const EdgeInsets.only(bottom: 24),
                        itemCount: transfers.length,
                        separatorBuilder: (_, _) => const SizedBox(height: 12),
                        itemBuilder: (_, index) => _TransferRow(
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

class _TransferRow extends StatelessWidget {
  const _TransferRow({required this.transfer, required this.onCommand});

  final TransferSnapshot transfer;
  final ValueChanged<TransferCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Card(
      child: Padding(
        padding: const EdgeInsets.all(16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(
                  transfer.direction == TransferDirection.incoming
                      ? LucideIcons.download
                      : LucideIcons.upload,
                  size: 20,
                  color: colors.primary,
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(
                    transfer.peerName,
                    style: Theme.of(context).textTheme.titleMedium,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                StatusLabel(state: transfer.state),
                const SizedBox(width: 4),
                _TransferActions(transfer: transfer, onCommand: onCommand),
              ],
            ),
            const SizedBox(height: 12),
            LinearProgressIndicator(
              value: transfer.progress,
              minHeight: 6,
              borderRadius: BorderRadius.circular(3),
            ),
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(
                  child: Text(
                    '${transfer.itemCount} 项 · ${formatBytes(transfer.completedBytes)} / ${formatBytes(transfer.totalBytes)}',
                    style: Theme.of(context).textTheme.bodySmall,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
                if (transfer.bytesPerSecond > 0)
                  Text(
                    '${formatBytes(transfer.bytesPerSecond)}/s',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
              ],
            ),
            if (transfer.error case final error?) ...[
              const SizedBox(height: 8),
              Text(error, style: TextStyle(color: colors.error)),
            ],
          ],
        ),
      ),
    );
  }
}

class _TransferActions extends StatelessWidget {
  const _TransferActions({required this.transfer, required this.onCommand});

  final TransferSnapshot transfer;
  final ValueChanged<TransferCommand> onCommand;

  @override
  Widget build(BuildContext context) {
    final action = switch (transfer.state) {
      TransferState.transferring => (
        LucideIcons.pause,
        '暂停',
        TransferCommand.pause,
      ),
      TransferState.paused => (LucideIcons.play, '继续', TransferCommand.resume),
      TransferState.failed || TransferState.interrupted => (
        LucideIcons.rotateCcw,
        '重试',
        TransferCommand.retry,
      ),
      _ => null,
    };
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (action != null)
          IconButton(
            onPressed: () => onCommand(action.$3),
            tooltip: action.$2,
            icon: Icon(action.$1),
          ),
        if (transfer.isActive)
          IconButton(
            onPressed: () => onCommand(TransferCommand.cancel),
            tooltip: '取消',
            icon: const Icon(LucideIcons.x),
          ),
      ],
    );
  }
}
