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

  String? get etaText {
    if (!isActive || state != TransferState.transferring || bytesPerSecond <= 0) {
      return null;
    }
    final remainingBytes = (totalBytes - completedBytes).clamp(0, totalBytes);
    if (remainingBytes <= 0) return null;
    final seconds = remainingBytes ~/ bytesPerSecond;
    if (seconds < 5) return '即将完成';
    if (seconds < 60) return '约 $seconds 秒';
    final minutes = seconds ~/ 60;
    final remSeconds = seconds % 60;
    if (minutes < 60) {
      return remSeconds > 0 ? '约 $minutes 分 $remSeconds 秒' : '约 $minutes 分钟';
    }
    final hours = minutes ~/ 60;
    final remMinutes = minutes % 60;
    return '约 $hours 小时 $remMinutes 分';
  }

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

enum AppThemeStyle {
  radar('全景星轨雷达', 'AirDrop · 空间雷达天体交互'),
  chat('流式传输会话', '微信/Telegram · 设备会话流与气泡');

  const AppThemeStyle(this.label, this.description);
  final String label;
  final String description;
}

class AppSettings {
  const AppSettings({
    required this.deviceName,
    required this.receiveDirectory,
    this.backgroundReceive = false,
    this.autoAcceptTrusted = false,
    this.themeMode = ThemeMode.system,
    this.themeStyle = AppThemeStyle.radar,
    this.hasCompletedFirstSetup = false,
  });

  final String deviceName;
  final String receiveDirectory;
  final bool backgroundReceive;
  final bool autoAcceptTrusted;
  final ThemeMode themeMode;
  final AppThemeStyle themeStyle;
  final bool hasCompletedFirstSetup;

  AppSettings copyWith({
    String? deviceName,
    String? receiveDirectory,
    bool? backgroundReceive,
    bool? autoAcceptTrusted,
    ThemeMode? themeMode,
    AppThemeStyle? themeStyle,
    bool? hasCompletedFirstSetup,
  }) => AppSettings(
    deviceName: deviceName ?? this.deviceName,
    receiveDirectory: receiveDirectory ?? this.receiveDirectory,
    backgroundReceive: backgroundReceive ?? this.backgroundReceive,
    autoAcceptTrusted: autoAcceptTrusted ?? this.autoAcceptTrusted,
    themeMode: themeMode ?? this.themeMode,
    themeStyle: themeStyle ?? this.themeStyle,
    hasCompletedFirstSetup:
        hasCompletedFirstSetup ?? this.hasCompletedFirstSetup,
  );

  Map<String, Object?> toJson() => {
    'device_name': deviceName,
    'receive_directory': receiveDirectory,
    'background_receive': backgroundReceive,
    'auto_accept_trusted': autoAcceptTrusted,
    'theme_mode': themeMode.name,
    'theme_style': themeStyle.name,
    'has_completed_first_setup': hasCompletedFirstSetup,
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

class HistoryFileItem {
  const HistoryFileItem({
    required this.id,
    required this.transferId,
    required this.fileName,
    required this.relativePath,
    required this.localPath,
    required this.isDirectory,
    required this.size,
    required this.peerName,
    required this.direction,
    required this.completedAt,
  });

  final String id;
  final String transferId;
  final String fileName;
  final String relativePath;
  final String? localPath;
  final bool isDirectory;
  final int size;
  final String peerName;
  final TransferDirection direction;
  final DateTime completedAt;

  factory HistoryFileItem.fromJson(Map<String, Object?> json) =>
      HistoryFileItem(
        id: json['id']! as String,
        transferId: json['transfer_id']! as String,
        fileName: json['file_name']! as String,
        relativePath: json['relative_path']! as String,
        localPath: json['local_path'] as String?,
        isDirectory: json['is_directory'] == true,
        size: (json['size'] as num).toInt(),
        peerName: json['peer_name']! as String,
        direction: json['direction'] == 'incoming'
            ? TransferDirection.incoming
            : TransferDirection.outgoing,
        completedAt: DateTime.fromMillisecondsSinceEpoch(
          (json['completed_unix_ms'] as num).toInt(),
        ),
      );
}

class TransferItem {
  const TransferItem({
    required this.id,
    required this.transferId,
    required this.relativePath,
    required this.isDirectory,
    required this.size,
    required this.modifiedUnixMs,
    this.sourceRevision,
    this.temporaryRef,
    this.finalRef,
  });

  final String id;
  final String transferId;
  final String relativePath;
  final bool isDirectory;
  final int size;
  final int modifiedUnixMs;
  final String? sourceRevision;
  final String? temporaryRef;
  final String? finalRef;

  factory TransferItem.fromJson(Map<String, Object?> json) => TransferItem(
        id: json['id']! as String,
        transferId: json['transfer_id']! as String,
        relativePath: json['relative_path']! as String,
        isDirectory: json['kind'] == 'directory',
        size: (json['size'] as num).toInt(),
        modifiedUnixMs: (json['modified_unix_ms'] as num).toInt(),
        sourceRevision: json['source_revision'] as String?,
        temporaryRef: json['temporary_ref'] as String?,
        finalRef: json['final_ref'] as String?,
      );
}

class SharedPayload {
  const SharedPayload({
    required this.type,
    this.text,
    this.paths = const [],
  });

  final String type; // 'text' or 'files'
  final String? text;
  final List<String> paths;

  factory SharedPayload.fromJson(Map<String, Object?> json) {
    return SharedPayload(
      type: json['type'] as String? ?? 'files',
      text: json['text'] as String?,
      paths: (json['paths'] as List<Object?>? ?? const [])
          .map((e) => e.toString())
          .toList(growable: false),
    );
  }
}

class TrustedPeerInfo {
  const TrustedPeerInfo({
    required this.peerId,
    required this.displayName,
    required this.fingerprintHex,
    required this.createdUnixMs,
    required this.lastSeenUnixMs,
    required this.autoAccept,
  });

  final String peerId;
  final String displayName;
  final String fingerprintHex;
  final int createdUnixMs;
  final int lastSeenUnixMs;
  final bool autoAccept;

  DateTime get createdAt => DateTime.fromMillisecondsSinceEpoch(createdUnixMs);
  DateTime get lastSeenAt => DateTime.fromMillisecondsSinceEpoch(lastSeenUnixMs);

  factory TrustedPeerInfo.fromJson(Map<String, Object?> json) => TrustedPeerInfo(
    peerId: json['peer_id']! as String,
    displayName: json['display_name']! as String,
    fingerprintHex: json['fingerprint_hex']! as String,
    createdUnixMs: (json['created_unix_ms'] as num).toInt(),
    lastSeenUnixMs: (json['last_seen_unix_ms'] as num).toInt(),
    autoAccept: json['auto_accept'] == true || json['auto_accept'] == 1,
  );
}
