import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:transfer_assistant/app.dart';
import 'package:transfer_assistant/core/models.dart';
import 'package:transfer_assistant/core/platform_bridge.dart';
import 'package:transfer_assistant/core/transfer_core_client.dart';
import 'package:transfer_assistant/state/app_controller.dart';

extension on WidgetTester {
  Future<void> settleFast() async {
    await pump();
    await pump(const Duration(milliseconds: 300));
  }
}

void main() {
  tearDown(() async {
    // 确保每次测试后清理定时器和资源
  });
  testWidgets('mobile shell lists peers and navigates to transfers', (
    tester,
  ) async {
    final core = _FakeCore();
    final controller = AppController(
      core: core,
      platform: const _FakePlatform(),
    );
    await tester.pumpWidget(TransferAssistantApp(controller: controller));
    await tester.settleFast();

    expect(find.text('附近设备'), findsWidgets);
    expect(find.text('书房电脑'), findsOneWidget);
    expect(find.byIcon(LucideIcons.arrowUpDown), findsWidgets);

    await tester.tap(find.text('传输中').last);
    await tester.settleFast();

    expect(find.text('当前没有正在进行的传输'), findsOneWidget);

    await tester.tap(find.text('历史记录').last);
    await tester.settleFast();

    expect(find.text('传输历史'), findsOneWidget);
    expect(find.text('年度总结报告.pdf'), findsOneWidget);
    expect(find.text('度假合影.jpg'), findsOneWidget);
  });

  testWidgets(
    'incoming transfer triggers notice banner and auto-navigates to transfers',
    (tester) async {
      final core = _FakeCore();
      final controller = AppController(
        core: core,
        platform: const _FakePlatform(),
      );
      await tester.pumpWidget(TransferAssistantApp(controller: controller));
      await tester.settleFast();

      expect(controller.selectedPage, 0);

      // 模拟收到信任设备的自动接收任务
      core.controller.add(
        TransfersChanged([
          TransferSnapshot(
            id: 't-1',
            peerName: '书房电脑',
            direction: TransferDirection.incoming,
            state: TransferState.transferring,
            itemCount: 3,
            totalBytes: 1048576,
            completedBytes: 524288,
            bytesPerSecond: 102400,
            updatedAt: DateTime.now(),
          ),
        ]),
      );
      await tester.settleFast();

      // 验证自动跳转到传输任务页（page index 1）
      expect(controller.selectedPage, 1);
      // 验证顶部展示 NoticeBanner
      expect(find.textContaining('收到来自「书房电脑」的文件'), findsOneWidget);
      // 验证任务列表中展示了该任务
      expect(find.textContaining('书房电脑'), findsWidgets);

      // 模拟任务完成
      core.controller.add(
        TransfersChanged([
          TransferSnapshot(
            id: 't-1',
            peerName: '书房电脑',
            direction: TransferDirection.incoming,
            state: TransferState.completed,
            itemCount: 3,
            totalBytes: 1048576,
            completedBytes: 1048576,
            bytesPerSecond: 0,
            updatedAt: DateTime.now(),
          ),
        ]),
      );
      await tester.settleFast();
      expect(find.textContaining('已完成来自「书房电脑」的文件接收'), findsOneWidget);
      controller.clearNotice();
    },
  );

  testWidgets('history view searches files and switches view modes', (
    tester,
  ) async {
    final core = _FakeCore();
    final controller = AppController(
      core: core,
      platform: const _FakePlatform(),
    );
    await tester.pumpWidget(TransferAssistantApp(controller: controller));
    await tester.settleFast();

    // 切换到历史记录
    await tester.tap(find.text('历史记录').last);
    await tester.settleFast();

    expect(find.text('年度总结报告.pdf'), findsOneWidget);
    expect(find.text('度假合影.jpg'), findsOneWidget);

    // 搜索过滤
    await tester.enterText(find.byType(TextField), '报告');
    await tester.settleFast();

    expect(find.text('年度总结报告.pdf'), findsOneWidget);
    expect(find.text('度假合影.jpg'), findsNothing);

    // 清除搜索
    await tester.tap(find.byIcon(LucideIcons.x));
    await tester.settleFast();

    expect(find.text('年度总结报告.pdf'), findsOneWidget);
    expect(find.text('度假合影.jpg'), findsOneWidget);

    // 测试清空历史
    await tester.tap(find.byIcon(LucideIcons.trash2));
    await tester.settleFast();
    expect(find.text('清空历史记录'), findsOneWidget);

    await tester.tap(find.text('确认清空'));
    await tester.settleFast();
    expect(core.clearedHistory, true);
    controller.clearNotice();
  });

  testWidgets('text transfer dialog and ETA calculation work correctly', (
    tester,
  ) async {
    final core = _FakeCore();
    final controller = AppController(
      core: core,
      platform: const _FakePlatform(),
    );
    await tester.pumpWidget(TransferAssistantApp(controller: controller));
    await tester.settleFast();

    // 点击设备卡片进入专属会话
    await tester.tap(find.text('书房电脑'));
    await tester.settleFast();

    // 验证会话输入栏与操作按钮展现
    expect(find.text('输入便签文字直接发送...'), findsOneWidget);
    await tester.enterText(find.byType(TextField), '测试剪贴板文本');
    await tester.settleFast();

    // 点击返回回到设备列表
    await tester.tap(find.byIcon(LucideIcons.arrowLeft));
    await tester.settleFast();

    // 验证 ETA 计算
    final snap = TransferSnapshot(
      id: 't-eta',
      peerName: '测试设备',
      direction: TransferDirection.outgoing,
      state: TransferState.transferring,
      itemCount: 1,
      totalBytes: 10485760, // 10MB
      completedBytes: 5242880, // 5MB
      bytesPerSecond: 1048576, // 1MB/s -> 剩余 5s
      updatedAt: DateTime.now(),
    );
    expect(snap.etaText, '约 5 秒');

    controller.clearNotice();
    await tester.settleFast();
  });

  testWidgets('chat theme shows device list and opens chat session', (
    tester,
  ) async {
    final core = _FakeCore();
    final controller = AppController(
      core: core,
      platform: const _FakePlatform(),
    );
    await tester.pumpWidget(TransferAssistantApp(controller: controller));
    await tester.settleFast();

    // 切换至流式传输会话流主题
    controller.updateSettings(
      controller.settings.copyWith(themeStyle: AppThemeStyle.chat),
    );
    await tester.settleFast();

    // 验证一级页面：显示传输会话和设备列表
    expect(find.text('传输会话'), findsOneWidget);
    expect(find.text('书房电脑'), findsOneWidget);

    // 点击设备卡片进入专属对话窗口
    await tester.tap(find.text('书房电脑'));
    await tester.settleFast();

    // 验证进入了二级对话窗口
    expect(find.text('输入便签文字直接发送...'), findsOneWidget);
    expect(find.byIcon(LucideIcons.arrowLeft), findsOneWidget);

    // 点击返回按钮回到设备列表
    await tester.tap(find.byIcon(LucideIcons.arrowLeft));
    await tester.settleFast();
    expect(find.text('传输会话'), findsOneWidget);
  });

  testWidgets(
    'first launch shows device naming overlay and dismisses on submit',
    (tester) async {
      final core = _FakeCore()..forceFirstSetup = false;
      final controller = AppController(
        core: core,
        platform: const _FakePlatform(),
        initialSettings: const AppSettings(
          deviceName: '我的新设备',
          receiveDirectory: 'C:/test/downloads',
          hasCompletedFirstSetup: false,
        ),
      );

      await tester.pumpWidget(TransferAssistantApp(controller: controller));
      await tester.settleFast();

      expect(find.text('欢迎使用互传'), findsOneWidget);
      expect(find.text('完成并开启极速传输'), findsOneWidget);

      await tester.tap(find.text('完成并开启极速传输'));
      await tester.settleFast();

      expect(controller.settings.hasCompletedFirstSetup, isTrue);
      expect(find.text('欢迎使用互传'), findsNothing);
    },
  );
}

