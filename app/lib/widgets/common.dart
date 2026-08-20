import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../theme/app_theme.dart';

// =========================================================================
// 1. 品牌超级符号：↔ 双向水平对冲箭头 (Brand Dual Arrow)
// =========================================================================

/// 核心品牌双向对冲箭头（上右 →，下左 ←）
class BrandDualArrowIcon extends StatelessWidget {
  const BrandDualArrowIcon({
    super.key,
    this.size = 24,
    this.color,
    this.withBackground = false,
    this.backgroundColor,
    this.gradient,
    this.borderRadius = 8,
    this.hasShadow = true,
  });

  final double size;
  final Color? color;
  final bool withBackground;
  final Color? backgroundColor;
  final Gradient? gradient;
  final double borderRadius;
  final bool hasShadow;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final primary = color ?? colors.primary;

    Widget arrowSvg = SizedBox(
      width: size,
      height: size,
      child: CustomPaint(
        painter: _DualArrowPainter(
          color: withBackground ? Colors.white : primary,
        ),
      ),
    );

    if (!withBackground) {
      return arrowSvg;
    }

    final bgGradient =
        gradient ??
        LinearGradient(
          colors: [AppTheme.brand400, AppTheme.brand600],
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
        );

    return Container(
      width: size * 1.5,
      height: size * 1.5,
      decoration: BoxDecoration(
        color: backgroundColor,
        gradient: backgroundColor == null ? bgGradient : null,
        borderRadius: BorderRadius.circular(borderRadius),
        boxShadow: hasShadow
            ? [
                BoxShadow(
                  color: (color ?? AppTheme.brand500).withValues(alpha: 0.35),
                  blurRadius: size * 0.4,
                  offset: Offset(0, size * 0.15),
                ),
              ]
            : null,
      ),
      child: Center(child: arrowSvg),
    );
  }
}

class _DualArrowPainter extends CustomPainter {
  _DualArrowPainter({required this.color});

  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final strokePaint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeCap = StrokeCap.round
      ..strokeJoin = StrokeJoin.round
      ..strokeWidth = (size.height * 0.14).clamp(1.8, 4.0);

    final w = size.width;
    final h = size.height;

    // 上半部分：向右箭头 (→)
    final topY = h * 0.33;
    final topStartX = w * 0.15;
    final topEndX = w * 0.85;
    final headSize = w * 0.22;

    // 绘制上箭头水平轴线
    canvas.drawLine(
      Offset(topStartX, topY),
      Offset(topEndX, topY),
      strokePaint,
    );
    // 绘制上箭头尖头
    final topPath = Path()
      ..moveTo(topEndX - headSize, topY - headSize * 0.8)
      ..lineTo(topEndX, topY)
      ..lineTo(topEndX - headSize, topY + headSize * 0.8);
    canvas.drawPath(topPath, strokePaint);

    // 下半部分：向左箭头 (←)
    final bottomY = h * 0.67;
    final bottomStartX = w * 0.85;
    final bottomEndX = w * 0.15;

    // 绘制下箭头水平轴线
    canvas.drawLine(
      Offset(bottomStartX, bottomY),
      Offset(bottomEndX, bottomY),
      strokePaint,
    );
    // 绘制下箭头尖头
    final bottomPath = Path()
      ..moveTo(bottomEndX + headSize, bottomY - headSize * 0.8)
      ..lineTo(bottomEndX, bottomY)
      ..lineTo(bottomEndX + headSize, bottomY + headSize * 0.8);
    canvas.drawPath(bottomPath, strokePaint);
  }

  @override
  bool shouldRepaint(covariant _DualArrowPainter oldDelegate) =>
      oldDelegate.color != color;
}

// =========================================================================
// 2. 现代纯净实体卡片 & 状态组件
// =========================================================================

/// 现代高质感实体卡片（去毛玻璃、极简大厂克制美学）
class GlassCard extends StatelessWidget {
  const GlassCard({
    super.key,
    required this.child,
    this.padding = const EdgeInsets.all(16),
    this.borderRadius,
    this.onTap,
    this.borderGradient,
    this.backgroundColor,
    this.enableBlur = false,
  });

  final Widget child;
  final EdgeInsetsGeometry padding;
  final double? borderRadius;
  final VoidCallback? onTap;
  final Gradient? borderGradient;
  final Color? backgroundColor;
  final bool enableBlur;

