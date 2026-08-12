import 'package:flutter/material.dart';

abstract final class AppTheme {
  static const _lightScheme = ColorScheme(
    brightness: Brightness.light,
    primary: Color(0xff087f5b),
    onPrimary: Colors.white,
    primaryContainer: Color(0xffd3f9d8),
    onPrimaryContainer: Color(0xff0b3d2e),
    secondary: Color(0xffb86200),
    onSecondary: Colors.white,
    secondaryContainer: Color(0xffffe8cc),
    onSecondaryContainer: Color(0xff512b00),
    error: Color(0xffc92a2a),
    onError: Colors.white,
    errorContainer: Color(0xffffe3e3),
    onErrorContainer: Color(0xff6b1111),
    surface: Color(0xfffbfcfa),
    onSurface: Color(0xff202522),
    surfaceContainerHighest: Color(0xffe9ecea),
    onSurfaceVariant: Color(0xff58615c),
    outline: Color(0xffabb3ae),
    outlineVariant: Color(0xffd8ddda),
    shadow: Color(0x22000000),
    scrim: Color(0x66000000),
    inverseSurface: Color(0xff2d332f),
    onInverseSurface: Color(0xfff2f5f3),
    inversePrimary: Color(0xff63e6be),
  );

  static const _darkScheme = ColorScheme(
    brightness: Brightness.dark,
    primary: Color(0xff63e6be),
    onPrimary: Color(0xff073c2e),
    primaryContainer: Color(0xff145c48),
    onPrimaryContainer: Color(0xffd3f9d8),
    secondary: Color(0xffffb86b),
    onSecondary: Color(0xff4d2900),
    secondaryContainer: Color(0xff6b3d0d),
    onSecondaryContainer: Color(0xffffe8cc),
    error: Color(0xffff8787),
    onError: Color(0xff5f1010),
    errorContainer: Color(0xff812020),
    onErrorContainer: Color(0xffffe3e3),
    surface: Color(0xff181c1a),
    onSurface: Color(0xffe8ece9),
    surfaceContainerHighest: Color(0xff323834),
    onSurfaceVariant: Color(0xffb8c0bb),
    outline: Color(0xff77817b),
    outlineVariant: Color(0xff3e4641),
    shadow: Colors.black,
    scrim: Color(0x99000000),
    inverseSurface: Color(0xffe8ece9),
    onInverseSurface: Color(0xff232825),
    inversePrimary: Color(0xff087f5b),
  );

  static ThemeData get light => _build(_lightScheme);
  static ThemeData get dark => _build(_darkScheme);

  static ThemeData _build(ColorScheme scheme) {
    final text = Typography.material2021(
      platform: TargetPlatform.windows,
    ).black;
    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: scheme.surface,
      textTheme: text
          .copyWith(
            headlineSmall: text.headlineSmall?.copyWith(
              fontWeight: FontWeight.w700,
            ),
            titleLarge: text.titleLarge?.copyWith(fontWeight: FontWeight.w700),
            titleMedium: text.titleMedium?.copyWith(
              fontWeight: FontWeight.w600,
            ),
          )
          .apply(bodyColor: scheme.onSurface, displayColor: scheme.onSurface),
      cardTheme: CardThemeData(
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(color: scheme.outlineVariant),
        ),
      ),
      inputDecorationTheme: InputDecorationTheme(
        filled: true,
        fillColor: scheme.surface,
        border: OutlineInputBorder(borderRadius: BorderRadius.circular(6)),
        enabledBorder: OutlineInputBorder(
          borderRadius: BorderRadius.circular(6),
          borderSide: BorderSide(color: scheme.outlineVariant),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: ButtonStyle(
          shape: WidgetStatePropertyAll(
            RoundedRectangleBorder(borderRadius: BorderRadius.circular(6)),
          ),
        ),
      ),
      tooltipTheme: const TooltipThemeData(
        waitDuration: Duration(milliseconds: 350),
      ),
      dividerTheme: DividerThemeData(color: scheme.outlineVariant, space: 1),
    );
  }
}
