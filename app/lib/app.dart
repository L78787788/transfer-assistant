import 'package:flutter/material.dart';

import 'core/models.dart';
import 'shell/app_shell.dart';
import 'state/app_controller.dart';
import 'theme/app_theme.dart';

class TransferAssistantApp extends StatefulWidget {
  const TransferAssistantApp({super.key, required this.controller});

  final AppController controller;

  @override
  State<TransferAssistantApp> createState() => _TransferAssistantAppState();
}

class _TransferAssistantAppState extends State<TransferAssistantApp> {
  AppThemeStyle? _cachedStyle;
  ThemeData? _cachedLightTheme;
  ThemeData? _cachedDarkTheme;

  @override
  void initState() {
    super.initState();
    widget.controller.initialize();
  }

  @override
  void dispose() {
    widget.controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.controller,
      builder: (context, _) {
        final style = widget.controller.settings.themeStyle;
        if (_cachedStyle != style || _cachedLightTheme == null) {
          _cachedStyle = style;
          _cachedLightTheme = AppTheme.build(style, Brightness.light);
          _cachedDarkTheme = AppTheme.build(style, Brightness.dark);
        }
        return MaterialApp(
          title: '传输助手',
          debugShowCheckedModeBanner: false,
          themeAnimationDuration: Duration.zero,
          theme: _cachedLightTheme,
          darkTheme: _cachedDarkTheme,
          themeMode: widget.controller.settings.themeMode,
          home: AppShell(controller: widget.controller),
        );
      },
    );
  }
}