  @override
  Widget build(BuildContext context) {
    final tokens = Theme.of(context).extension<AppThemeTokens>();
    final radius = borderRadius ?? tokens?.cardRadius ?? 14;
    final isDark = Theme.of(context).brightness == Brightness.dark;

    final defaultBg =
        tokens?.cardBg ?? (isDark ? const Color(0xff131c2e) : Colors.white);
    final defaultBorder =
        tokens?.cardBorder ??
        (isDark ? const Color(0xff1e2a44) : const Color(0xffe2e8f0));

    final shadows =
        tokens?.shadowMd ??
        [
          BoxShadow(
            color: const Color(0x0f0f172a),
            blurRadius: 6,
            offset: const Offset(0, 4),
          ),
        ];

    Widget content = Material(
      color: backgroundColor ?? defaultBg,
      borderRadius: BorderRadius.circular(radius),
      clipBehavior: Clip.antiAlias,
      child: onTap != null
          ? InkWell(
              borderRadius: BorderRadius.circular(radius),
              onTap: onTap,
              child: Container(
                padding: padding,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(radius),
                  border: Border.all(color: defaultBorder, width: 1),
                ),
                child: child,
              ),
            )
          : Container(
              padding: padding,
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(radius),
                border: Border.all(color: defaultBorder, width: 1),
              ),
              child: child,
            ),
    );

    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(radius),
        boxShadow: shadows,
      ),
      child: content,
    );
  }
}

/// 规范状态 Chip：传输中（蓝）/ 已暂停（琥珀）/ 已完成（绿）/ 失败（红）/ 等待（灰），圆角 full + 内置小圆点
class StatusLabel extends StatelessWidget {
  const StatusLabel({super.key, required this.state});

  final TransferState state;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;

    final (label, dotColor, bgAlpha) = switch (state) {
      TransferState.preparing => ('准备中', AppTheme.brand500, 0.12),
      TransferState.connecting => ('连接中', AppTheme.brand500, 0.14),
      TransferState.pairing => ('安全配对中', AppTheme.accent, 0.15),
      TransferState.waitingForAcceptance => ('等待接收', AppTheme.accent, 0.15),
      TransferState.transferring => (
        '传输中',
        isDark ? AppTheme.brand400 : AppTheme.brand600,
        0.16,
      ),
      TransferState.paused => ('已暂停', AppTheme.warning, 0.14),
      TransferState.interrupted => ('已中断', AppTheme.error, 0.14),
      TransferState.verifying => ('BLAKE3 校验中', AppTheme.brand500, 0.14),
      TransferState.completed => ('已完成', AppTheme.success, 0.15),
      TransferState.failed => ('传输失败', AppTheme.error, 0.14),
      TransferState.cancelled => ('已取消', colors.onSurfaceVariant, 0.10),
    };

    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
      decoration: BoxDecoration(
        color: dotColor.withValues(alpha: bgAlpha),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(
          color: dotColor.withValues(alpha: isDark ? 0.35 : 0.25),
          width: 1,
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Container(
            width: 6,
            height: 6,
            decoration: BoxDecoration(color: dotColor, shape: BoxShape.circle),
          ),
          const SizedBox(width: 6),
          Text(
            label,
            style: TextStyle(
              fontSize: 11,
              fontWeight: FontWeight.w600,
              color: dotColor,
              letterSpacing: -0.1,
            ),
          ),
        ],
      ),
    );
  }
}

/// 统一虚线边框卡片 + 双向箭头插画空状态 (Brand Empty State)
class EmptyState extends StatelessWidget {
  const EmptyState({
    super.key,
    this.icon,
    required this.title,
    this.description,
    required this.action,
  });

  final IconData? icon;
  final String title;
  final String? description;
  final Widget action;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;

