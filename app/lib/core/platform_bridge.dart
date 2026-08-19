import 'dart:io';

import 'package:flutter/services.dart';

import 'models.dart';

class PlatformPaths {
  const PlatformPaths({
    required this.dataDirectory,
    required this.receiveDirectory,
    this.identityWrapKey,
    this.logDirectory,
  });

  final String dataDirectory;
  final String receiveDirectory;
  final String? identityWrapKey;
  final String? logDirectory;
}

class PlatformBridge {
  const PlatformBridge();

  static const _channel = MethodChannel('transassist/platform');

  Future<PlatformPaths> paths() async {
    try {
      final result = await _channel.invokeMapMethod<String, String>('paths');
      if (result != null) {
        return PlatformPaths(
          dataDirectory: result['dataDirectory']!,
          receiveDirectory: result['receiveDirectory']!,
          identityWrapKey: result['identityWrapKey'],
          logDirectory: result['logDirectory'],
        );
      }
    } on MissingPluginException {
      // Unit tests and an unpackaged Windows debug run use the local fallback.
    }
    final home = Platform.environment['USERPROFILE'] ?? Directory.current.path;
    final local = Platform.environment['LOCALAPPDATA'] ?? home;
    return PlatformPaths(
      dataDirectory: '$local${Platform.pathSeparator}传输助手',
      receiveDirectory:
          '$home${Platform.pathSeparator}Downloads${Platform.pathSeparator}传输助手',
    );
  }

  Future<List<SourceHandle>> pickSources({required bool directory}) async {
    final result = await _channel.invokeListMethod<Map<Object?, Object?>>(
      directory ? 'pickDirectory' : 'pickFiles',
    );
    return (result ?? const [])
        .map(
          (item) => SourceHandle(
            token: item['token']! as String,
            displayName: item['displayName']! as String,
            persistentToken: item['persistentToken'] as String?,
            relativePath: item['relativePath'] as String?,
            isDirectory: item['isDirectory'] == true,
            size: (item['size'] as num?)?.toInt(),
            modifiedUnixMs: (item['modifiedUnixMs'] as num?)?.toInt(),
            randomAccess: item['randomAccess'] as bool?,
          ),
        )
        .toList(growable: false);
  }

  Future<String?> chooseReceiveDirectory() =>
      _channel.invokeMethod<String>('chooseReceiveDirectory');

  Future<void> setBackgroundReceive(bool enabled) =>
      _channel.invokeMethod<void>('setBackgroundReceive', enabled);

  Future<void> setTransferActive(bool active) =>
      _channel.invokeMethod<void>('setTransferActive', active);

  Future<void> showNotification({
    required String title,
    required String body,
  }) async {
    try {
      await _channel.invokeMethod<void>('showNotification', {
        'title': title,
        'body': body,
      });
    } on MissingPluginException {
      // Ignored on platforms without native implementation.
    } catch (_) {
      // Best-effort notification.
    }
  }

  Future<void> openFile(String path) async {
    if (Platform.isWindows) {
      final file = File(path);
      if (await file.exists()) {
        await Process.run('cmd', ['/c', 'start', '', path], runInShell: true);
        return;
      }
    }
    try {
      await _channel.invokeMethod<void>('openFile', {'path': path});
    } on MissingPluginException {
      // Ignored on platforms without native implementation.
    } catch (_) {
      // Best-effort
    }
  }

  Future<void> openDirectory(String path, {String? selectFile}) async {
    if (Platform.isWindows) {
      if (selectFile != null && File(selectFile).existsSync()) {
        await Process.run(
          'explorer.exe',
          ['/select,', selectFile],
          runInShell: true,
        );
        return;
      }
      if (Directory(path).existsSync()) {
        await Process.run('explorer.exe', [path], runInShell: true);
        return;
      }
    }
    try {
      await _channel.invokeMethod<void>('openDirectory', {
        'path': path,
        'selectFile': ?selectFile,
      });
    } on MissingPluginException {
      // Ignored on platforms without native implementation.
    } catch (_) {
      // Best-effort
    }
  }

  Future<SharedPayload?> getSharedPayload() async {
    try {
      final res = await _channel.invokeMapMethod<String, Object?>('getSharedPayload');
      if (res != null) {
        return SharedPayload.fromJson(res);
      }
    } on MissingPluginException {
      // Ignored
    } catch (_) {}
    return null;
  }

  Future<void> clearSharedPayload() async {
    try {
      await _channel.invokeMethod<void>('clearSharedPayload');
    } on MissingPluginException {
      // Ignored
    } catch (_) {}
  }

  Future<void> updateNotificationProgress({
    required String title,
    required String speed,
    required int percent,
    required bool active,
  }) async {
    try {
      await _channel.invokeMethod<void>('updateNotificationProgress', {
        'title': title,
        'speed': speed,
        'percent': percent,
        'active': active,
      });
    } catch (_) {}
  }

  Future<void> setContextMenuEnabled(bool enabled) async {
    try {
      await _channel.invokeMethod<bool>('setContextMenuEnabled', enabled);
    } catch (_) {}
  }

  Future<bool> isContextMenuEnabled() async {
    try {
      final res = await _channel.invokeMethod<bool>('isContextMenuEnabled');
      return res ?? false;
    } catch (_) {
      return false;
    }
  }

  Future<void> installApk(String path) async {
    try {
      await _channel.invokeMethod<void>('installApk', {'path': path});
    } catch (_) {}
  }

  Future<void> shareFile(String path) async {
    try {
      await _channel.invokeMethod<void>('shareFile', {'path': path});
    } catch (_) {}
  }

  void setFilesDroppedHandler(void Function(List<SourceHandle> files) handler) {
    _channel.setMethodCallHandler((call) async {
      if (call.method == 'onFilesDropped') {
        final list = call.arguments as List<dynamic>?;
        if (list != null) {
          final sources = list
              .map((item) {
                final map = item as Map<dynamic, dynamic>;
                return SourceHandle(
                  token: map['token'] as String,
                  displayName: map['displayName'] as String,
                  isDirectory: false,
                );
              })
              .toList(growable: false);
          handler(sources);
        }
      }
    });
  }

  Future<void> copyToClipboard(String text) async {
    await Clipboard.setData(ClipboardData(text: text));
  }

  Future<String?> readClipboard() async {
    final data = await Clipboard.getData(Clipboard.kTextPlain);
    return data?.text;
  }
}
