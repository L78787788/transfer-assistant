import 'models.dart';

abstract interface class TransferCoreClient {
  Stream<CoreEvent> get events;

  Future<void> initialize({
    required AppSettings settings,
    required String dataDirectory,
    String? identityWrapKey,
  });

  Future<void> refreshPeers();

  Future<void> connectAddress(String address);

  Future<String> send(String peerId, List<SourceHandle> sources);

  Future<void> answerOffer(
    String offerId, {
    required bool accept,
    required bool rememberPeer,
  });

  Future<void> commandTransfer(String transferId, TransferCommand command);

  Future<void> updateSettings(AppSettings settings);

  Future<void> removeTrustedPeer(String peerId);

  Future<void> shutdown();
}
