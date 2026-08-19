import 'package:flutter/material.dart';
import '../core/models.dart';

/// 扩展主题设计 Token · 完整设计系统规范
class AppThemeTokens extends ThemeExtension<AppThemeTokens> {
  const AppThemeTokens({
    required this.style,
    required this.brand400,
    required this.brand500,
    required this.brand600,
    required this.brand700,
    required this.accent,
    required this.brandGradient,
    required this.surface2,
    required this.border,
    required this.textSecondary,
    required this.textTertiary,
    required this.success,
    required this.warning,
    required this.cardRadius,
    required this.cardBg,
    required this.cardBorder,
    required this.shadowIcon,
    required this.shadowGlow,
    required this.shadowMd,
  });

  final AppThemeStyle style;
  final Color brand400;
  final Color brand500;
  final Color brand600;
  final Color brand700;
  final Color accent;
  final Gradient brandGradient;
  final Color surface2;
  final Color border;
  final Color textSecondary;
  final Color textTertiary;
  final Color success;
  final Color warning;
  final double cardRadius;
  final Color cardBg;
  final Color cardBorder;
  final List<BoxShadow> shadowIcon;
  final List<BoxShadow> shadowGlow;
  final List<BoxShadow> shadowMd;

  @override
  ThemeExtension<AppThemeTokens> copyWith({
    AppThemeStyle? style,
    Color? brand400,
    Color? brand500,
    Color? brand600,
    Color? brand700,
    Color? accent,
    Gradient? brandGradient,
    Color? surface2,
    Color? border,
    Color? textSecondary,
    Color? textTertiary,
    Color? success,
    Color? warning,
    double? cardRadius,
    Color? cardBg,
    Color? cardBorder,
    List<BoxShadow>? shadowIcon,
    List<BoxShadow>? shadowGlow,
    List<BoxShadow>? shadowMd,
  }) {
    return AppThemeTokens(
      style: style ?? this.style,
      brand400: brand400 ?? this.brand400,
      brand500: brand500 ?? this.brand500,
      brand600: brand600 ?? this.brand600,
      brand700: brand700 ?? this.brand700,
      accent: accent ?? this.accent,
      brandGradient: brandGradient ?? this.brandGradient,
      surface2: surface2 ?? this.surface2,
      border: border ?? this.border,
      textSecondary: textSecondary ?? this.textSecondary,
      textTertiary: textTertiary ?? this.textTertiary,
      success: success ?? this.success,
      warning: warning ?? this.warning,
      cardRadius: cardRadius ?? this.cardRadius,
      cardBg: cardBg ?? this.cardBg,
      cardBorder: cardBorder ?? this.cardBorder,
      shadowIcon: shadowIcon ?? this.shadowIcon,
      shadowGlow: shadowGlow ?? this.shadowGlow,
      shadowMd: shadowMd ?? this.shadowMd,
    );
  }

  @override
  ThemeExtension<AppThemeTokens> lerp(
    covariant ThemeExtension<AppThemeTokens>? other,
    double t,
  ) {
    if (other is! AppThemeTokens) return this;
    return AppThemeTokens(
      style: t < 0.5 ? style : other.style,
      brand400: Color.lerp(brand400, other.brand400, t)!,
      brand500: Color.lerp(brand500, other.brand500, t)!,
      brand600: Color.lerp(brand600, other.brand600, t)!,
      brand700: Color.lerp(brand700, other.brand700, t)!,
      accent: Color.lerp(accent, other.accent, t)!,
      brandGradient: other.brandGradient,
      surface2: Color.lerp(surface2, other.surface2, t)!,
      border: Color.lerp(border, other.border, t)!,
      textSecondary: Color.lerp(textSecondary, other.textSecondary, t)!,
      textTertiary: Color.lerp(textTertiary, other.textTertiary, t)!,
      success: Color.lerp(success, other.success, t)!,
      warning: Color.lerp(warning, other.warning, t)!,
      cardRadius: cardRadius + (other.cardRadius - cardRadius) * t,
      cardBg: Color.lerp(cardBg, other.cardBg, t)!,
      cardBorder: Color.lerp(cardBorder, other.cardBorder, t)!,
      shadowIcon: t < 0.5 ? shadowIcon : other.shadowIcon,
      shadowGlow: t < 0.5 ? shadowGlow : other.shadowGlow,
      shadowMd: t < 0.5 ? shadowMd : other.shadowMd,
    );
  }
}

