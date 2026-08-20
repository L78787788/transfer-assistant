import 'dart:ui';
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../state/app_controller.dart';
import '../core/models.dart';
import '../views/history_view.dart';
import '../views/nearby_view.dart';
import '../views/settings_view.dart';
import '../views/transfers_view.dart';
import '../widgets/common.dart';
import '../theme/app_theme.dart';

class AppShell extends StatelessWidget {
  const AppShell({super.key, required this.controller});

  final AppController controller;

  static const _items = [
    (LucideIcons.radio, '附近设备'),
    (LucideIcons.arrowUpDown, '传输中'),
    (LucideIcons.history, '历史记录'),
    (LucideIcons.settings, '设置'),
  ];

  @override
  Widget build(BuildContext context) {
    final pages = [
      RepaintBoundary(
        child: TickerMode(
          enabled: controller.selectedPage == 0,
          child: NearbyView(controller: controller),
        ),
      ),
      RepaintBoundary(
        child: TickerMode(
          enabled: controller.selectedPage == 1,
          child: TransfersView(controller: controller),
        ),
      ),
      RepaintBoundary(
        child: TickerMode(
          enabled: controller.selectedPage == 2,
          child: HistoryView(controller: controller),
        ),
      ),
      RepaintBoundary(
        child: TickerMode(
          enabled: controller.selectedPage == 3,
          child: SettingsView(controller: controller),
        ),
      ),
    ];

    return LayoutBuilder(
      builder: (context, constraints) {
        final desktop = constraints.maxWidth >= 840;
        final isDark = Theme.of(context).brightness == Brightness.dark;

        return Stack(
          children: [
            // 自适应主题氛围背景
            Positioned.fill(
              child: RepaintBoundary(child: _AmbientBackground(isDark: isDark)),
            ),
            Scaffold(
              backgroundColor: Colors.transparent,
              body: SafeArea(
                bottom: false,
                child: Column(
                  children: [
                    if (controller.errorMessage case final message?)
                      ErrorBanner(
                        message: message,
                        onDismiss: controller.clearError,
                      ),
                    if (controller.noticeMessage case final message?)
                      NoticeBanner(
                        message: message,
                        onDismiss: controller.clearNotice,
                        actionLabel: controller.selectedPage != 1
                            ? '查看任务'
                            : null,
                        onAction: () => controller.selectPage(1),
                      ),
                    Expanded(
                      child: Row(
                        children: [
                          if (desktop)
                            RepaintBoundary(
                              child: _DesktopNavigation(controller: controller),
                            ),
                          if (desktop)
                            VerticalDivider(
                              width: 1,
                              color: isDark
                                  ? const Color(
                                      0xff1e293b,
                                    ).withValues(alpha: 0.6)
                                  : const Color(0xffe2e8f0),
                            ),
                          Expanded(
                            child: IndexedStack(
                              index: controller.selectedPage,
                              children: pages,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              bottomNavigationBar: desktop
                  ? null
                  : RepaintBoundary(
                      child: _FloatingGlassNavBar(
                        selectedIndex: controller.selectedPage,
                        onSelected: controller.selectPage,
                        items: _items,
                        activeTransfersCount: controller.activeTransfers.length,
                      ),
                    ),
            ),
            if (!controller.isInitializing &&
                !controller.settings.hasCompletedFirstSetup)
              _FirstLaunchNamingOverlay(controller: controller),
            if (controller.pendingOffer case final TransferOffer offer)
              _OfferOverlay(controller: controller, offer: offer),
          ],
        );
      },
    );
  }
}

/// 统一的现代纯净大厂背景（去毛玻璃、极简大气质感）
class _AmbientBackground extends StatelessWidget {
  const _AmbientBackground({required this.isDark});

  final bool isDark;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return Container(
      color: isDark ? const Color(0xff0b1120) : const Color(0xfff8fafc),
      child: Stack(
        children: [
          Positioned(
            top: -100,
            right: -60,
            width: 360,
            height: 360,
            child: Container(
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                gradient: RadialGradient(
                  colors: [
                    colors.primary.withValues(alpha: isDark ? 0.08 : 0.04),
                    Colors.transparent,
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// 自适应悬浮药丸导航栏
class _FloatingGlassNavBar extends StatelessWidget {
  const _FloatingGlassNavBar({
    required this.selectedIndex,
    required this.onSelected,
    required this.items,
    this.activeTransfersCount = 0,
  });

  final int selectedIndex;
  final ValueChanged<int> onSelected;
  final List<(IconData, String)> items;
  final int activeTransfersCount;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final tokens = Theme.of(context).extension<AppThemeTokens>();
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final radius = tokens?.cardRadius ?? 28;

    Widget barContent = Container(
      height: 64,
      padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      decoration: BoxDecoration(
        color:
            tokens?.cardBg ??
            (isDark
                ? const Color(0xff131c2e).withValues(alpha: 0.8)
                : Colors.white.withValues(alpha: 0.85)),
        borderRadius: BorderRadius.circular(radius > 16 ? radius : 28),
        border: Border.all(
          color:
              tokens?.cardBorder ??
              (isDark
                  ? const Color(0xff2d3b59).withValues(alpha: 0.6)
                  : const Color(0xffe2e8f0)),
          width: 1,
        ),
        boxShadow: [
          BoxShadow(
            color: isDark
                ? Colors.black.withValues(alpha: 0.35)
                : const Color(0x14000000),
            blurRadius: 20,
            offset: const Offset(0, 6),
          ),
        ],
      ),
      child: Row(
        children: List.generate(items.length, (index) {
          final isSelected = selectedIndex == index;
          final item = items[index];
          final showBadge = index == 1 && activeTransfersCount > 0;

          return Expanded(
            child: InkWell(
              borderRadius: BorderRadius.circular(22),
              onTap: () => onSelected(index),
              child: AnimatedContainer(
                duration: const Duration(milliseconds: 200),
                curve: Curves.easeOutCubic,
                decoration: BoxDecoration(
                  color: isSelected
                      ? colors.primary.withValues(alpha: isDark ? 0.18 : 0.12)
                      : Colors.transparent,
                  borderRadius: BorderRadius.circular(22),
                ),
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Stack(
                      clipBehavior: Clip.none,
                      children: [
                        Icon(
                          item.$1,
                          size: 20,
                          color: isSelected
                              ? colors.primary
                              : colors.onSurfaceVariant,
                        ),
                        if (showBadge)
                          Positioned(
                            right: -6,
                            top: -4,
                            child: Container(
                              padding: const EdgeInsets.all(3.5),
                              decoration: BoxDecoration(
                                color: colors.primary,
                                shape: BoxShape.circle,
                              ),
                              constraints: const BoxConstraints(
                                minWidth: 8,
                                minHeight: 8,
                              ),
                            ),
                          ),
                      ],
                    ),
                    const SizedBox(height: 3),
                    Text(
                      item.$2,
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: isSelected
                            ? FontWeight.w600
                            : FontWeight.w500,
                        color: isSelected
                            ? colors.primary
                            : colors.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          );
        }),
      ),
    );

    barContent = ClipRRect(
      borderRadius: BorderRadius.circular(radius > 16 ? radius : 28),
      child: barContent,
    );

    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
        child: barContent,
      ),
    );
  }
}

class _DesktopNavigation extends StatelessWidget {
  const _DesktopNavigation({required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final selfInitial = controller.selfPeerName.isNotEmpty
        ? controller.selfPeerName.characters.first.toUpperCase()
        : '我';

    return Container(
      width: 248,
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 18),
      color: isDark ? const Color(0xff0b1120) : const Color(0xfff8fafc),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // 顶部品牌超级符号与名称
          Padding(
            padding: const EdgeInsets.fromLTRB(6, 4, 6, 12),
            child: Row(
              children: [
                const BrandDualArrowIcon(
                  size: 20,
                  withBackground: true,
                  borderRadius: 10,
                ),
                const SizedBox(width: 12),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '互传',
                      style: Theme.of(context).textTheme.titleMedium?.copyWith(
                        fontWeight: FontWeight.w800,
                        letterSpacing: -0.4,
                        fontSize: 16,
                      ),
                    ),
                    const SizedBox(height: 1),
                    Text(
                      '局域网极速互传',
                      style: TextStyle(
                        fontSize: 11,
                        fontWeight: FontWeight.w500,
                        color: colors.onSurfaceVariant,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 18),
          _navItem(context, index: 0, icon: LucideIcons.radio, label: '附近设备'),
          const SizedBox(height: 4),
          _navItem(
            context,
            index: 1,
            icon: LucideIcons.arrowUpDown,
            label: '传输中',
            badge: controller.activeTransfers.isNotEmpty
                ? '${controller.activeTransfers.length}'
                : null,
          ),
          const SizedBox(height: 4),
          _navItem(context, index: 2, icon: LucideIcons.history, label: '历史记录'),
          const SizedBox(height: 4),
          _navItem(context, index: 3, icon: LucideIcons.settings, label: '设置'),
          const Spacer(),
          // 底部本机状态卡片（首字母头像 + 设备名 + 在线脉冲）
          Container(
            padding: const EdgeInsets.all(12),
            decoration: BoxDecoration(
              color: isDark ? const Color(0xff131c2e) : Colors.white,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(
                color: isDark
                    ? const Color(0xff1e2a44)
                    : const Color(0xffe2e8f0),
              ),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: isDark ? 0.2 : 0.04),
                  blurRadius: 8,
                  offset: const Offset(0, 2),
                ),
              ],
            ),
            child: Row(
              children: [
                // 设备首字母圆形徽标
                Container(
                  width: 32,
                  height: 32,
                  decoration: BoxDecoration(
                    color: colors.primary.withValues(alpha: 0.14),
                    shape: BoxShape.circle,
                  ),
                  child: Center(
                    child: Text(
                      selfInitial,
                      style: TextStyle(
                        fontSize: 13,
                        fontWeight: FontWeight.w800,
                        color: colors.primary,
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        controller.selfPeerName,
                        style: const TextStyle(
                          fontSize: 12,
                          fontWeight: FontWeight.w700,
                        ),
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 1),
                      Row(
                        children: [
                          Container(
                            width: 6,
                            height: 6,
                            decoration: const BoxDecoration(
                              color: Color(0xff10b981),
                              shape: BoxShape.circle,
                            ),
                          ),
                          const SizedBox(width: 4),
                          Text(
                            'mDNS 在线',
                            style: TextStyle(
                              fontSize: 10,
                              fontWeight: FontWeight.w500,
                              color: colors.onSurfaceVariant,
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  Widget _navItem(
    BuildContext context, {
    required int index,
    required IconData icon,
    required String label,
    String? badge,
  }) {
    final isSelected = controller.selectedPage == index;
    final colors = Theme.of(context).colorScheme;

    return Material(
      color: Colors.transparent,
      child: InkWell(
        borderRadius: BorderRadius.circular(10),
        onTap: () => controller.selectPage(index),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 150),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
          decoration: BoxDecoration(
            color: isSelected
                ? colors.primary.withValues(alpha: 0.12)
                : Colors.transparent,
            borderRadius: BorderRadius.circular(10),
            border: isSelected
                ? Border.all(
                    color: colors.primary.withValues(alpha: 0.25),
                    width: 1,
                  )
                : null,
          ),
          child: Row(
            children: [
              Icon(
                icon,
                size: 18,
                color: isSelected ? colors.primary : colors.onSurfaceVariant,
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                    color: isSelected ? colors.primary : colors.onSurface,
                  ),
                ),
              ),
              if (badge != null)
                Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 6,
                    vertical: 2,
                  ),
                  decoration: BoxDecoration(
                    color: colors.primary,
                    borderRadius: BorderRadius.circular(10),
                  ),
                  child: Text(
                    badge,
                    style: TextStyle(
                      fontSize: 10,
                      fontWeight: FontWeight.w700,
                      color: colors.onPrimary,
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

class _OfferOverlay extends StatefulWidget {
  const _OfferOverlay({required this.controller, required this.offer});

  final AppController controller;
  final TransferOffer offer;

  @override
  State<_OfferOverlay> createState() => _OfferOverlayState();
}

class _OfferOverlayState extends State<_OfferOverlay> {
  bool _trust = false;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return Material(
      color: Colors.black54,
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 8, sigmaY: 8),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: SingleChildScrollView(
                child: GlassCard(
                  borderRadius: 20,
                  padding: const EdgeInsets.all(24),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Container(
                        width: 52,
                        height: 52,
                        decoration: BoxDecoration(
                          color: colors.primary.withValues(alpha: 0.12),
                          shape: BoxShape.circle,
                          border: Border.all(
                            color: colors.primary.withValues(alpha: 0.3),
                            width: 1.5,
                          ),
                        ),
                        child: Icon(
                          LucideIcons.shieldCheck,
                          size: 26,
                          color: colors.primary,
                        ),
                      ),
                      const SizedBox(height: 16),
                      Text(
                        widget.offer.direction == TransferDirection.outgoing
                            ? '与「${widget.offer.peerName}」核对安全码'
                            : '接收「${widget.offer.peerName}」的文件',
                        style: Theme.of(context).textTheme.titleMedium
                            ?.copyWith(fontWeight: FontWeight.w700),
                        textAlign: TextAlign.center,
                      ),
                      const SizedBox(height: 6),
                      Text(
                        '${widget.offer.itemCount} 项文件 · 共 ${_formatBytes(widget.offer.totalBytes)}',
                        style: TextStyle(
                          color: colors.onSurfaceVariant,
                          fontSize: 13,
                        ),
                        textAlign: TextAlign.center,
                      ),
                      const SizedBox(height: 20),
                      // 配对验证码
                      Container(
                        width: double.infinity,
                        padding: const EdgeInsets.symmetric(
                          horizontal: 20,
                          vertical: 14,
                        ),
                        decoration: BoxDecoration(
                          color: colors.primary.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(14),
                          border: Border.all(
                            color: colors.primary.withValues(alpha: 0.25),
                          ),
                        ),
                        child: Column(
                          children: [
                            Text(
                              '配对安全验证码',
                              style: TextStyle(
                                fontSize: 11,
                                color: colors.onSurfaceVariant,
                              ),
                            ),
                            const SizedBox(height: 4),
                            SelectableText(
                              widget.offer.pairingCode ?? '------',
                              style: TextStyle(
                                fontSize: 28,
                                fontWeight: FontWeight.w800,
                                letterSpacing: 6,
                                color: colors.primary,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              '请核对两台设备显示的六位数字是否一致',
                              style: TextStyle(
                                fontSize: 11,
                                color: colors.onSurfaceVariant,
                              ),
                              textAlign: TextAlign.center,
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 12),
                      // 记住此设备
                      InkWell(
                        borderRadius: BorderRadius.circular(8),
                        onTap: () => setState(() => _trust = !_trust),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            vertical: 6,
                            horizontal: 4,
                          ),
                          child: Row(
                            children: [
                              SizedBox(
                                width: 24,
                                height: 24,
                                child: Checkbox(
                                  value: _trust,
                                  onChanged: (v) =>
                                      setState(() => _trust = v ?? false),
                                ),
                              ),
                              const SizedBox(width: 8),
                              const Expanded(
                                child: Text(
                                  '信任此设备（下次自动接收）',
                                  style: TextStyle(fontSize: 12),
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                      const SizedBox(height: 20),
                      Row(
                        children: [
                          Expanded(
                            child: OutlinedButton(
                              onPressed: () => widget.controller.answerOffer(
                                accept: false,
                                rememberPeer: false,
                              ),
                              child: Text(
                                widget.offer.direction ==
                                        TransferDirection.outgoing
                                    ? '不匹配'
                                    : '拒绝',
                              ),
                            ),
                          ),
                          const SizedBox(width: 12),
                          Expanded(
                            child: FilledButton.icon(
                              onPressed: () => widget.controller.answerOffer(
                                accept: true,
                                rememberPeer: _trust,
                              ),
                              icon: const Icon(LucideIcons.check, size: 18),
                              label: Text(
                                widget.offer.direction ==
                                        TransferDirection.outgoing
                                    ? '一致，继续'
                                    : '接收',
                              ),
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  String _formatBytes(int bytes) {
    if (bytes < 1024) return '$bytes B';
    if (bytes < 1024 * 1024) {
      return '${(bytes / 1024).toStringAsFixed(1)} KB';
    }
    if (bytes < 1024 * 1024 * 1024) {
      return '${(bytes / (1024 * 1024)).toStringAsFixed(1)} MB';
    }
    return '${(bytes / (1024 * 1024 * 1024)).toStringAsFixed(2)} GB';
  }
}

/// 首次启动设备命名引导弹窗
class _FirstLaunchNamingOverlay extends StatefulWidget {
  const _FirstLaunchNamingOverlay({required this.controller});

  final AppController controller;

  @override
  State<_FirstLaunchNamingOverlay> createState() =>
      _FirstLaunchNamingOverlayState();
}

class _FirstLaunchNamingOverlayState extends State<_FirstLaunchNamingOverlay> {
  late final TextEditingController _nameController;
  String? _errorMessage;

  @override
  void initState() {
    super.initState();
    _nameController = TextEditingController(
      text: widget.controller.settings.deviceName,
    );
  }

  @override
  void dispose() {
    _nameController.dispose();
    super.dispose();
  }

  void _submit() {
    final name = _nameController.text.trim();
    if (name.isEmpty) {
      setState(() {
        _errorMessage = '设备名称不能为空';
      });
      return;
    }
    widget.controller.updateSettings(
      widget.controller.settings.copyWith(
        deviceName: name,
        hasCompletedFirstSetup: true,
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final isDark = theme.brightness == Brightness.dark;

    return Material(
      color: Colors.black.withValues(alpha: 0.65),
      child: BackdropFilter(
        filter: ImageFilter.blur(sigmaX: 10, sigmaY: 10),
        child: Center(
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 440),
            child: Padding(
              padding: const EdgeInsets.all(24),
              child: SingleChildScrollView(
                child: GlassCard(
                  borderRadius: 24,
                  padding: const EdgeInsets.all(28),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Center(
                        child: Container(
                          width: 58,
                          height: 58,
                          decoration: BoxDecoration(
                            gradient: LinearGradient(
                              begin: Alignment.topLeft,
                              end: Alignment.bottomRight,
                              colors: [colors.primary, colors.secondary],
                            ),
                            shape: BoxShape.circle,
                            boxShadow: [
                              BoxShadow(
                                color: colors.primary.withValues(alpha: 0.35),
                                blurRadius: 16,
                                offset: const Offset(0, 6),
                              ),
                            ],
                          ),
                          child: const Icon(
                            LucideIcons.sparkles,
                            size: 28,
                            color: Colors.white,
                          ),
                        ),
                      ),
                      const SizedBox(height: 20),
                      Text(
                        '欢迎使用互传',
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          fontSize: 20,
                          fontWeight: FontWeight.w700,
                          color: colors.onSurface,
                          letterSpacing: -0.5,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        '请为这台设备起一个名称，局域网内的其他手机或电脑将通过此名称快速识别您。',
                        textAlign: TextAlign.center,
                        style: TextStyle(
                          fontSize: 13,
                          height: 1.45,
                          color: colors.onSurfaceVariant,
                        ),
                      ),
                      const SizedBox(height: 24),
                      TextField(
                        controller: _nameController,
                        autofocus: true,
                        textInputAction: TextInputAction.done,
                        onSubmitted: (_) => _submit(),
                        onChanged: (val) {
                          if (_errorMessage != null && val.trim().isNotEmpty) {
                            setState(() => _errorMessage = null);
                          }
                        },
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w600,
                          color: colors.onSurface,
                        ),
                        decoration: InputDecoration(
                          labelText: '本设备名称',
                          hintText: '例如：我的工作电脑 / 小米的手机',
                          errorText: _errorMessage,
                          prefixIcon: Icon(
                            LucideIcons.smartphone,
                            size: 20,
                            color: colors.primary,
                          ),
                          suffixIcon: IconButton(
                            icon: const Icon(LucideIcons.x, size: 16),
                            onPressed: () {
                              _nameController.clear();
                              setState(() => _errorMessage = null);
                            },
                          ),
                          filled: true,
                          fillColor: isDark
                              ? const Color(0xff1e293b).withValues(alpha: 0.5)
                              : const Color(0xfff1f5f9),
                          border: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(12),
                            borderSide: BorderSide.none,
                          ),
                          focusedBorder: OutlineInputBorder(
                            borderRadius: BorderRadius.circular(12),
                            borderSide: BorderSide(
                              color: colors.primary,
                              width: 1.5,
                            ),
                          ),
                        ),
                      ),
                      const SizedBox(height: 16),
                      Container(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 14,
                          vertical: 10,
                        ),
                        decoration: BoxDecoration(
                          color: colors.primary.withValues(alpha: 0.08),
                          borderRadius: BorderRadius.circular(10),
                          border: Border.all(
                            color: colors.primary.withValues(alpha: 0.18),
                            width: 1,
                          ),
                        ),
                        child: Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Icon(
                              LucideIcons.info,
                              size: 16,
                              color: colors.primary,
                            ),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Text(
                                '提示：首次设置后，您可以随时在「设置」页面中重新修改设备名称与接收目录。',
                                style: TextStyle(
                                  fontSize: 11.5,
                                  height: 1.4,
                                  color: colors.onSurfaceVariant,
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 24),
                      FilledButton(
                        onPressed: _submit,
                        style: FilledButton.styleFrom(
                          padding: const EdgeInsets.symmetric(vertical: 14),
                          shape: RoundedRectangleBorder(
                            borderRadius: BorderRadius.circular(12),
                          ),
                        ),
                        child: const Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(LucideIcons.check, size: 18),
                            SizedBox(width: 8),
                            Text(
                              '完成并开启极速传输',
                              style: TextStyle(
                                fontSize: 14.5,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
