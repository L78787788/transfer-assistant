import 'dart:async';

import 'package:flutter/material.dart';

import '../core/models.dart';
import '../core/platform_bridge.dart';
import '../core/transfer_core_client.dart';

class AppController extends ChangeNotifier {
  AppController({required this.core, required this.platform});

  final TransferCoreClient core;
  final PlatformBridge platform;
  StreamSubscription<CoreEvent>? _subscription;

  var selectedPage = 0;
  var isInitializing = true;
  var isRefreshing = false;
  String? errorMessage;
  List<PeerSummary> peers = const [];
  List<TransferSnapshot> transfers = const [];
  TransferOffer? pendingOffer;
  AppSettings settings = const AppSettings(
    deviceName: '这台设备',
    receiveDirectory: '',
  );

  Future<void> initialize() async {
    _subscription = core.events.listen(_handleEvent);
    try {
      final paths = await platform.paths();
      settings = settings.copyWith(receiveDirectory: paths.receiveDirectory);
      await core.initialize(
        settings: settings,
        dataDirectory: paths.dataDirectory,
        identityWrapKey: paths.identityWrapKey,
        logDirectory: paths.logDirectory,
      );
    } catch (error) {
      errorMessage = '初始化失败：$error';
      isInitializing = false;
      notifyListeners();
    }
  }

  void selectPage(int index) {
    if (selectedPage == index) return;
    selectedPage = index;
    notifyListeners();
  }

  Future<void> refreshPeers() async {
    if (isRefreshing) return;
    isRefreshing = true;
    notifyListeners();
    try {
      await core.refreshPeers();
    } catch (error) {
      errorMessage = '刷新设备失败：$error';
    } finally {
      isRefreshing = false;
      notifyListeners();
    }
  }

  Future<void> connectAddress(String address) async {
    try {
      await core.connectAddress(address.trim());
    } catch (error) {
      errorMessage = '连接失败：$error';
      notifyListeners();
      rethrow;
    }
  }

  Future<void> sendToPeer(PeerSummary peer, {required bool directory}) async {
    try {
      final sources = await platform.pickSources(directory: directory);
      if (sources.isEmpty) return;
      await core.send(peer.id, sources);
      selectedPage = 1;
      notifyListeners();
    } catch (error) {
      errorMessage = '创建传输失败：$error';
      notifyListeners();
    }
  }

  Future<void> commandTransfer(
    TransferSnapshot transfer,
    TransferCommand command,
  ) async {
    try {
      await core.commandTransfer(transfer.id, command);
    } catch (error) {
      errorMessage = '任务操作失败：$error';
      notifyListeners();
    }
  }

  Future<void> answerOffer({
    required bool accept,
    required bool rememberPeer,
  }) async {
    final offer = pendingOffer;
    if (offer == null) return;
    pendingOffer = null;
    notifyListeners();
    await core.answerOffer(
      offer.id,
      accept: accept,
      rememberPeer: rememberPeer,
    );
  }

  Future<void> updateSettings(AppSettings next) async {
    final previous = settings;
    settings = next;
    notifyListeners();
    try {
      if (previous.backgroundReceive != next.backgroundReceive) {
        await platform.setBackgroundReceive(next.backgroundReceive);
      }
      await core.updateSettings(next);
    } catch (error) {
      settings = previous;
      errorMessage = '保存设置失败：$error';
      notifyListeners();
    }
  }

  Future<void> chooseReceiveDirectory() async {
    final selected = await platform.chooseReceiveDirectory();
    if (selected != null) {
      await updateSettings(settings.copyWith(receiveDirectory: selected));
    }
  }

  Future<void> removeTrustedPeer(String peerId) async {
    try {
      await core.removeTrustedPeer(peerId);
    } catch (error) {
      errorMessage = '移除可信设备失败：$error';
      notifyListeners();
    }
  }

  void clearError() {
    errorMessage = null;
    notifyListeners();
  }

  void _handleEvent(CoreEvent event) {
    switch (event) {
      case CoreReady():
        isInitializing = false;
      case CoreSettingsLoaded(:final settings):
        this.settings = settings;
        unawaited(platform.setBackgroundReceive(settings.backgroundReceive));
      case PeersChanged(:final peers):
        this.peers = peers;
        isRefreshing = false;
      case TransfersChanged(:final transfers):
        this.transfers = transfers;
        unawaited(
          platform.setTransferActive(
            transfers.any((transfer) => transfer.isActive),
          ),
        );
      case IncomingOffer(:final offer):
        pendingOffer = offer;
      case CoreFailure(:final message):
        errorMessage = message;
        isInitializing = false;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _subscription?.cancel();
    unawaited(core.shutdown());
    super.dispose();
  }
}