    return Center(
      child: SingleChildScrollView(
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 480),
          child: Container(
            margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
            padding: const EdgeInsets.symmetric(vertical: 28, horizontal: 22),
            decoration: BoxDecoration(
              color: isDark ? const Color(0xff131c2e) : Colors.white,
              borderRadius: BorderRadius.circular(18),
              border: Border.all(
                color: isDark
                    ? const Color(0xff1e2a44)
                    : const Color(0xffe2e8f0),
                width: 1.2,
              ),
              boxShadow: [
                BoxShadow(
                  color: Colors.black.withValues(alpha: isDark ? 0.2 : 0.04),
                  blurRadius: 12,
                  offset: const Offset(0, 3),
                ),
              ],
            ),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                // 品牌双向箭头插画圆盘
                Container(
                  width: 64,
                  height: 64,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    gradient: LinearGradient(
                      colors: [
                        AppTheme.brand400.withValues(alpha: 0.16),
                        AppTheme.brand600.withValues(alpha: 0.08),
                      ],
                      begin: Alignment.topLeft,
                      end: Alignment.bottomRight,
                    ),
                    border: Border.all(
                      color: AppTheme.brand500.withValues(alpha: 0.3),
                      width: 1.5,
                    ),
                    boxShadow: const [
                      BoxShadow(
                        color: Color(0x330ea5e9),
                        blurRadius: 16,
                        offset: Offset(0, 4),
                      ),
                    ],
                  ),
                  child: Center(
                    child: BrandDualArrowIcon(
                      size: 26,
                      color: isDark ? AppTheme.brand400 : AppTheme.brand600,
                    ),
                  ),
                ),
                const SizedBox(height: 18),
                Text(
                  title,
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.w800,
                    letterSpacing: -0.3,
                    fontSize: 16,
                  ),
                  textAlign: TextAlign.center,
                ),
                if (description != null) ...[
                  const SizedBox(height: 8),
                  ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 380),
                    child: Text(
                      description!,
                      style: TextStyle(
                        fontSize: 12,
                        color: colors.onSurfaceVariant,
                        height: 1.5,
                      ),
                      textAlign: TextAlign.center,
                    ),
                  ),
                ],
                const SizedBox(height: 22),
                action,
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// 8px 高度品牌渐变 + 白色流光扫光动态进度条 (Liquid Progress Bar)
class LiquidProgressBar extends StatefulWidget {
  const LiquidProgressBar({
    super.key,
    required this.progress,
    this.height = 8,
    this.color,
    this.backgroundColor,
  });

  final double progress;
  final double height;
  final Color? color;
  final Color? backgroundColor;

  @override
  State<LiquidProgressBar> createState() => _LiquidProgressBarState();
}

class _LiquidProgressBarState extends State<LiquidProgressBar>
    with SingleTickerProviderStateMixin {
  late final AnimationController _sweepController;

  @override
  void initState() {
    super.initState();
    _sweepController = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 1800),
    )..repeat();
  }

  @override
  void dispose() {
    _sweepController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final isDark = Theme.of(context).brightness == Brightness.dark;
    final bg =
        widget.backgroundColor ??
        (isDark ? const Color(0xff16213a) : const Color(0xffe2e8f0));

    final clampedProgress = widget.progress.clamp(0.0, 1.0);

    return RepaintBoundary(
      child: ClipRRect(
        borderRadius: BorderRadius.circular(widget.height / 2),
        child: Container(
          height: widget.height,
          color: bg,
          child: LayoutBuilder(
            builder: (context, constraints) {
              final fillWidth = constraints.maxWidth * clampedProgress;
              return Stack(
                children: [
                  Container(
                    width: fillWidth,
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(widget.height / 2),
                      gradient: LinearGradient(
                        colors: [AppTheme.brand400, AppTheme.brand600],
                        begin: Alignment.centerLeft,
                        end: Alignment.centerRight,
                      ),
                    ),
                  ),
                  if (clampedProgress > 0.05)
                    AnimatedBuilder(
                      animation: _sweepController,
                      builder: (context, child) {
                        final sweepX =
                            -fillWidth +
                            (fillWidth * 2) * _sweepController.value;
                        return Positioned(
                          left: sweepX,
                          top: 0,
                          bottom: 0,
                          width: fillWidth * 0.4,
                          child: Container(
                            decoration: BoxDecoration(
                              gradient: LinearGradient(
                                colors: [
                                  Colors.white.withValues(alpha: 0.0),
                                  Colors.white.withValues(alpha: 0.55),
                                  Colors.white.withValues(alpha: 0.0),
                                ],
                              ),
                            ),
                          ),
                        );
                      },
                    ),
                ],
              );
            },
          ),
        ),
      ),
    );
  }
}

/// 6 位等宽安全配对核验弹窗 (Pairing Code Dialog)
class PairingCodeDialog extends StatelessWidget {
  const PairingCodeDialog({
    super.key,
    required this.peerName,
    required this.pairingCode,
    required this.onAccept,
    required this.onReject,
  });

