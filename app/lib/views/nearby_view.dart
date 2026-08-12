import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';

class NearbyView extends StatelessWidget {
  const NearbyView({super.key, required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1080),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20),
          child: Column(
            children: [
              PageHeader(
                title: '附近设备',
                actions: [
                  IconButton(
                    onPressed: () => _showManualConnect(context),
                    tooltip: '通过地址连接',
                    icon: const Icon(LucideIcons.plus),
                  ),
                  IconButton(
                    onPressed: controller.isRefreshing
                        ? null
                        : controller.refreshPeers,
                    tooltip: '刷新',
                    icon: const Icon(LucideIcons.refreshCw),
                  ),
                ],
              ),
              Expanded(child: _content(context)),
            ],
          ),
        ),
      ),
    );
  }

  Widget _content(BuildContext context) {
    if (controller.isInitializing) {
      return const _PeerSkeleton();
    }
    if (controller.peers.isEmpty) {
      return EmptyState(
        icon: LucideIcons.wifi,
        title: '未发现附近设备',
        action: OutlinedButton.icon(
          onPressed: controller.refreshPeers,
          icon: const Icon(LucideIcons.refreshCw, size: 18),
          label: const Text('重新扫描'),
        ),
      );
    }
    return ListView.separated(
      padding: const EdgeInsets.only(bottom: 24),
      itemCount: controller.peers.length,
      separatorBuilder: (_, _) => const Divider(),
      itemBuilder: (context, index) => _PeerRow(
        peer: controller.peers[index],
        onSendFile: () =>
            controller.sendToPeer(controller.peers[index], directory: false),
        onSendDirectory: () =>
            controller.sendToPeer(controller.peers[index], directory: true),
      ),
    );
  }

  Future<void> _showManualConnect(BuildContext context) async {
    final address = await showDialog<String>(
      context: context,
      builder: (context) => const _ManualConnectDialog(),
    );
    if (address != null && address.isNotEmpty) {
      await controller.connectAddress(address);
    }
  }
}

class _PeerRow extends StatelessWidget {
  const _PeerRow({
    required this.peer,
    required this.onSendFile,
    required this.onSendDirectory,
  });

  final PeerSummary peer;
  final VoidCallback onSendFile;
  final VoidCallback onSendDirectory;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return ListTile(
      minTileHeight: 76,
      contentPadding: const EdgeInsets.symmetric(horizontal: 8),
      leading: SizedBox.square(
        dimension: 44,
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: colors.primaryContainer,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Icon(
            peer.deviceKind == DeviceKind.phone
                ? LucideIcons.smartphone
                : LucideIcons.monitor,
            color: colors.onPrimaryContainer,
          ),
        ),
      ),
      title: Text(peer.name, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: Text(
        '${peer.address}${peer.trusted ? ' · 已信任' : ''}',
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
      ),
      trailing: PopupMenuButton<String>(
        tooltip: '发送',
        icon: const Icon(LucideIcons.send),
        onSelected: (value) =>
            value == 'directory' ? onSendDirectory() : onSendFile(),
        itemBuilder: (context) => const [
          PopupMenuItem(
            value: 'file',
            child: ListTile(
              leading: Icon(LucideIcons.file),
              title: Text('发送文件'),
            ),
          ),
          PopupMenuItem(
            value: 'directory',
            child: ListTile(
              leading: Icon(LucideIcons.folder),
              title: Text('发送文件夹'),
            ),
          ),
        ],
      ),
    );
  }
}

class _ManualConnectDialog extends StatefulWidget {
  const _ManualConnectDialog();

  @override
  State<_ManualConnectDialog> createState() => _ManualConnectDialogState();
}

class _ManualConnectDialogState extends State<_ManualConnectDialog> {
  final controller = TextEditingController();

  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('通过地址连接'),
      content: TextField(
        controller: controller,
        autofocus: true,
        keyboardType: TextInputType.url,
        decoration: const InputDecoration(
          labelText: 'IP 地址与端口',
          hintText: '192.168.1.8:53317',
        ),
        onSubmitted: (value) => Navigator.pop(context, value.trim()),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('取消'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, controller.text.trim()),
          child: const Text('连接'),
        ),
      ],
    );
  }
}

class _PeerSkeleton extends StatelessWidget {
  const _PeerSkeleton();

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.surfaceContainerHighest;
    return ListView.separated(
      itemCount: 4,
      separatorBuilder: (_, _) => const SizedBox(height: 12),
      itemBuilder: (_, _) => Container(height: 68, color: color),
    );
  }
}