class _FakePlatform extends PlatformBridge {
  const _FakePlatform();

  @override
  Future<PlatformPaths> paths() async => PlatformPaths(
    dataDirectory: '${Directory.systemTemp.path}/test_data',
    receiveDirectory: '${Directory.systemTemp.path}/test_downloads',
  );

  @override
  Future<SharedPayload?> getSharedPayload() async => null;

  @override
  Future<void> clearSharedPayload() async {}
}

class _FakeCore implements TransferCoreClient {
  final controller = StreamController<CoreEvent>.broadcast();
  var clearedHistory = false;
  bool? forceFirstSetup;

  @override
  Stream<CoreEvent> get events => controller.stream;

  @override
  Future<void> initialize({
    required AppSettings settings,
    required String dataDirectory,
    String? identityWrapKey,
    String? logDirectory,
  }) async {
    controller.add(
      CoreSettingsLoaded(
        settings.copyWith(hasCompletedFirstSetup: forceFirstSetup ?? true),
      ),
    );
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
  Future<List<HistoryFileItem>> listHistoryFiles() async => clearedHistory
      ? const []
      : [
          HistoryFileItem(
            id: 'item-1',
            transferId: 't-1',
            fileName: '年度总结报告.pdf',
            relativePath: 'docs/年度总结报告.pdf',
            localPath: 'C:/test/downloads/docs/年度总结报告.pdf',
            isDirectory: false,
            size: 2048576,
            peerName: '书房电脑',
            direction: TransferDirection.incoming,
            completedAt: DateTime.now(),
          ),
          HistoryFileItem(
            id: 'item-2',
            transferId: 't-2',
            fileName: '度假合影.jpg',
            relativePath: 'photos/度假合影.jpg',
            localPath: 'C:/test/downloads/photos/度假合影.jpg',
            isDirectory: false,
            size: 5048576,
            peerName: '客厅手机',
            direction: TransferDirection.outgoing,
            completedAt: DateTime.now(),
          ),
        ];

  @override
  Future<List<TransferItem>> listTransferItems(String transferId) async =>
      const [];

  @override
  Future<int> clearHistory() async {
    clearedHistory = true;
    return 2;
  }

  @override
  Future<bool> deleteHistoryTransfer(String transferId) async => true;

  @override
  Future<void> shutdown() => controller.close();

  @override
  Future<void> updateSettings(AppSettings settings) async {}

  @override
  Future<void> removeTrustedPeer(String peerId) async {}

  @override
  Future<List<TrustedPeerInfo>> listTrustedPeers() async => const [];

  @override
  Future<int> clearTrustedPeers() async => 0;
}