class AppTheme {
  // 品牌色板
  static const brand400 = Color(0xff38bdf8);
  static const brand500 = Color(0xff0ea5e9);
  static const brand600 = Color(0xff0284c7);
  static const brand700 = Color(0xff0369a1);
  static const accent = Color(0xff06b6d4);

  // 语义色板
  static const success = Color(0xff10b981);
  static const warning = Color(0xfff59e0b);
  static const error = Color(0xffef4444);

  // 品牌渐变
  static const brandGradient = LinearGradient(
    colors: [brand400, brand600],
    begin: Alignment.topLeft,
    end: Alignment.bottomRight,
  );

  static ThemeData build(AppThemeStyle style, Brightness brightness) {
    final isDark = brightness == Brightness.dark;
    return _buildTheme(isDark);
  }

  static ThemeData _buildTheme(bool isDark) {
    final primary = isDark ? brand400 : brand600;
    final bg = isDark ? const Color(0xff0b1120) : const Color(0xfff8fafc);
    final surface = isDark ? const Color(0xff131c2e) : const Color(0xffffffff);
    final surface2 = isDark ? const Color(0xff16213a) : const Color(0xfff8fafc);
    final border = isDark ? const Color(0xff1e2a44) : const Color(0xffe2e8f0);
    final text = isDark ? const Color(0xfff1f5f9) : const Color(0xff0f172a);
    final text2 = isDark ? const Color(0xff94a3b8) : const Color(0xff64748b);
    final text3 = isDark ? const Color(0xff64748b) : const Color(0xff94a3b8);

    final colorScheme = ColorScheme(
      brightness: isDark ? Brightness.dark : Brightness.light,
      primary: primary,
      onPrimary: Colors.white,
      primaryContainer: isDark ? const Color(0xff0369a1) : const Color(0xffe0f2fe),
      onPrimaryContainer: isDark ? const Color(0xffe0f2fe) : const Color(0xff0369a1),
      secondary: accent,
      onSecondary: Colors.white,
      secondaryContainer: isDark ? const Color(0xff155e75) : const Color(0xffcffafe),
      onSecondaryContainer: isDark ? const Color(0xffcffafe) : const Color(0xff155e75),
      error: error,
      onError: Colors.white,
      surface: surface,
      onSurface: text,
      onSurfaceVariant: text2,
      outline: border,
    );

    final tokens = AppThemeTokens(
      style: AppThemeStyle.chat,
      brand400: brand400,
      brand500: brand500,
      brand600: brand600,
      brand700: brand700,
      accent: accent,
      brandGradient: brandGradient,
      surface2: surface2,
      border: border,
      textSecondary: text2,
      textTertiary: text3,
      success: success,
      warning: warning,
      cardRadius: 14,
      cardBg: surface,
      cardBorder: border,
      shadowIcon: const [
        BoxShadow(
          color: Color(0x4d0ea5e9),
          blurRadius: 12,
          offset: Offset(0, 4),
        ),
      ],
      shadowGlow: const [
        BoxShadow(
          color: Color(0x730ea5e9),
          blurRadius: 30,
          offset: Offset(0, 10),
        ),
      ],
      shadowMd: const [
        BoxShadow(
          color: Color(0x0f0f172a),
          blurRadius: 6,
          offset: Offset(0, 4),
        ),
      ],
    );

    return ThemeData(
      useMaterial3: true,
      colorScheme: colorScheme,
      scaffoldBackgroundColor: bg,
      fontFamily: 'sans-serif',
      fontFamilyFallback: const [
        'Roboto',
        'PingFang SC',
        'Hiragino Sans GB',
        'Microsoft YaHei',
        'Noto Sans CJK SC',
        'Source Han Sans SC',
        'sans-serif',
      ],
      extensions: [tokens],
      dividerTheme: DividerThemeData(
        color: border,
        thickness: 1,
      ),
      chipTheme: ChipThemeData(
        labelStyle: TextStyle(
          fontSize: 12,
          color: text,
          fontWeight: FontWeight.w500,
        ),
        secondaryLabelStyle: TextStyle(
          fontSize: 12,
          color: primary,
          fontWeight: FontWeight.w600,
        ),
        selectedColor: primary.withValues(alpha: isDark ? 0.20 : 0.12),
        backgroundColor: isDark ? surface2 : surface,
        side: BorderSide(color: border),
        shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(999)),
      ),
    );
  }
}
