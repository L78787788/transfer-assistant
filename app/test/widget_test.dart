import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:transfer_assistant/app.dart';
import 'package:transfer_assistant/core/models.dart';
import 'package:transfer_assistant/core/platform_bridge.dart';
import 'package:transfer_assistant/core/transfer_core_client.dart';
import 'package:transfer_assistant/state/app_controller.dart';

void main() {
  testWidgets('mobile shell lists peers and navigates to transfers', (
    tester,
  ) async {
    final core = _FakeCore();
    final controller = AppController(
      core: core,
      platform: const _FakePlatform(),
    );
    await tester.pumpWidget(TransferAssistantApp(controller: controller));
    await tester.pumpAndSettle();

    expect(find.text('附近设备'), findsWidgets);
    expect(find.text('书房电脑'), findsOneWidget);
    expect(find.byType(NavigationBar), findsOneWidget);

    await tester.tap(find.text('传输任务').last);
    await tester.pumpAndSettle();

    expect(find.text('没有进行中的任务'), findsOneWidget);
  });
}

class _FakePlatform extends PlatformBridge {
  const _FakePlatform();

  @override
  Future<PlatformPaths> paths() async => const PlatformPaths(
    dataDirectory: 'C:/test/data',
    receiveDirectory: 'C:/test/downloads',
  );
}

class _FakeCore implements TransferCoreClient {
  final controller = StreamController<CoreEvent>.broadcast();

  @override
  Stream<CoreEvent> get events => controller.stream;

  @override
  Future<void> initialize({
    required AppSettings settings,
    required String dataDirectory,
    String? identityWrapKey,
    String? logDirectory,
  }) async {
    controller.add(const CoreReady());
    controller.add(
      const PeersChanged([
        PeerSummary(
          id: 'peer-1',
          name: '书房电脑',
          address: '192.168.1.8:53317',
          deviceKind: DeviceKind.computer,
          trusted: true,
        ),
      ]),
    );
  }

  @override
  Future<void> answerOffer(
    String offerId, {
    required bool accept,
    required bool rememberPeer,
  }) async {}

  @override
  Future<void> commandTransfer(
    String transferId,
    TransferCommand command,
  ) async {}

  @override
  Future<void> connectAddress(String address) async {}

  @override
  Future<void> refreshPeers() async {}

  @override
  Future<String> send(String peerId, List<SourceHandle> sources) async =>
      'transfer-1';

  @override
  Future<void> shutdown() => controller.close();

  @override
  Future<void> updateSettings(AppSettings settings) async {}

  @override
  Future<void> removeTrustedPeer(String peerId) async {}
}
