import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';

class SettingsView extends StatefulWidget {
  const SettingsView({super.key, required this.controller});

  final AppController controller;

  @override
  State<SettingsView> createState() => _SettingsViewState();
}

class _SettingsViewState extends State<SettingsView> {
  late final TextEditingController nameController;

  @override
  void initState() {
    super.initState();
    nameController = TextEditingController(
      text: widget.controller.settings.deviceName,
    );
  }

  @override
  void dispose() {
    nameController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final settings = widget.controller.settings;
    if (nameController.text != settings.deviceName &&
        !nameController.selection.isValid) {
      nameController.text = settings.deviceName;
    }
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 880),
        child: ListView(
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 0),
          children: [
            const PageHeader(title: '设置'),
            Text('本机', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 12),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(labelText: '设备名称'),
              textInputAction: TextInputAction.done,
              onSubmitted: (value) {
                final trimmed = value.trim();
                if (trimmed.isNotEmpty) {
                  widget.controller.updateSettings(
                    settings.copyWith(deviceName: trimmed),
                  );
                }
              },
            ),
            const SizedBox(height: 20),
            _SettingsRow(
              icon: LucideIcons.folderOpen,
              title: '接收位置',
              value: settings.receiveDirectory,
              onTap: widget.controller.chooseReceiveDirectory,
            ),
            const Divider(),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              secondary: const Icon(LucideIcons.radio),
              title: const Text('后台接收'),
              value: settings.backgroundReceive,
              onChanged: (value) => widget.controller.updateSettings(
                settings.copyWith(backgroundReceive: value),
              ),
            ),
            const Divider(),
            SwitchListTile(
              contentPadding: EdgeInsets.zero,
              secondary: const Icon(LucideIcons.shieldCheck),
              title: const Text('自动接收可信设备'),
              value: settings.autoAcceptTrusted,
              onChanged: (value) => widget.controller.updateSettings(
                settings.copyWith(autoAcceptTrusted: value),
              ),
            ),
            const SizedBox(height: 28),
            Text('外观', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 12),
            SegmentedButton<ThemeMode>(
              segments: const [
                ButtonSegment(value: ThemeMode.system, label: Text('跟随系统')),
                ButtonSegment(
                  value: ThemeMode.light,
                  icon: Icon(LucideIcons.sun),
                  label: Text('浅色'),
                ),
                ButtonSegment(
                  value: ThemeMode.dark,
                  icon: Icon(LucideIcons.moon),
                  label: Text('深色'),
                ),
              ],
              selected: {settings.themeMode},
              onSelectionChanged: (value) => widget.controller.updateSettings(
                settings.copyWith(themeMode: value.first),
              ),
            ),
            const SizedBox(height: 28),
            Text('可信设备', style: Theme.of(context).textTheme.titleMedium),
            const SizedBox(height: 8),
            ...widget.controller.peers
                .where((peer) => peer.trusted)
                .map(
                  (peer) => ListTile(
                    contentPadding: EdgeInsets.zero,
                    leading: Icon(
                      peer.deviceKind == DeviceKind.phone
                          ? LucideIcons.smartphone
                          : LucideIcons.monitor,
                    ),
                    title: Text(peer.name),
                    subtitle: Text(peer.address),
                    trailing: IconButton(
                      onPressed: () =>
                          widget.controller.removeTrustedPeer(peer.id),
                      tooltip: '移除信任',
                      icon: const Icon(LucideIcons.trash2),
                    ),
                  ),
                ),
            if (!widget.controller.peers.any((peer) => peer.trusted))
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 20),
                child: Text(
                  '暂无可信设备',
                  style: TextStyle(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            const SizedBox(height: 32),
          ],
        ),
      ),
    );
  }
}

class _SettingsRow extends StatelessWidget {
  const _SettingsRow({
    required this.icon,
    required this.title,
    required this.value,
    required this.onTap,
  });

  final IconData icon;
  final String title;
  final String value;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    return ListTile(
      contentPadding: EdgeInsets.zero,
      leading: Icon(icon),
      title: Text(title),
      subtitle: Text(value, maxLines: 2, overflow: TextOverflow.ellipsis),
      trailing: const Icon(LucideIcons.chevronRight),
      onTap: onTap,
    );
  }
}
