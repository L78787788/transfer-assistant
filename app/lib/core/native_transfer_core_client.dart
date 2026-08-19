import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';
import 'package:flutter/material.dart';

import 'models.dart';
import 'transfer_core_client.dart';

typedef _NativeJsonCall = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _DartJsonCall = Pointer<Utf8> Function(Pointer<Utf8>);
typedef _NativePoll = Pointer<Utf8> Function();
typedef _DartPoll = Pointer<Utf8> Function();
typedef _NativeFree = Void Function(Pointer<Utf8>);
typedef _DartFree = void Function(Pointer<Utf8>);

class NativeTransferCoreClient implements TransferCoreClient {
  NativeTransferCoreClient._(DynamicLibrary library)
    : _library = library,
      _loadError = null,
      _initialize = library.lookupFunction<_NativeJsonCall, _DartJsonCall>(
        'transassist_initialize',
      ),
      _invoke = library.lookupFunction<_NativeJsonCall, _DartJsonCall>(
        'transassist_invoke',
      ),
      _poll = library.lookupFunction<_NativePoll, _DartPoll>(
        'transassist_poll_event',
      ),
      _free = library.lookupFunction<_NativeFree, _DartFree>(
        'transassist_free_string',
      );

  factory NativeTransferCoreClient.create() {
    try {
      final library = DynamicLibrary.open(
        Platform.isWindows ? 'transfer_core.dll' : 'libtransfer_core.so',
      );
      return NativeTransferCoreClient._(library);
    } catch (error) {
      return NativeTransferCoreClient._unavailable(error.toString());
    }
  }

  NativeTransferCoreClient._unavailable(this._loadError)
    : _library = null,
      _initialize = null,
      _invoke = null,
      _poll = null,
      _free = null;

  final DynamicLibrary? _library;
  final String? _loadError;
  final _DartJsonCall? _initialize;
  final _DartJsonCall? _invoke;
  final _DartPoll? _poll;
  final _DartFree? _free;
  final _events = StreamController<CoreEvent>.broadcast();
  Timer? _pollTimer;

  @override
  Stream<CoreEvent> get events => _events.stream;

  @override
  Future<void> initialize({
    required AppSettings settings,
    required String dataDirectory,
    String? identityWrapKey,
    String? logDirectory,
  }) async {
    if (_library == null) {
      _events.add(CoreFailure('传输内核未加载：$_loadError'));
      return;
    }
    _call(_initialize!, {
      'settings': settings.toJson(),
      'data_directory': dataDirectory,
      'identity_wrap_key': ?identityWrapKey,
      'log_directory': ?logDirectory,
    });
    _pollTimer = Timer.periodic(
      const Duration(milliseconds: 120),
      (_) => _drainEvents(),
    );
    _drainEvents();
  }

  @override
  Future<void> refreshPeers() => _command('refresh_peers');

  @override
  Future<void> connectAddress(String address) =>
      _command('connect_address', {'address': address});

  @override
  Future<String> send(String peerId, List<SourceHandle> sources) async {
    final response = _call(_invoke!, {
      'command': 'send',
      'peer_id': peerId,
      'sources': sources.map((source) => source.toJson()).toList(),
    });
    return response['transfer_id']! as String;
  }

  @override
  Future<void> answerOffer(
    String offerId, {
    required bool accept,
    required bool rememberPeer,
  }) => _command('answer_offer', {
    'offer_id': offerId,
    'accept': accept,
    'remember_peer': rememberPeer,
  });

  @override
  Future<void> commandTransfer(String transferId, TransferCommand command) =>
      _command('command_transfer', {
        'transfer_id': transferId,
        'action': _snakeCase(command.name),
      });

  @override
  Future<void> updateSettings(AppSettings settings) =>
      _command('update_settings', {'settings': settings.toJson()});

  @override
  Future<void> removeTrustedPeer(String peerId) =>
      _command('remove_trusted_peer', {'peer_id': peerId});

