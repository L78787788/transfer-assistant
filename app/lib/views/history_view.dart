import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/models.dart';
import '../state/app_controller.dart';
import '../widgets/common.dart';

enum HistoryViewMode { files, batches }

enum HistoryDirectionFilter { all, incoming, outgoing }

enum HistoryCategoryFilter {
  all('全部'),
  media('影音图片'),
  docs('文档'),
  apps('应用安装包'),
  archives('压缩包');

  const HistoryCategoryFilter(this.label);
  final String label;
}

class HistoryView extends StatefulWidget {
  const HistoryView({super.key, required this.controller});

  final AppController controller;

  @override
  State<HistoryView> createState() => _HistoryViewState();
}

class _HistoryViewState extends State<HistoryView> {
  var _viewMode = HistoryViewMode.files;
  var _directionFilter = HistoryDirectionFilter.all;
  var _categoryFilter = HistoryCategoryFilter.all;
  var _searchQuery = '';
  final _searchController = TextEditingController();

  @override
  void dispose() {
    _searchController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;

    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 1080),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 20),
          child: Column(
            children: [
              PageHeader(
                title: '传输历史',
                actions: [
                  SegmentedButton<HistoryViewMode>(
                    segments: const [
                      ButtonSegment(
                        value: HistoryViewMode.files,
                        label: Text('文件汇总'),
                        icon: Icon(LucideIcons.files, size: 16),
                      ),
                      ButtonSegment(
                        value: HistoryViewMode.batches,
                        label: Text('任务归档'),
                        icon: Icon(LucideIcons.folderArchive, size: 16),
                      ),
                    ],
                    selected: {_viewMode},
                    showSelectedIcon: false,
                    onSelectionChanged: (value) =>
                        setState(() => _viewMode = value.first),
                  ),
                  if (widget.controller.historyFiles.isNotEmpty) ...[
                    const SizedBox(width: 8),
                    IconButton.filledTonal(
                      onPressed: () => _confirmClearHistory(context),
                      tooltip: '清空全部历史记录',
                      icon: const Icon(LucideIcons.trash2, size: 18),
                      visualDensity: VisualDensity.compact,
                    ),
                  ],
                ],
              ),
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Row(
                  children: [
                    Expanded(
                      child: TextField(
                        controller: _searchController,
                        decoration: InputDecoration(
                          hintText: '搜索文件名、路径或设备名...',
                          prefixIcon:
                              const Icon(LucideIcons.search, size: 18),
                          suffixIcon: _searchQuery.isNotEmpty
                              ? IconButton(
                                  onPressed: () {
                                    _searchController.clear();
                                    setState(() => _searchQuery = '');
                                  },
                                  icon: const Icon(LucideIcons.x, size: 16),
                                )
                              : null,
                          isDense: true,
                          contentPadding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 10,
                          ),
                        ),
                        onChanged: (value) =>
                            setState(() => _searchQuery = value.trim()),
                      ),
                    ),
                    const SizedBox(width: 10),
                    SegmentedButton<HistoryDirectionFilter>(
                      segments: const [
                        ButtonSegment(
                          value: HistoryDirectionFilter.all,
                          label: Text('全部'),
                        ),
                        ButtonSegment(
                          value: HistoryDirectionFilter.incoming,
                          label: Text('已接收'),
                        ),
                        ButtonSegment(
                          value: HistoryDirectionFilter.outgoing,
                          label: Text('已发送'),
                        ),
                      ],
                      selected: {_directionFilter},
                      showSelectedIcon: false,
                      onSelectionChanged: (value) =>
                          setState(() => _directionFilter = value.first),
                    ),
                  ],
                ),
              ),
              if (_viewMode == HistoryViewMode.files)
                Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: Row(
                      children: HistoryCategoryFilter.values.map((cat) {
                        final isSelected = _categoryFilter == cat;
                        final isDark = Theme.of(context).brightness == Brightness.dark;
                        final activeColor = isDark ? colors.primary : const Color(0xff0284c7);
                        return Padding(
                          padding: const EdgeInsets.only(right: 6),
                          child: FilterChip(
                            label: Text(
                              cat.label,
                              style: TextStyle(
                                fontSize: 12,
                                fontWeight: isSelected ? FontWeight.w600 : FontWeight.w500,
                                color: isSelected
                                    ? activeColor
                                    : colors.onSurfaceVariant,
                              ),
                            ),
                            selected: isSelected,
                            onSelected: (_) =>
                                setState(() => _categoryFilter = cat),
                            visualDensity: VisualDensity.compact,
                            showCheckmark: false,
                            materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
                            backgroundColor: isDark ? Colors.transparent : colors.surface,
                            selectedColor: activeColor.withValues(alpha: isDark ? 0.20 : 0.12),
                            side: BorderSide(
                              color: isSelected
                                  ? activeColor
                                  : colors.outline.withValues(alpha: isDark ? 0.35 : 0.6),
                              width: isSelected ? 1.4 : 1.0,
                            ),
                          ),
                        );
                      }).toList(growable: false),
                    ),
                  ),
                ),
              Expanded(
                child: _viewMode == HistoryViewMode.files
                    ? _buildFilesList()
                    : _buildBatchesList(),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildFilesList() {
    final query = _searchQuery.toLowerCase();
    final files = widget.controller.historyFiles.where((file) {
      if (_directionFilter == HistoryDirectionFilter.incoming &&
          file.direction != TransferDirection.incoming) {
        return false;
      }
      if (_directionFilter == HistoryDirectionFilter.outgoing &&
          file.direction != TransferDirection.outgoing) {
        return false;
      }
      if (!_matchesCategory(file.fileName)) {
        return false;
      }
      if (query.isNotEmpty) {
        final matchesName = file.fileName.toLowerCase().contains(query);
        final matchesPath = file.relativePath.toLowerCase().contains(query);
        final matchesPeer = file.peerName.toLowerCase().contains(query);
        if (!matchesName && !matchesPath && !matchesPeer) return false;
      }
      return true;
    }).toList(growable: false);

    if (files.isEmpty) {
      return EmptyState(
        title: _searchQuery.isNotEmpty || _categoryFilter != HistoryCategoryFilter.all
            ? '没有找到匹配的历史文件'
            : '暂无已完成的传输文件',
        description: _searchQuery.isNotEmpty || _categoryFilter != HistoryCategoryFilter.all
            ? '尝试更换搜索关键词或选择其他分类'
            : '从「附近设备」发送或接收文件后将在此汇总展示',
        action: _searchQuery.isNotEmpty || _categoryFilter != HistoryCategoryFilter.all
            ? OutlinedButton.icon(
                onPressed: () {
                  _searchController.clear();
                  setState(() {
                    _searchQuery = '';
                    _categoryFilter = HistoryCategoryFilter.all;
                  });
                },
                icon: const Icon(LucideIcons.x, size: 16),
                label: const Text('重置筛选条件'),
              )
            : FilledButton.icon(
                onPressed: () => widget.controller.selectPage(0),
                icon: const Icon(LucideIcons.radio, size: 16),
                label: const Text('去传输文件'),
              ),
      );
    }

    return ListView.separated(
      padding: const EdgeInsets.only(bottom: 24, top: 4),
      itemCount: files.length,
      separatorBuilder: (_, _) => const SizedBox(height: 8),
      itemBuilder: (_, index) {
        final item = files[index];
        final isApk = item.fileName.toLowerCase().endsWith('.apk');
        return _HistoryFileCard(
          item: item,
          onOpen: () => widget.controller.openHistoryFile(item),
          onLocate: () => widget.controller.locateHistoryFile(item),
          onShare: () => widget.controller.shareHistoryFile(item),
          onInstall: isApk ? () => widget.controller.installHistoryFile(item) : null,
        );
      },
    );
  }

  bool _matchesCategory(String fileName) {
    final ext = fileName.contains('.') ? fileName.split('.').last.toLowerCase() : '';
    return switch (_categoryFilter) {
      HistoryCategoryFilter.all => true,
      HistoryCategoryFilter.media => const [
        'png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg',
        'mp4', 'mkv', 'avi', 'mov', 'flv', 'wmv', 'webm',
        'mp3', 'wav', 'flac', 'aac', 'm4a', 'ogg'
      ].contains(ext),
      HistoryCategoryFilter.docs => const [
        'pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx',
        'txt', 'md', 'json', 'yaml', 'log', 'csv'
      ].contains(ext),
      HistoryCategoryFilter.apps => const [
        'apk', 'exe', 'msi', 'dmg', 'deb', 'rpm'
      ].contains(ext),
      HistoryCategoryFilter.archives => const [
        'zip', 'rar', '7z', 'tar', 'gz', 'bz2'
      ].contains(ext),
    };
  }

  Widget _buildBatchesList() {
    final completedTransfers = widget.controller.transfers.where((t) {
      if (t.state != TransferState.completed) return false;
      if (_directionFilter == HistoryDirectionFilter.incoming &&
          t.direction != TransferDirection.incoming) {
        return false;
      }
      if (_directionFilter == HistoryDirectionFilter.outgoing &&
          t.direction != TransferDirection.outgoing) {
        return false;
      }
      if (_searchQuery.isNotEmpty) {
        return t.peerName.toLowerCase().contains(_searchQuery.toLowerCase());
      }
      return true;
    }).toList(growable: false);

    if (completedTransfers.isEmpty) {
      return EmptyState(
        title: _searchQuery.isNotEmpty ? '没有找到匹配的任务' : '暂无历史任务归档',
        description: '每次完成的传输批次会作为任务归档保留在此处',
        action: FilledButton.icon(
          onPressed: () => widget.controller.selectPage(0),
          icon: const Icon(LucideIcons.radio, size: 16),
          label: const Text('去传输文件'),
        ),
      );
    }

    return ListView.separated(
      padding: const EdgeInsets.only(bottom: 24, top: 4),
      itemCount: completedTransfers.length,
      separatorBuilder: (_, _) => const SizedBox(height: 10),
      itemBuilder: (_, index) => _HistoryBatchCard(
        transfer: completedTransfers[index],
        controller: widget.controller,
      ),
    );
  }

  Future<void> _confirmClearHistory(BuildContext context) async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('清空历史记录'),
        content: const Text('确定要清空所有传输历史记录吗？\n注意：本地已接收保存的文件不会被删除。'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            style: FilledButton.styleFrom(
              backgroundColor: Theme.of(context).colorScheme.error,
              foregroundColor: Colors.white,
            ),
            child: const Text('确认清空'),
          ),
        ],
      ),
    );
    if (confirmed == true) {
      await widget.controller.clearHistory();
    }
  }
}

