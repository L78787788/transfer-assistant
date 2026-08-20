import 'dart:io';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

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
    final colors = Theme.of(context).colorScheme;
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
            PageHeader(title: '设置'),
            const SizedBox(height: 8),

            _SectionHeader(title: '外观与主题', icon: LucideIcons.palette),
            const SizedBox(height: 8),
            GlassCard(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Icon(LucideIcons.sunMoon, size: 16, color: colors.primary),
                      const SizedBox(width: 8),
                      Text(
                        '明暗外观模式',
                        style: TextStyle(
                          fontSize: 13,
                          fontWeight: FontWeight.w600,
                          color: colors.onSurface,
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 12),
                  Center(
                    child: SegmentedButton<ThemeMode>(
                      segments: const [
                        ButtonSegment(
                          value: ThemeMode.system,
                          label: Text('跟随系统'),
                          icon: Icon(LucideIcons.monitor, size: 16),
                        ),
                        ButtonSegment(
                          value: ThemeMode.light,
                          icon: Icon(LucideIcons.sun, size: 16),
                          label: Text('浅色模式'),
                        ),
                        ButtonSegment(
                          value: ThemeMode.dark,
                          icon: Icon(LucideIcons.moon, size: 16),
                          label: Text('深色模式'),
                        ),
                      ],
                      selected: {settings.themeMode},
                      onSelectionChanged: (value) =>
                          widget.controller.updateSettings(
                        settings.copyWith(themeMode: value.first),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 20),

            _SectionHeader(title: '本机与存储', icon: LucideIcons.hardDrive),
            const SizedBox(height: 8),
            GlassCard(
              padding: const EdgeInsets.all(16),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  TextField(
                    controller: nameController,
                    decoration: InputDecoration(
                      labelText: '这台设备的显示名称',
                      prefixIcon: const Icon(LucideIcons.smartphone, size: 20),
                      suffixIcon: IconButton(
                        onPressed: () {
                          final trimmed = nameController.text.trim();
                          if (trimmed.isNotEmpty) {
                            widget.controller.updateSettings(
                              settings.copyWith(deviceName: trimmed),
                            );
                            ScaffoldMessenger.of(context).showSnackBar(
                              const SnackBar(
                                content: Text('设备名称已保存'),
                                duration: Duration(seconds: 1),
                              ),
                            );
                          }
                        },
                        tooltip: '保存名称',
                        icon: const Icon(LucideIcons.check, size: 18),
                      ),
                    ),
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
                  const SizedBox(height: 14),
                  const Divider(),
                  const SizedBox(height: 6),
                  ListTile(
                    contentPadding: EdgeInsets.zero,
                    leading: Container(
                      padding: const EdgeInsets.all(8),
                      decoration: BoxDecoration(
                        color: colors.primary.withValues(alpha: 0.1),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Icon(LucideIcons.folderOpen,
                          color: colors.primary, size: 20),
                    ),
                    title: const Text('接收文件保存目录'),
                    subtitle: Text(
                      settings.receiveDirectory,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                      style: TextStyle(
                          color: colors.onSurfaceVariant, fontSize: 12),
                    ),
                    trailing: FilledButton.tonal(
                      onPressed: widget.controller.chooseReceiveDirectory,
                      child: const Text('更改目录'),
                    ),
                  ),
                  if (_isAppPrivateDirectory(settings.receiveDirectory))
                    Container(
                      margin: const EdgeInsets.only(top: 8),
                      padding: const EdgeInsets.all(10),
                      decoration: BoxDecoration(
                        color: colors.errorContainer.withValues(alpha: 0.4),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Row(
                        children: [
                          Icon(LucideIcons.info,
                              size: 16, color: colors.error),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              '当前为应用内部私有目录，卸载或清数据可能丢失接收的文件。建议点击上方选择系统公共「下载/Download」目录。',
                              style: TextStyle(
                                fontSize: 11,
                                color: colors.onErrorContainer,
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(height: 20),

            _SectionHeader(title: '传输与安全', icon: LucideIcons.shieldCheck),
            const SizedBox(height: 8),
            GlassCard(
              padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
              child: Column(
                children: [
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    secondary: Container(
                      padding: const EdgeInsets.all(8),
                      decoration: BoxDecoration(
                        color: colors.primary.withValues(alpha: 0.1),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Icon(LucideIcons.radio,
                          color: colors.primary, size: 20),
                    ),
                    title: const Text('后台息屏保活传输'),
                    subtitle: const Text('在后台或息屏时保持 Wi-Fi 高性能传输与保活连接'),
                    value: settings.backgroundReceive,
                    onChanged: (value) => widget.controller.updateSettings(
                      settings.copyWith(backgroundReceive: value),
                    ),
                  ),
                  const Divider(),
                  SwitchListTile(
                    contentPadding: EdgeInsets.zero,
                    secondary: Container(
                      padding: const EdgeInsets.all(8),
                      decoration: BoxDecoration(
                        color: colors.secondary.withValues(alpha: 0.1),
                        borderRadius: BorderRadius.circular(8),
                      ),
                      child: Icon(LucideIcons.shieldAlert,
                          color: colors.secondary, size: 20),
                    ),
                    title: const Text('自动接收可信设备传输'),
                    subtitle: const Text('来自已信任设备的文件无需弹窗确认直接接收'),
                    value: settings.autoAcceptTrusted,
                    onChanged: (value) => widget.controller.updateSettings(
                      settings.copyWith(autoAcceptTrusted: value),
                    ),
                  ),
                  if (Platform.isWindows) ...[
                    const Divider(),
                    FutureBuilder<bool>(
                      future: widget.controller.platform.isContextMenuEnabled(),
                      builder: (context, snapshot) {
                        final isEnabled = snapshot.data ?? false;
                        return SwitchListTile(
                          contentPadding: EdgeInsets.zero,
                          secondary: Container(
                            padding: const EdgeInsets.all(8),
                            decoration: BoxDecoration(
                              color: colors.primary.withValues(alpha: 0.1),
                              borderRadius: BorderRadius.circular(8),
                            ),
                            child: Icon(
                              LucideIcons.mousePointerClick,
                              color: colors.primary,
                              size: 20,
                            ),
                          ),
                          title: const Text('Windows 资源管理器右键菜单'),
                          subtitle: const Text('在文件或文件夹右键菜单中增加「使用互传发送」'),
                          value: isEnabled,
                          onChanged: (value) async {
                            await widget.controller.platform
                                .setContextMenuEnabled(value);
                            setState(() {});
                            widget.controller.showNotice(
                              value ? '已添加到系统右键菜单' : '已从系统右键菜单移除',
                            );
                          },
                        );
                      },
                    ),
                  ],
                  if (Platform.isAndroid && !widget.controller.isNotificationPermissionGranted) ...[
                    const Divider(),
                    Container(
                      padding: const EdgeInsets.all(12),
                      decoration: BoxDecoration(
                        color: colors.errorContainer.withValues(alpha: 0.5),
                        borderRadius: BorderRadius.circular(12),
                        border: Border.all(color: colors.error.withValues(alpha: 0.3)),
                      ),
                      child: Row(
                        children: [
                          Icon(LucideIcons.bellOff, color: colors.error, size: 20),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                const Text(
                                  '通知权限未开启',
                                  style: TextStyle(fontSize: 13, fontWeight: FontWeight.w700),
                                ),
                                Text(
                                  '开启通知后可在后台实时查看传输进度与接收弹窗提醒',
                                  style: TextStyle(fontSize: 11, color: colors.onErrorContainer),
                                ),
                              ],
                            ),
                          ),
                          FilledButton.tonal(
                            onPressed: () => widget.controller.requestNotificationPermission(),
                            style: FilledButton.styleFrom(
                              padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                              textStyle: const TextStyle(fontSize: 12),
                            ),
                            child: const Text('去开启'),
                          ),
                        ],
                      ),
                    ),
                  ],
                ],
              ),
            ),
            const SizedBox(height: 20),

            Row(
              mainAxisAlignment: MainAxisAlignment.spaceBetween,
              children: [
                _SectionHeader(title: '已信任设备 (${widget.controller.trustedPeers.length})', icon: LucideIcons.shieldCheck),
                if (widget.controller.trustedPeers.isNotEmpty)
                  TextButton.icon(
                    onPressed: () async {
                      final confirm = await showDialog<bool>(
                        context: context,
                        builder: (ctx) => AlertDialog(
                          title: const Text('清空可信设备'),
                          content: const Text('确定要清空所有已信任的设备绑定吗？清空后来自这些设备的传输将重新弹窗确认。'),
                          actions: [
                            TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')),
                            FilledButton(
                              style: FilledButton.styleFrom(backgroundColor: colors.error),
                              onPressed: () => Navigator.pop(ctx, true),
                              child: const Text('确认清空'),
                            ),
                          ],
                        ),
                      );
                      if (confirm == true) {
                        await widget.controller.clearTrustedPeers();
                      }
                    },
                    icon: const Icon(LucideIcons.trash2, size: 14, color: Colors.redAccent),
                    label: const Text('清空全部', style: TextStyle(fontSize: 12, color: Colors.redAccent)),
                  ),
              ],
            ),
            const SizedBox(height: 8),
            GlassCard(
              padding: const EdgeInsets.all(16),
              child: Column(
                children: [
                  ...widget.controller.trustedPeers.map(
                    (peer) {
                      final fpShort = peer.fingerprintHex.length > 16
                          ? '${peer.fingerprintHex.substring(0, 8)}...${peer.fingerprintHex.substring(peer.fingerprintHex.length - 8)}'
                          : peer.fingerprintHex;
                      return ListTile(
                        contentPadding: EdgeInsets.zero,
                        leading: Container(
                          padding: const EdgeInsets.all(8),
                          decoration: BoxDecoration(
                            color: colors.primary.withValues(alpha: 0.1),
                            borderRadius: BorderRadius.circular(8),
                          ),
                          child: Icon(LucideIcons.shieldCheck, color: colors.primary, size: 20),
                        ),
                        title: Text(peer.displayName, style: const TextStyle(fontWeight: FontWeight.w600, fontSize: 14)),
                        subtitle: Text(
                          '指纹: $fpShort\n首次添加: ${_formatDate(peer.createdAt)}',
                          style: TextStyle(fontSize: 11, color: colors.onSurfaceVariant),
                        ),
                        isThreeLine: true,
                        trailing: IconButton(
                          onPressed: () => widget.controller.removeTrustedPeer(peer.peerId),
                          tooltip: '取消信任',
                          icon: const Icon(LucideIcons.userMinus, size: 18, color: Colors.redAccent),
                        ),
                      );
                    },
                  ),
                  if (widget.controller.trustedPeers.isEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      child: Center(
                        child: Text(
                          '暂无可信设备（在接收文件弹窗中勾选“记住此设备”即可自动信任）',
                          style: TextStyle(
                            color: colors.onSurfaceVariant,
                            fontSize: 12,
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(height: 32),
          ],
        ),
      ),
    );
  }

  String _formatDate(DateTime dt) {
    return '${dt.year}-${dt.month.toString().padLeft(2, '0')}-${dt.day.toString().padLeft(2, '0')} ${dt.hour.toString().padLeft(2, '0')}:${dt.minute.toString().padLeft(2, '0')}';
  }

  bool _isAppPrivateDirectory(String path) {
    return path.contains('/data/user/') ||
        path.contains('/data/data/') ||
        path.contains('app_flutter');
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader({required this.title, required this.icon});

  final String title;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Row(
      children: [
        Icon(icon, size: 16, color: colors.primary),
        const SizedBox(width: 6),
        Text(
          title,
          style: Theme.of(context).textTheme.titleSmall?.copyWith(
                fontWeight: FontWeight.w700,
                color: colors.primary,
                letterSpacing: -0.2,
              ),
        ),
      ],
    );
  }
}