  @override
  Future<List<HistoryFileItem>> listHistoryFiles() async {
    final response = _call(_invoke!, {'command': 'list_history_files'});
    final list = (response['files'] as List<Object?>? ?? const [])
        .cast<Map<String, Object?>>();
    return list.map(HistoryFileItem.fromJson).toList(growable: false);
  }

  @override
  Future<List<TransferItem>> listTransferItems(String transferId) async {
    final response = _call(_invoke!, {
      'command': 'list_transfer_items',
      'transfer_id': transferId,
    });
    final list = (response['items'] as List<Object?>? ?? const [])
        .cast<Map<String, Object?>>();
    return list.map(TransferItem.fromJson).toList(growable: false);
  }

  @override
  Future<int> clearHistory() async {
    final response = _call(_invoke!, {'command': 'clear_history'});
    return (response['count'] as num?)?.toInt() ?? 0;
  }

  @override
  Future<bool> deleteHistoryTransfer(String transferId) async {
    final response = _call(_invoke!, {
      'command': 'delete_history_transfer',
      'transfer_id': transferId,
    });
    return response['deleted'] == true;
  }

  @override
  Future<void> shutdown() async {
    _pollTimer?.cancel();
    if (_library != null) {
      _command('shutdown');
    }
    await _events.close();
  }

  Future<void> _command(String command, [Map<String, Object?>? body]) async {
    if (_library == null) {
      throw StateError('传输内核未加载');
    }
    _call(_invoke!, {'command': command, ...?body});
  }

  Map<String, Object?> _call(
    _DartJsonCall function,
    Map<String, Object?> request,
  ) {
    final input = jsonEncode(request).toNativeUtf8();
    try {
      final output = function(input);
      if (output == nullptr) {
        throw StateError('传输内核返回了空响应');
      }
      final decoded = jsonDecode(output.toDartString()) as Map<String, Object?>;
      _free!(output);
      if (decoded['ok'] != true) {
        throw StateError(decoded['error'] as String? ?? '传输内核操作失败');
      }
      return decoded;
    } finally {
      malloc.free(input);
    }
  }

  void _drainEvents() {
    if (_poll == null) return;
    for (var count = 0; count < 100; count++) {
      final pointer = _poll();
      if (pointer == nullptr) break;
      final json = jsonDecode(pointer.toDartString()) as Map<String, Object?>;
      _free!(pointer);
      _events.add(_decodeEvent(json));
    }
  }

  CoreEvent _decodeEvent(Map<String, Object?> json) => switch (json['type']) {
    'ready' => const CoreReady(),
    'settings_loaded' => CoreSettingsLoaded(
      AppSettings(
        deviceName: json['device_name']! as String,
        receiveDirectory: json['receive_directory']! as String,
        backgroundReceive: json['background_receive'] == true,
        autoAcceptTrusted: json['auto_accept_trusted'] == true,
        themeMode: _themeModeFromName(json['theme_mode'] as String?),
        themeStyle: _themeStyleFromName(json['theme_style'] as String?),
      ),
    ),
    'peers_changed' => PeersChanged(
      (json['peers']! as List<Object?>)
          .cast<Map<String, Object?>>()
          .map(PeerSummary.fromJson)
          .toList(),
    ),
    'transfers_changed' => TransfersChanged(
      (json['transfers']! as List<Object?>)
          .cast<Map<String, Object?>>()
          .map(TransferSnapshot.fromJson)
          .toList(),
    ),
    'incoming_offer' => IncomingOffer(
      TransferOffer.fromJson(json['offer']! as Map<String, Object?>),
    ),
    'failure' => CoreFailure(json['message']! as String),
    _ => CoreFailure(json['message'] as String? ?? '收到未知核心事件'),
  };
}

String _snakeCase(String value) => value.replaceAllMapped(
  RegExp('[A-Z]'),
  (match) => '_${match[0]!.toLowerCase()}',
);

ThemeMode _themeModeFromName(String? name) => switch (name) {
  'light' => ThemeMode.light,
  'dark' => ThemeMode.dark,
  _ => ThemeMode.system,
};

AppThemeStyle _themeStyleFromName(String? name) => switch (name) {
  'chat' => AppThemeStyle.chat,
  _ => AppThemeStyle.radar,
};