  final String peerName;
  final String pairingCode;
  final VoidCallback onAccept;
  final VoidCallback onReject;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final isDark = Theme.of(context).brightness == Brightness.dark;

    return Dialog(
      shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(20)),
      backgroundColor: isDark ? const Color(0xff131c2e) : Colors.white,
      child: Padding(
        padding: const EdgeInsets.all(24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              width: 52,
              height: 52,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: AppTheme.brand500.withValues(alpha: 0.12),
                border: Border.all(
                  color: AppTheme.brand500.withValues(alpha: 0.3),
                  width: 1.5,
                ),
              ),
              child: Icon(
                LucideIcons.shieldCheck,
                color: isDark ? AppTheme.brand400 : AppTheme.brand600,
                size: 26,
              ),
            ),
            const SizedBox(height: 16),
            Text(
              '与「$peerName」核对安全码',
              style: const TextStyle(fontSize: 16, fontWeight: FontWeight.w700),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 6),
            Text(
              '请确认对方设备屏幕上显示的 6 位配对码一致，以确保 TLS 1.3 双向加密传输安全。',
              style: TextStyle(fontSize: 12, color: colors.onSurfaceVariant),
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 20),
            // 6 位超大等宽配对码卡片
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14),
              decoration: BoxDecoration(
                color: isDark
                    ? const Color(0xff0b1120)
                    : const Color(0xfff1f5f9),
                borderRadius: BorderRadius.circular(12),
                border: Border.all(
                  color: isDark
                      ? const Color(0xff1e2a44)
                      : const Color(0xffe2e8f0),
                ),
              ),
              child: Text(
                pairingCode,
                style: TextStyle(
                  fontFamily: 'monospace',
                  fontSize: 32,
                  fontWeight: FontWeight.w800,
                  letterSpacing: 8,
                  color: isDark ? AppTheme.brand400 : AppTheme.brand600,
                ),
              ),
            ),
            const SizedBox(height: 24),
            Row(
              children: [
                Expanded(
                  child: OutlinedButton(
                    onPressed: onReject,
                    style: OutlinedButton.styleFrom(
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(10),
                      ),
                    ),
                    child: const Text('不匹配，取消'),
                  ),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: FilledButton(
                    onPressed: onAccept,
                    style: FilledButton.styleFrom(
                      backgroundColor: isDark
                          ? AppTheme.brand400
                          : AppTheme.brand600,
                      foregroundColor: Colors.white,
                      padding: const EdgeInsets.symmetric(vertical: 12),
                      shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(10),
                      ),
                    ),
                    child: const Text('一致，继续传输'),
                  ),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

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
    return Container(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: colors.errorContainer.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: colors.error.withValues(alpha: 0.4)),
        boxShadow: [
          BoxShadow(
            color: colors.error.withValues(alpha: 0.15),
            blurRadius: 10,
            offset: const Offset(0, 2),
          ),
        ],
      ),
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
              style: TextStyle(
                color: colors.onErrorContainer,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
          IconButton(
            onPressed: onDismiss,
            tooltip: '关闭',
            icon: const Icon(LucideIcons.x, size: 18),
            visualDensity: VisualDensity.compact,
          ),
        ],
      ),
    );
  }
}

class NoticeBanner extends StatelessWidget {
  const NoticeBanner({
    super.key,
    required this.message,
    required this.onDismiss,
    this.actionLabel,
    this.onAction,
  });

  final String message;
  final VoidCallback onDismiss;
  final String? actionLabel;
  final VoidCallback? onAction;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Container(
      margin: const EdgeInsets.symmetric(horizontal: 16, vertical: 8),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      decoration: BoxDecoration(
        color: colors.primaryContainer.withValues(alpha: 0.9),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: colors.primary.withValues(alpha: 0.4)),
        boxShadow: [
          BoxShadow(
            color: colors.primary.withValues(alpha: 0.15),
            blurRadius: 10,
            offset: const Offset(0, 2),
          ),
        ],
      ),
      child: Row(
        children: [
          Icon(
            LucideIcons.sparkles,
            size: 18,
            color: colors.onPrimaryContainer,
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              message,
              style: TextStyle(
                color: colors.onPrimaryContainer,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
          if (actionLabel != null && onAction != null)
            TextButton(onPressed: onAction, child: Text(actionLabel!)),
          IconButton(
            onPressed: onDismiss,
            tooltip: '关闭',
            icon: const Icon(LucideIcons.x, size: 18),
            visualDensity: VisualDensity.compact,
          ),
        ],
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
              style: Theme.of(context).textTheme.headlineSmall?.copyWith(
                fontWeight: FontWeight.w800,
                letterSpacing: -0.5,
              ),
            ),
          ),
          ...actions,
        ],
      ),
    );
  }
}

