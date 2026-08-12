import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../state/app_controller.dart';
import '../core/models.dart';
import '../views/nearby_view.dart';
import '../views/settings_view.dart';
import '../views/transfers_view.dart';
import '../widgets/common.dart';

class AppShell extends StatelessWidget {
  const AppShell({super.key, required this.controller});

  final AppController controller;

  static const _destinations = [
    NavigationDestination(icon: Icon(LucideIcons.radio), label: '附近设备'),
    NavigationDestination(icon: Icon(LucideIcons.history), label: '传输任务'),
    NavigationDestination(icon: Icon(LucideIcons.settings), label: '设置'),
  ];

  @override
  Widget build(BuildContext context) {
    final pages = [
      NearbyView(controller: controller),
      TransfersView(controller: controller),
      SettingsView(controller: controller),
    ];
    return LayoutBuilder(
      builder: (context, constraints) {
        final desktop = constraints.maxWidth >= 840;
        return Stack(
          children: [
            Scaffold(
              body: SafeArea(
                child: Column(
                  children: [
                    if (controller.errorMessage case final message?)
                      ErrorBanner(
                        message: message,
                        onDismiss: controller.clearError,
                      ),
                    Expanded(
                      child: Row(
                        children: [
                          if (desktop)
                            _DesktopNavigation(controller: controller),
                          if (desktop) const VerticalDivider(width: 1),
                          Expanded(
                            child: IndexedStack(
                              index: controller.selectedPage,
                              children: pages,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              bottomNavigationBar: desktop
                  ? null
                  : NavigationBar(
                      selectedIndex: controller.selectedPage,
                      onDestinationSelected: controller.selectPage,
                      destinations: _destinations,
                    ),
            ),
            if (controller.pendingOffer case final TransferOffer offer)
              _OfferOverlay(controller: controller, offer: offer),
          ],
        );
      },
    );
  }
}

class _OfferOverlay extends StatefulWidget {
  const _OfferOverlay({required this.controller, required this.offer});

  final AppController controller;
  final TransferOffer offer;

  @override
  State<_OfferOverlay> createState() => _OfferOverlayState();
}

class _OfferOverlayState extends State<_OfferOverlay> {
  var remember = false;

  @override
  Widget build(BuildContext context) {
    final offer = widget.offer;
    final pairingCode = offer.pairingCode;
    final outgoing = offer.direction == TransferDirection.outgoing;
    return Material(
      color: Colors.transparent,
      child: Stack(
        children: [
          const ModalBarrier(dismissible: false, color: Color(0x66000000)),
          Center(
            child: AlertDialog(
              title: Text(
                outgoing
                    ? '与 ${offer.peerName} 核对验证码'
                    : '${offer.peerName} 请求发送文件',
              ),
              content: ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 440),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      '${offer.itemCount} 项 · ${formatBytes(offer.totalBytes)}',
                    ),
                    if (pairingCode != null) ...[
                      const SizedBox(height: 20),
                      Text(
                        '配对验证码',
                        style: Theme.of(context).textTheme.labelLarge,
                      ),
                      const SizedBox(height: 6),
                      SelectableText(
                        pairingCode,
                        style: Theme.of(context).textTheme.headlineSmall,
                      ),
                      const SizedBox(height: 8),
                      const Text('确认两台设备显示的六位数字完全一致。'),
                      CheckboxListTile(
                        contentPadding: EdgeInsets.zero,
                        title: const Text('信任此设备'),
                        value: remember,
                        onChanged: (value) =>
                            setState(() => remember = value ?? false),
                      ),
                    ],
                  ],
                ),
              ),
              actions: [
                TextButton(
                  onPressed: () => widget.controller.answerOffer(
                    accept: false,
                    rememberPeer: false,
                  ),
                  child: Text(outgoing ? '不匹配' : '拒绝'),
                ),
                FilledButton(
                  onPressed: () => widget.controller.answerOffer(
                    accept: true,
                    rememberPeer: remember,
                  ),
                  child: Text(outgoing ? '一致，继续' : '接收'),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _DesktopNavigation extends StatelessWidget {
  const _DesktopNavigation({required this.controller});

  final AppController controller;

  @override
  Widget build(BuildContext context) {
    final extended = MediaQuery.sizeOf(context).width >= 1080;
    return NavigationRail(
      extended: extended,
      minExtendedWidth: 224,
      selectedIndex: controller.selectedPage,
      onDestinationSelected: controller.selectPage,
      leading: Padding(
        padding: const EdgeInsets.only(top: 12, bottom: 20),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              LucideIcons.send,
              color: Theme.of(context).colorScheme.primary,
            ),
            if (extended) ...[
              const SizedBox(width: 12),
              Text('传输助手', style: Theme.of(context).textTheme.titleMedium),
            ],
          ],
        ),
      ),
      destinations: const [
        NavigationRailDestination(
          icon: Icon(LucideIcons.radio),
          label: Text('附近设备'),
        ),
        NavigationRailDestination(
          icon: Icon(LucideIcons.history),
          label: Text('传输任务'),
        ),
        NavigationRailDestination(
          icon: Icon(LucideIcons.settings),
          label: Text('设置'),
        ),
      ],
    );
  }
}
