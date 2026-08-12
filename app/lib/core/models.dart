import 'package:flutter/material.dart';

enum DeviceKind { computer, phone }

enum TransferDirection { incoming, outgoing }

enum TransferState {
  preparing,
  connecting,
  pairing,
  waitingForAcceptance,
  transferring,
  paused,
  interrupted,
  verifying,
  completed,
  failed,
  cancelled,
}

enum TransferCommand { pause, resume, cancel, retry }

class PeerSummary {
  const PeerSummary({
    required this.id,
    required this.name,
    required this.address,
    required this.deviceKind,
    required this.trusted,
    this.online = true,
  });

  final String id;
  final String name;
  final String address;
  final DeviceKind deviceKind;
  final bool trusted;
  final bool online;

  factory PeerSummary.fromJson(Map<String, Object?> json) => PeerSummary(
    id: json['id']! as String,
    name: json['name']! as String,
    address: json['address']! as String,
    deviceKind: json['device_kind'] == 'phone'
        ? DeviceKind.phone
        : DeviceKind.computer,
    trusted: json['trusted'] == true,
    online: json['online'] != false,
  );
}

class TransferSnapshot {
  const TransferSnapshot({
    required this.id,
    required this.peerName,
    required this.direction,
    required this.state,
    required this.itemCount,
    required this.totalBytes,
    required this.completedBytes,
    required this.updatedAt,
    this.bytesPerSecond = 0,
    this.error,
  });

  final String id;
  final String peerName;
  final TransferDirection direction;
  final TransferState state;
  final int itemCount;
  final int totalBytes;
  final int completedBytes;
  final int bytesPerSecond;
  final DateTime updatedAt;
  final String? error;

  double get progress => totalBytes == 0
      ? (state == TransferState.completed ? 1 : 0)
      : (completedBytes / totalBytes).clamp(0, 1);

  bool get isActive => switch (state) {
    TransferState.completed ||
    TransferState.failed ||
    TransferState.cancelled => false,
    _ => true,
  };

  factory TransferSnapshot.fromJson(Map<String, Object?> json) {
    final stateName = (json['state']! as String).replaceAll('_', '');
    return TransferSnapshot(
      id: json['id']! as String,
      peerName: json['peer_name']! as String,
      direction: json['direction'] == 'incoming'
          ? TransferDirection.incoming
          : TransferDirection.outgoing,
      state: TransferState.values.firstWhere(
        (state) => state.name.toLowerCase() == stateName.toLowerCase(),
      ),
      itemCount: (json['item_count'] as num).toInt(),
      totalBytes: (json['total_bytes'] as num).toInt(),
      completedBytes: (json['completed_bytes'] as num).toInt(),
      bytesPerSecond: (json['bytes_per_second'] as num?)?.toInt() ?? 0,
      updatedAt: DateTime.fromMillisecondsSinceEpoch(
        (json['updated_unix_ms'] as num).toInt(),
      ),
      error: json['error'] as String?,
    );
  }
}

class TransferOffer {
  const TransferOffer({
    required this.id,
    required this.peerName,
    required this.itemCount,
    required this.totalBytes,
    required this.topLevelNames,
    required this.direction,
    this.pairingCode,
  });

  final String id;
  final String peerName;
  final int itemCount;
  final int totalBytes;
  final List<String> topLevelNames;
  final TransferDirection direction;
  final String? pairingCode;

  factory TransferOffer.fromJson(Map<String, Object?> json) => TransferOffer(
    id: json['id']! as String,
    peerName: json['peer_name']! as String,
    itemCount: (json['item_count'] as num).toInt(),
    totalBytes: (json['total_bytes'] as num).toInt(),
    topLevelNames: (json['top_level_names']! as List<Object?>).cast<String>(),
    direction: json['direction'] == 'outgoing'
        ? TransferDirection.outgoing
        : TransferDirection.incoming,
    pairingCode: json['pairing_code'] as String?,
  );
}

class SourceHandle {
  const SourceHandle({
    required this.token,
    required this.displayName,
    this.persistentToken,
    this.relativePath,
    this.isDirectory = false,
    this.size,
    this.modifiedUnixMs,
    this.randomAccess,
  });

  final String token;
  final String displayName;
  final String? persistentToken;
  final String? relativePath;
  final bool isDirectory;
  final int? size;
  final int? modifiedUnixMs;
  final bool? randomAccess;

  Map<String, Object?> toJson() => {
    'token': token,
    if (persistentToken != null) 'persistent_token': persistentToken,
    'display_name': displayName,
    if (relativePath != null) 'relative_path': relativePath,
    'is_directory': isDirectory,
    if (size != null) 'size': size,
    if (modifiedUnixMs != null) 'modified_unix_ms': modifiedUnixMs,
    if (randomAccess != null) 'random_access': randomAccess,
  };
}

class AppSettings {
  const AppSettings({
    required this.deviceName,
    required this.receiveDirectory,
    this.backgroundReceive = false,
    this.autoAcceptTrusted = false,
    this.themeMode = ThemeMode.system,
  });

  final String deviceName;
  final String receiveDirectory;
  final bool backgroundReceive;
  final bool autoAcceptTrusted;
  final ThemeMode themeMode;

  AppSettings copyWith({
    String? deviceName,
    String? receiveDirectory,
    bool? backgroundReceive,
    bool? autoAcceptTrusted,
    ThemeMode? themeMode,
  }) => AppSettings(
    deviceName: deviceName ?? this.deviceName,
    receiveDirectory: receiveDirectory ?? this.receiveDirectory,
    backgroundReceive: backgroundReceive ?? this.backgroundReceive,
    autoAcceptTrusted: autoAcceptTrusted ?? this.autoAcceptTrusted,
    themeMode: themeMode ?? this.themeMode,
  );

  Map<String, Object?> toJson() => {
    'device_name': deviceName,
    'receive_directory': receiveDirectory,
    'background_receive': backgroundReceive,
    'auto_accept_trusted': autoAcceptTrusted,
    'theme_mode': themeMode.name,
  };
}

sealed class CoreEvent {
  const CoreEvent();
}

class PeersChanged extends CoreEvent {
  const PeersChanged(this.peers);
  final List<PeerSummary> peers;
}

class TransfersChanged extends CoreEvent {
  const TransfersChanged(this.transfers);
  final List<TransferSnapshot> transfers;
}

class IncomingOffer extends CoreEvent {
  const IncomingOffer(this.offer);
  final TransferOffer offer;
}

class CoreFailure extends CoreEvent {
  const CoreFailure(this.message);
  final String message;
}

class CoreReady extends CoreEvent {
  const CoreReady();
}

class CoreSettingsLoaded extends CoreEvent {
  const CoreSettingsLoaded(this.settings);
  final AppSettings settings;
}
