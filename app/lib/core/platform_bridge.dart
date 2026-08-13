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
}
