import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';

import '../core/models.dart';
import '../core/platform_bridge.dart';
import '../core/transfer_core_client.dart';
import '../widgets/common.dart';

class AppController extends ChangeNotifier {
  AppController({
    required this.core,
    required this.platform,
    AppSettings? initialSettings,
  }) {
    if (initialSettings != null) {
      settings = initialSettings;
    }
  }

  final TransferCoreClient core;
  final PlatformBridge platform;
  StreamSubscription<CoreEvent>? _subscription;

  var selectedPage = 0;
  var isInitializing = true;
  var isRefreshing = false;
  String? errorMessage;
  String? noticeMessage;
  Timer? _noticeTimer;
  final Set<String> _knownTransferIds = <String>{};
  final Map<String, TransferState> _knownTransferStates = <String, TransferState>{};

  List<PeerSummary> peers = const [];
  List<TransferSnapshot> transfers = const [];
  List<TransferSnapshot> get activeTransfers =>
      transfers.where((t) => t.isActive).toList(growable: false);
  String get selfPeerName =>
      settings.deviceName.isNotEmpty ? settings.deviceName : '这台设备';
  List<HistoryFileItem> historyFiles = const [];
  var isLoadingHistory = false;
  TransferOffer? pendingOffer;
  String? sharedTextPayload;
  List<SourceHandle>? sharedFileSources;
  String? dataDirectoryPath;
  AppSettings settings = const AppSettings(
    deviceName: '这台设备',
    receiveDirectory: '',
  );

  Future<void> initialize() async {
    _subscription = core.events.listen(_handleEvent);
    try {
      final paths = await platform.paths();
      dataDirectoryPath = paths.dataDirectory;
      settings = settings.copyWith(receiveDirectory: paths.receiveDirectory);
      await core.initialize(
        settings: settings,
        dataDirectory: paths.dataDirectory,
        identityWrapKey: paths.identityWrapKey,
        logDirectory: paths.logDirectory,
      );
      platform.setFilesDroppedHandler((droppedFiles) {
        if (droppedFiles.isNotEmpty) {
          sharedFileSources = droppedFiles;
          sharedTextPayload = null;
          selectedPage = 0;
          showNotice('已通过拖拽载入 ${droppedFiles.length} 项文件，请点击目标设备发送');
          notifyListeners();
        }
      });
      unawaited(checkSharedPayload());
    } catch (error) {
      errorMessage = '初始化失败：$error';
      isInitializing = false;
      notifyListeners();
    }
  }

  Future<void> checkSharedPayload() async {
    final payload = await platform.getSharedPayload();
    if (payload == null) return;
    await platform.clearSharedPayload();

    if (payload.type == 'text' && payload.text != null) {
      sharedTextPayload = payload.text;
      selectedPage = 0; // 切换到附近设备页
      showNotice('已准备好分享的文本，请选择发送目标设备');
      notifyListeners();
    } else if (payload.paths.isNotEmpty) {
      final sources = <SourceHandle>[];
      for (final p in payload.paths) {
        final name = p.split('/').last;
        sources.add(SourceHandle(token: p, displayName: name.isNotEmpty ? name : '分享文件'));
      }
      sharedFileSources = sources;
      selectedPage = 0; // 切换到附近设备页
      showNotice('已加载 ${sources.length} 项分享文件，请选择接收设备');
      notifyListeners();
    }
  }

  void clearSharedDraft() {
    sharedTextPayload = null;
    sharedFileSources = null;
    notifyListeners();
  }

