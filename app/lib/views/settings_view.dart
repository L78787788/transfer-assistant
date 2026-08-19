import 'dart:io';
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
            _BrandAboutCard(),
            const SizedBox(height: 16),

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
                          subtitle: const Text('在文件或文件夹右键菜单中增加「使用传输助手发送」'),
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
                ],
              ),
            ),
            const SizedBox(height: 20),

            _SectionHeader(title: '已信任设备', icon: LucideIcons.users),
            const SizedBox(height: 8),
            GlassCard(
              padding: const EdgeInsets.all(16),
              child: Column(
                children: [
                  ...widget.controller.peers
                      .where((peer) => peer.trusted)
                      .map(
                        (peer) => ListTile(
                          contentPadding: EdgeInsets.zero,
                          leading: Icon(
                            peer.deviceKind == DeviceKind.phone
                                ? LucideIcons.smartphone
                                : LucideIcons.monitor,
                            color: colors.primary,
                          ),
                          title: Text(peer.name),
                          subtitle: Text(peer.address),
                          trailing: IconButton(
                            onPressed: () =>
                                widget.controller.removeTrustedPeer(peer.id),
                            tooltip: '取消信任',
                            icon: const Icon(LucideIcons.trash2, size: 18),
                          ),
                        ),
                      ),
                  if (widget.controller.peers.where((p) => p.trusted).isEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      child: Center(
                        child: Text(
                          '暂无可信设备（在接收弹窗中勾选“记住”即可添加）',
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

class _BrandAboutCard extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return GlassCard(
      padding: const EdgeInsets.all(20),
      borderRadius: 18,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              const BrandDualArrowIcon(
                size: 26,
                withBackground: true,
                borderRadius: 14,
              ),
              const SizedBox(width: 16),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '传输助手 Gemini',
                      style: Theme.of(context).textTheme.titleMedium?.copyWith(
                            fontWeight: FontWeight.w800,
                            letterSpacing: -0.3,
                          ),
                    ),
                    const SizedBox(height: 3),
                    Text(
                      'v1.0.0 · 局域网极速跨端原生互传',
                      style: TextStyle(
                        fontSize: 12,
                        fontWeight: FontWeight.w500,
                        color: colors.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 14),
          const Divider(height: 1),
          const SizedBox(height: 12),
          Text(
            '技术架构：Flutter + Rust 核心 · mDNS 局域网无感发现 · TLS 1.3 双向加密 · 6 位安全配对码 · 4 路并行通道 · BLAKE3 哈希校验 · 断点续传',
            style: TextStyle(
              fontSize: 11,
              height: 1.45,
              color: colors.onSurfaceVariant,
            ),
          ),
        ],
      ),
    );
  }
}