/// 呼吸雷达动画指示器
class RadarPulseIndicator extends StatefulWidget {
  const RadarPulseIndicator({
    super.key,
    this.size = 28,
    this.color,
    this.isActive = true,
  });

  final double size;
  final Color? color;
  final bool isActive;

  @override
  State<RadarPulseIndicator> createState() => _RadarPulseIndicatorState();
}

class _RadarPulseIndicatorState extends State<RadarPulseIndicator>
    with SingleTickerProviderStateMixin {
  late final AnimationController _controller;

  @override
  void initState() {
    super.initState();
    _controller = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 2400),
    )..repeat();
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final themeColor = widget.color ?? Theme.of(context).colorScheme.primary;
    if (!widget.isActive) {
      return SizedBox(
        width: widget.size,
        height: widget.size,
        child: Center(
          child: Container(
            width: 8,
            height: 8,
            decoration: BoxDecoration(
              color: themeColor.withValues(alpha: 0.4),
              shape: BoxShape.circle,
            ),
          ),
        ),
      );
    }

    return SizedBox(
      width: widget.size,
      height: widget.size,
      child: RepaintBoundary(
        child: AnimatedBuilder(
          animation: _controller,
          builder: (context, child) {
            final val1 = _controller.value;
            final val2 = (val1 + 0.5) % 1.0;
            return CustomPaint(
              painter: _RadarWavePainter(
                color: themeColor,
                progress1: val1,
                progress2: val2,
              ),
            );
          },
        ),
      ),
    );
  }
}

class _RadarWavePainter extends CustomPainter {
  _RadarWavePainter({
    required this.color,
    required this.progress1,
    required this.progress2,
  });

  final Color color;
  final double progress1;
  final double progress2;

  @override
  void paint(Canvas canvas, Size size) {
    final center = Offset(size.width / 2, size.height / 2);
    final maxRadius = size.width / 2;

    void drawRing(double progress) {
      final radius = maxRadius * progress;
      final opacity = (1.0 - progress).clamp(0.0, 1.0);
      final paint = Paint()
        ..color = color.withValues(alpha: opacity * 0.45)
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1.6;
      canvas.drawCircle(center, radius, paint);

      final fillPaint = Paint()
        ..color = color.withValues(alpha: opacity * 0.08)
        ..style = PaintingStyle.fill;
      canvas.drawCircle(center, radius, fillPaint);
    }

    drawRing(progress1);
    drawRing(progress2);

    final corePaint = Paint()
      ..color = color
      ..style = PaintingStyle.fill;
    canvas.drawCircle(center, 3.5, corePaint);
  }

  @override
  bool shouldRepaint(covariant _RadarWavePainter oldDelegate) =>
      oldDelegate.progress1 != progress1 || oldDelegate.progress2 != progress2;
}

/// 模块化卡片分组组件
class SettingsSectionCard extends StatelessWidget {
  const SettingsSectionCard({
    super.key,
    required this.title,
    required this.icon,
    required this.children,
    this.subtitle,
  });

  final String title;
  final String? subtitle;
  final IconData icon;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return GlassCard(
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 16, 18, 12),
            child: Row(
              children: [
                Container(
                  padding: const EdgeInsets.all(7),
                  decoration: BoxDecoration(
                    color: colors.primary.withValues(alpha: 0.12),
                    borderRadius: BorderRadius.circular(8),
                  ),
                  child: Icon(icon, size: 16, color: colors.primary),
                ),
                const SizedBox(width: 12),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: Theme.of(context).textTheme.titleSmall?.copyWith(
                        fontWeight: FontWeight.w700,
                        letterSpacing: -0.2,
                      ),
                    ),
                    if (subtitle != null) ...[
                      const SizedBox(height: 2),
                      Text(
                        subtitle!,
                        style: Theme.of(context).textTheme.bodySmall?.copyWith(
                          color: colors.onSurfaceVariant,
                          fontSize: 11,
                        ),
                      ),
                    ],
                  ],
                ),
              ],
            ),
          ),
          const Divider(height: 1),
          ...children,
        ],
      ),
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