  void stageSharedText(String text) {
    sharedTextPayload = text;
    sharedFileSources = null;
    notifyListeners();
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

  Future<void> sendSourcesToPeer(PeerSummary peer, List<SourceHandle> sources) async {
    try {
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
        unawaited(platform.setBackgroundReceive(next.backgroundReceive));
      }
      unawaited(core.updateSettings(next).catchError((error) {
        settings = previous;
        errorMessage = '保存设置失败：$error';
        notifyListeners();
      }));
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

  Future<void> loadHistoryFiles() async {
    isLoadingHistory = true;
    notifyListeners();
    try {
      historyFiles = await core.listHistoryFiles();
    } catch (_) {
      // 保持现有列表
    } finally {
      isLoadingHistory = false;
      notifyListeners();
    }
  }

  Future<List<TransferItem>> loadTransferItems(String transferId) async {
    try {
      return await core.listTransferItems(transferId);
    } catch (_) {
      return const [];
    }
  }

  Future<void> openHistoryFile(HistoryFileItem item) async {
    final path = item.localPath;
    if (path != null && path.isNotEmpty) {
      if (Platform.isAndroid || Platform.isIOS) {
        showNotice(
          '正在打开「${item.fileName}」...',
          duration: const Duration(seconds: 2),
        );
      }
      await platform.openFile(path);
    } else {
      errorMessage = '未找到该文件的本地路径';
      notifyListeners();
    }
  }

  Future<void> locateHistoryFile(HistoryFileItem item) async {
    final path = item.localPath;
    if (path != null && path.isNotEmpty) {
      await platform.openDirectory(
        settings.receiveDirectory,
        selectFile: path,
      );
    } else {
      await platform.openDirectory(settings.receiveDirectory);
    }
    if (Platform.isAndroid || Platform.isIOS) {
      showNotice(
        '正在打开文件管理...',
        duration: const Duration(seconds: 2),
      );
    }
  }

  Future<void> installHistoryFile(HistoryFileItem item) async {
    final target = item.localPath ?? item.relativePath;
    await platform.installApk(target);
  }

  Future<void> shareHistoryFile(HistoryFileItem item) async {
    final target = item.localPath ?? item.relativePath;
    await platform.shareFile(target);
  }

  Future<void> sendTextMessage(String peerId, String text) async {
    final trimmed = text.trim();
    if (trimmed.isEmpty) return;
    try {
      final now = DateTime.now();
      final stamp =
          '${now.hour.toString().padLeft(2, '0')}${now.minute.toString().padLeft(2, '0')}${now.second.toString().padLeft(2, '0')}';
      final file = File('${Directory.systemTemp.path}/text_note_$stamp.txt');
      await file.writeAsString(trimmed);

      await core.send(peerId, [
        SourceHandle(token: file.path, displayName: '文本便签_$stamp.txt'),
      ]);
      showNotice('已发起文本便签发送');
      selectedPage = 1; // 切换到传输中
      notifyListeners();
    } catch (error) {
      errorMessage = '发送文本失败：$error';
      notifyListeners();
    }
  }

  Future<void> clearHistory() async {
    try {
      await core.clearHistory();
      await loadHistoryFiles();
      showNotice('已清空所有传输历史');
    } catch (error) {
      errorMessage = '清空历史失败：$error';
      notifyListeners();
    }
  }

  Future<void> deleteHistoryTransfer(String transferId) async {
    try {
      await core.deleteHistoryTransfer(transferId);
      await loadHistoryFiles();
    } catch (error) {
      errorMessage = '删除记录失败：$error';
      notifyListeners();
    }
  }

  void clearError() {
    errorMessage = null;
    notifyListeners();
  }

  void showNotice(
    String message, {
    Duration duration = const Duration(seconds: 3),
  }) {
    _noticeTimer?.cancel();
    noticeMessage = message;
    notifyListeners();
    _noticeTimer = Timer(duration, () {
      if (noticeMessage == message) {
        noticeMessage = null;
        notifyListeners();
      }
    });
  }

  void clearNotice() {
    _noticeTimer?.cancel();
    noticeMessage = null;
    notifyListeners();
  }

  void _handleEvent(CoreEvent event) {
    switch (event) {
      case CoreReady():
        isInitializing = false;
        unawaited(loadHistoryFiles());
      case CoreSettingsLoaded(:final settings):
        this.settings = settings;
        unawaited(platform.setBackgroundReceive(settings.backgroundReceive));
      case PeersChanged(:final peers):
        this.peers = peers;
        isRefreshing = false;
      case TransfersChanged(:final transfers):
        final previousIds = Set<String>.from(_knownTransferIds);
        final previousStates =
            Map<String, TransferState>.from(_knownTransferStates);
        var anyCompleted = false;

        this.transfers = transfers;
        _knownTransferIds
          ..clear()
          ..addAll(transfers.map((t) => t.id));
        _knownTransferStates
          ..clear()
          ..addEntries(transfers.map((t) => MapEntry(t.id, t.state)));

        // 检查新接收的传输任务（例如受信任设备自动接收）
        for (final transfer in transfers) {
          final isCompletedNow =
              previousStates[transfer.id] != null &&
              previousStates[transfer.id] != TransferState.completed &&
              transfer.state == TransferState.completed;
          if (isCompletedNow) {
            anyCompleted = true;
          }

          if (transfer.direction == TransferDirection.incoming) {
            final isNew = !previousIds.contains(transfer.id);
            if (isNew && transfer.isActive) {
              // 自动切换到传输页面，弹出接收提示
              if (selectedPage == 0) {
                selectedPage = 1;
              }
              final msg =
                  '收到来自「${transfer.peerName}」的文件（${transfer.itemCount} 项 · ${formatBytes(transfer.totalBytes)}）';
              showNotice(msg);
              unawaited(
                platform.showNotification(
                  title: '正在接收文件',
                  body: '来自「${transfer.peerName}」的 ${transfer.itemCount} 项文件',
                ),
              );
            } else if (isCompletedNow) {
              final msg =
                  '已完成来自「${transfer.peerName}」的文件接收（${transfer.itemCount} 项 · ${formatBytes(transfer.totalBytes)}）';
              showNotice(msg);
              unawaited(
                platform.showNotification(
                  title: '文件接收完成',
                  body: '已接收来自「${transfer.peerName}」的 ${transfer.itemCount} 项文件',
                ),
              );
            }
          }

          // 检查发送端或接收端失败通知
          final isFailedNow =
              previousStates[transfer.id] != null &&
              previousStates[transfer.id] != TransferState.failed &&
              transfer.state == TransferState.failed;
          if (isFailedNow) {
            final errorText = transfer.error ?? '网络中断或对端取消';
            unawaited(
              platform.showNotification(
                title: '传输失败',
                body: '与「${transfer.peerName}」的传输未完成：$errorText',
              ),
            );
          }
        }

        if (anyCompleted) {
          unawaited(loadHistoryFiles());
        }

        final activeList = transfers.where((t) => t.isActive).toList(growable: false);
        if (activeList.isNotEmpty) {
          final primary = activeList.first;
          final percent = primary.totalBytes > 0
              ? ((primary.completedBytes / primary.totalBytes) * 100).toInt().clamp(0, 100)
              : 0;
          final speedStr = '${formatBytes(primary.bytesPerSecond)}/s';
          unawaited(
            platform.updateNotificationProgress(
              title: '${primary.peerName} (${primary.itemCount}项)',
              speed: speedStr,
              percent: percent,
              active: true,
            ),
          );
        } else {
          unawaited(
            platform.updateNotificationProgress(
              title: '传输助手',
              speed: '',
              percent: 0,
              active: false,
            ),
          );
        }

        unawaited(
          platform.setTransferActive(
            activeList.isNotEmpty,
          ),
        );
      case IncomingOffer(:final offer):
        pendingOffer = offer;
        unawaited(
          platform.showNotification(
            title: '收到传输请求',
            body: '「${offer.peerName}」请求向您发送 ${offer.itemCount} 项文件 (${formatBytes(offer.totalBytes)})',
          ),
        );
      case CoreFailure(:final message):
        errorMessage = message;
        isInitializing = false;
    }
    notifyListeners();
  }

  @override
  void dispose() {
    _noticeTimer?.cancel();
    _subscription?.cancel();
    unawaited(core.shutdown());
    super.dispose();
  }
}
