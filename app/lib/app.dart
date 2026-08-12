import 'package:flutter/material.dart';

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
        return MaterialApp(
          title: '传输助手',
          debugShowCheckedModeBanner: false,
          theme: AppTheme.light,
          darkTheme: AppTheme.dark,
          themeMode: widget.controller.settings.themeMode,
          home: AppShell(controller: widget.controller),
        );
      },
    );
  }
}