class _HistoryFileCard extends StatelessWidget {
  const _HistoryFileCard({
    required this.item,
    required this.onOpen,
    required this.onLocate,
    this.onShare,
    this.onInstall,
  });

  final HistoryFileItem item;
  final VoidCallback onOpen;
  final VoidCallback onLocate;
  final VoidCallback? onShare;
  final VoidCallback? onInstall;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final iconData = _resolveFileIcon(item.fileName, item.isDirectory);
    final isIncoming = item.direction == TransferDirection.incoming;

    return GlassCard(
      padding: EdgeInsets.zero,
      child: InkWell(
        onTap: onOpen,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          child: Row(
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: colors.primary.withValues(alpha: 0.12),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Icon(iconData, color: colors.primary, size: 20),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      item.fileName,
                      style: Theme.of(context).textTheme.titleSmall?.copyWith(
                            fontWeight: FontWeight.w600,
                            height: 1.25,
                          ),
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 4),
                    Text.rich(
                      TextSpan(
                        children: [
                          WidgetSpan(
                            alignment: PlaceholderAlignment.middle,
                            child: Padding(
                              padding: const EdgeInsets.only(right: 3),
                              child: Icon(
                                isIncoming
                                    ? LucideIcons.arrowDownLeft
                                    : LucideIcons.arrowUpRight,
                                size: 12,
                                color: isIncoming
                                    ? colors.primary
                                    : colors.secondary,
                              ),
                            ),
                          ),
                          TextSpan(
                            text:
                                '${isIncoming ? '来自 ' : '发送给 '}${item.peerName} · ${formatBytes(item.size)} · ${_formatTime(item.completedAt)}',
                          ),
                        ],
                      ),
                      style: Theme.of(context).textTheme.bodySmall?.copyWith(
                            color: colors.onSurfaceVariant,
                            fontSize: 12,
                          ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 4),
              if (onInstall != null)
                Padding(
                  padding: const EdgeInsets.only(right: 4),
                  child: FilledButton.tonalIcon(
                    onPressed: onInstall,
                    icon: const Icon(LucideIcons.packagePlus, size: 16),
                    label: const Text('安装'),
                    style: FilledButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                      padding: const EdgeInsets.symmetric(horizontal: 10),
                    ),
                  ),
                ),
              if (onShare != null)
                IconButton(
                  onPressed: onShare,
                  tooltip: '分享',
                  icon: const Icon(LucideIcons.share2, size: 18),
                  visualDensity: VisualDensity.compact,
                ),
              IconButton(
                onPressed: onLocate,
                tooltip: '在文件管理器中定位',
                icon: const Icon(LucideIcons.folderOpen, size: 18),
                visualDensity: VisualDensity.compact,
              ),
              IconButton(
                onPressed: onOpen,
                tooltip: '打开文件',
                icon: const Icon(LucideIcons.externalLink, size: 18),
                visualDensity: VisualDensity.compact,
              ),
            ],
          ),
        ),
      ),
    );
  }

  IconData _resolveFileIcon(String fileName, bool isDirectory) {
    if (isDirectory) return LucideIcons.folder;
    final ext = fileName.contains('.')
        ? fileName.split('.').last.toLowerCase()
        : '';
    return switch (ext) {
      'png' || 'jpg' || 'jpeg' || 'gif' || 'webp' || 'bmp' || 'svg' =>
        LucideIcons.image,
      'mp4' || 'mkv' || 'avi' || 'mov' || 'flv' || 'wmv' || 'webm' =>
        LucideIcons.video,
      'mp3' || 'wav' || 'flac' || 'aac' || 'm4a' || 'ogg' =>
        LucideIcons.music,
      'zip' || 'rar' || '7z' || 'tar' || 'gz' || 'bz2' =>
        LucideIcons.archive,
      'pdf' || 'doc' || 'docx' || 'xls' || 'xlsx' || 'ppt' || 'pptx' || 'txt' || 'md' =>
        LucideIcons.fileText,
      'apk' || 'exe' || 'msi' || 'dmg' || 'sh' || 'bat' || 'dart' || 'rs' || 'json' || 'yaml' =>
        LucideIcons.fileCode,
      _ => LucideIcons.file,
    };
  }

  String _formatTime(DateTime time) {
    final now = DateTime.now();
    final diff = now.difference(time);
    if (diff.inMinutes < 1) return '刚刚';
    if (diff.inMinutes < 60) return '${diff.inMinutes} 分钟前';
    if (diff.inHours < 24) return '${diff.inHours} 小时前';
    return '${time.month}月${time.day}日 ${time.hour.toString().padLeft(2, '0')}:${time.minute.toString().padLeft(2, '0')}';
  }
}

