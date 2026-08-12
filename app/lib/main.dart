import 'package:flutter/material.dart';

import 'app.dart';
import 'core/native_transfer_core_client.dart';
import 'core/platform_bridge.dart';
import 'state/app_controller.dart';

void main() {
  WidgetsFlutterBinding.ensureInitialized();
  final controller = AppController(
    core: NativeTransferCoreClient.create(),
    platform: const PlatformBridge(),
  );
  runApp(TransferAssistantApp(controller: controller));
}