class _HistoryBatchCard extends StatefulWidget {
  const _HistoryBatchCard({
    required this.transfer,
    required this.controller,
  });

  final TransferSnapshot transfer;
  final AppController controller;

  @override
  State<_HistoryBatchCard> createState() => _HistoryBatchCardState();
}

class _HistoryBatchCardState extends State<_HistoryBatchCard> {
  var _expanded = false;
  List<TransferItem>? _items;
  var _isLoadingItems = false;

  Future<void> _toggleExpand() async {
    final next = !_expanded;
    setState(() => _expanded = next);
    if (next && _items == null) {
      setState(() => _isLoadingItems = true);
      final items = await widget.controller.loadTransferItems(widget.transfer.id);
      if (mounted) {
        setState(() {
          _items = items;
          _isLoadingItems = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final t = widget.transfer;
    final isIncoming = t.direction == TransferDirection.incoming;

    return GlassCard(
      child: Column(
        children: [
          Row(
            children: [
              Icon(
                isIncoming ? LucideIcons.download : LucideIcons.upload,
                size: 20,
                color: colors.primary,
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      t.peerName,
                      style: Theme.of(context).textTheme.titleMedium?.copyWith(
                            fontWeight: FontWeight.w600,
                          ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      '${t.itemCount} 项 · ${formatBytes(t.totalBytes)}',
                      style: TextStyle(
                        color: colors.onSurfaceVariant,
                        fontSize: 12,
                      ),
                    ),
                  ],
                ),
              ),
              IconButton(
                onPressed: () => widget.controller.platform.openDirectory(
                  widget.controller.settings.receiveDirectory,
                ),
                tooltip: '打开接收目录',
                icon: const Icon(LucideIcons.folderOpen, size: 20),
              ),
              IconButton(
                onPressed: _toggleExpand,
                tooltip: _expanded ? '收起文件清单' : '展开文件清单',
                icon: Icon(
                  _expanded ? LucideIcons.chevronUp : LucideIcons.chevronDown,
                  size: 20,
                ),
              ),
            ],
          ),
          if (_expanded) ...[
            const SizedBox(height: 12),
            const Divider(height: 1),
            if (_isLoadingItems)
              const Padding(
                padding: EdgeInsets.all(20),
                child: Center(child: CircularProgressIndicator()),
              )
            else if (_items != null && _items!.isEmpty)
              const Padding(
                padding: EdgeInsets.all(16),
                child: Text('无详细文件记录'),
              )
            else if (_items != null)
              ListView.builder(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                itemCount: _items!.length,
                itemBuilder: (context, idx) {
                  final item = _items![idx];
                  return ListTile(
                    dense: true,
                    leading: Icon(
                      item.isDirectory ? LucideIcons.folder : LucideIcons.file,
                      size: 18,
                      color: colors.primary,
                    ),
                    title: Text(item.relativePath),
                    trailing: Text(
                      formatBytes(item.size),
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                    onTap: () {
                      final path = item.finalRef ?? item.temporaryRef;
                      if (path != null && path.isNotEmpty) {
                        widget.controller.platform.openDirectory(
                          widget.controller.settings.receiveDirectory,
                          selectFile: path,
                        );
                      }
                    },
                  );
                },
              ),
          ],
        ],
      ),
    );
  }
}