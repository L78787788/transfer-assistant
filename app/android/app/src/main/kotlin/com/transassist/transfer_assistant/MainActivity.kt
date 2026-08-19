package com.transassist.transfer_assistant

import android.app.Activity
import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.net.wifi.WifiManager
import android.os.Build
import android.os.Environment
import android.os.ParcelFileDescriptor
import android.system.Os
import android.system.OsConstants
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel
import java.io.File

class MainActivity : FlutterActivity() {
    private var pendingResult: MethodChannel.Result? = null
    private var pendingMode: PickerMode? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private var sharedPayload: Map<String, Any?>? = null

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        handleShareIntent(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleShareIntent(intent)
    }

    private fun handleShareIntent(intent: Intent?) {
        if (intent == null) return
        when (intent.action) {
            Intent.ACTION_SEND -> {
                val text = intent.getStringExtra(Intent.EXTRA_TEXT)
                val uri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableExtra(Intent.EXTRA_STREAM)
                }
                if (uri != null) {
                    sharedPayload = mapOf("type" to "files", "paths" to listOf("android-saf:$uri"))
                } else if (!text.isNullOrBlank()) {
                    sharedPayload = mapOf("type" to "text", "text" to text)
                }
            }
            Intent.ACTION_SEND_MULTIPLE -> {
                val uris = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java)
                } else {
                    @Suppress("DEPRECATION")
                    intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM)
                }
                if (!uris.isNullOrEmpty()) {
                    val list = uris.map { "android-saf:$it" }
                    sharedPayload = mapOf("type" to "files", "paths" to list)
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        acquireMulticastLock()
    }

    private fun acquireMulticastLock() {
        if (multicastLock?.isHeld == true) return
        multicastLock?.takeIf { it.isHeld }?.release()
        val wifi = applicationContext.getSystemService(WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("transassist-foreground-discovery").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    override fun onPause() {
        multicastLock?.takeIf { it.isHeld }?.release()
        multicastLock = null
        super.onPause()
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        AndroidStorageBridge.initialize(this)
        requestNetworkPermissions()
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL).setMethodCallHandler { call, result ->
            when (call.method) {
                "paths" -> result.success(
                    mapOf(
                        "dataDirectory" to File(filesDir, "transfer-core").absolutePath,
                        "receiveDirectory" to configuredReceiveDirectory(),
                        "identityWrapKey" to IdentityKeyStore(this).wrappingKeyBase64(),
                        "logDirectory" to (getExternalFilesDir(null)?.absolutePath ?: filesDir.absolutePath),
                    ),
                )
                "pickFiles" -> launchFilePicker(result)
                "pickDirectory" -> launchDirectoryPicker(result, PickerMode.SOURCE_DIRECTORY)
                "chooseReceiveDirectory" -> launchDirectoryPicker(result, PickerMode.RECEIVE_DIRECTORY)
                "setBackgroundReceive" -> {
                    val enabled = call.arguments as? Boolean ?: false
                    TransferForegroundService.setBackgroundEnabled(this, enabled)
                    result.success(null)
                }
                "setTransferActive" -> {
                    val active = call.arguments as? Boolean ?: false
                    TransferForegroundService.setTransferActive(this, active)
                    result.success(null)
                }
                "showNotification" -> {
                    val title = call.argument<String>("title") ?: "传输助手"
                    val body = call.argument<String>("body") ?: ""
                    TransferForegroundService.showTransferNotification(this, title, body)
                    result.success(null)
                }
                "updateNotificationProgress" -> {
                    val title = call.argument<String>("title") ?: "文件"
                    val speed = call.argument<String>("speed") ?: "0 B/s"
                    val percent = call.argument<Int>("percent") ?: 0
                    val active = call.argument<Boolean>("active") ?: false
                    TransferForegroundService.updateProgressNotification(this, title, speed, percent, active)
                    result.success(null)
                }
                "openFile" -> {
                    val rawPath = call.argument<String>("path") ?: ""
                    openAndroidFile(rawPath)
                    result.success(null)
                }
                "installApk" -> {
                    val rawPath = call.argument<String>("path") ?: ""
                    installAndroidApk(rawPath)
                    result.success(null)
                }
                "shareFile" -> {
                    val rawPath = call.argument<String>("path") ?: ""
                    shareAndroidFile(rawPath)
                    result.success(null)
                }
                "openDirectory" -> {
                    val rawPath = call.argument<String>("path") ?: ""
                    openAndroidDirectory(rawPath)
                    result.success(null)
                }
                "getSharedPayload" -> {
                    result.success(sharedPayload)
                }
                "clearSharedPayload" -> {
                    sharedPayload = null
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
    }

    private fun launchFilePicker(result: MethodChannel.Result) {
        if (!beginPicker(result, PickerMode.FILES)) return
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION)
        }
        startActivityForResult(intent, REQUEST_PICK)
    }

    private fun launchDirectoryPicker(result: MethodChannel.Result, mode: PickerMode) {
        if (!beginPicker(result, mode)) return
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION,
            )
        }
        startActivityForResult(intent, REQUEST_PICK)
    }

    private fun beginPicker(result: MethodChannel.Result, mode: PickerMode): Boolean {
        if (pendingResult != null) {
            result.error("picker_busy", "已有文件选择窗口正在打开", null)
            return false
        }
        pendingResult = result
        pendingMode = mode
        return true
    }

    @Deprecated("Activity result compatibility is required down to Android 9")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode != REQUEST_PICK) return
        val result = pendingResult ?: return
        val mode = pendingMode
        pendingResult = null
        pendingMode = null
        if (resultCode != Activity.RESULT_OK || data == null) {
            result.success(if (mode == PickerMode.RECEIVE_DIRECTORY) null else emptyList<Any>())
            return
        }

        val uris = buildList {
            data.data?.let(::add)
            data.clipData?.let { clip ->
                for (index in 0 until clip.itemCount) add(clip.getItemAt(index).uri)
            }
        }.distinct()
        val grantedFlags = data.flags and
            (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        uris.forEach { uri -> persistPermission(uri, grantedFlags) }
        if (mode == PickerMode.RECEIVE_DIRECTORY) {
            val selected = uris.firstOrNull()
            if (selected == null) {
                result.success(null)
            } else {
                getSharedPreferences(RECEIVE_PREFERENCES, MODE_PRIVATE)
                    .edit()
                    .putString(RECEIVE_TREE_URI, selected.toString())
                    .apply()
                result.success("android-saf:$selected")
            }
        } else {
            try {
                val sources = if (mode == PickerMode.SOURCE_DIRECTORY) {
                    uris.firstOrNull()?.let(::enumerateTree).orEmpty()
                } else {
                    uris.map { uri -> sourceFile(uri, displayName(uri), null) }
                }
                result.success(sources)
            } catch (error: Exception) {
                result.error("source_open_failed", "无法读取所选文件：${error.message}", null)
            }
        }
    }

    private fun persistPermission(uri: Uri, flags: Int) {
        if (flags == 0) return
        try {
            contentResolver.takePersistableUriPermission(uri, flags)
        } catch (_: SecurityException) {
            // Some document providers grant access for the current process only.
        }
    }

    private fun enumerateTree(treeUri: Uri): List<Map<String, Any?>> {
        val rootId = DocumentsContract.getTreeDocumentId(treeUri)
        val rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootId)
        val rootName = displayName(rootUri)
        return buildList {
            add(sourceDirectory(rootName, queryModified(rootUri)))
            enumerateChildren(treeUri, rootId, rootName, this)
        }
    }

    private fun enumerateChildren(
        treeUri: Uri,
        parentDocumentId: String,
        parentRelativePath: String,
        output: MutableList<Map<String, Any?>>,
    ) {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentDocumentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
            val modifiedColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
            while (cursor.moveToNext()) {
                val documentId = cursor.getString(idColumn)
                val name = cursor.getString(nameColumn) ?: "未命名文件"
                val relativePath = "$parentRelativePath/$name"
                val modified = cursor.takeUnless { it.isNull(modifiedColumn) }?.getLong(modifiedColumn) ?: 0L
                if (cursor.getString(mimeColumn) == DocumentsContract.Document.MIME_TYPE_DIR) {
                    output.add(sourceDirectory(relativePath, modified))
                    enumerateChildren(treeUri, documentId, relativePath, output)
                } else {
                    val documentUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
                    val size = cursor.takeUnless { it.isNull(sizeColumn) }?.getLong(sizeColumn)
                    output.add(sourceFile(documentUri, name, relativePath, size, modified))
                }
            }
        }
    }

    private fun sourceDirectory(relativePath: String, modified: Long) = mapOf(
        "token" to "android-directory",
        "displayName" to relativePath.substringAfterLast('/'),
        "relativePath" to relativePath,
        "isDirectory" to true,
        "size" to 0L,
        "modifiedUnixMs" to modified,
    )

    private fun sourceFile(
        uri: Uri,
        displayName: String,
        relativePath: String?,
        knownSize: Long? = null,
        knownModified: Long? = null,
    ): Map<String, Any?> {
        val descriptor = contentResolver.openFileDescriptor(uri, "r")
            ?: error("文档提供程序没有返回文件描述符")
        val detached = descriptor.detachFd()
        val metadata = queryMetadata(uri)
        return mapOf(
            "token" to "android-fd:$detached",
            "persistentToken" to uri.toString(),
            "displayName" to displayName,
            "relativePath" to relativePath,
            "isDirectory" to false,
            "size" to (knownSize ?: metadata.first),
            "modifiedUnixMs" to (knownModified ?: metadata.second),
            "randomAccess" to isRandomAccess(detached),
        )
    }

    private fun isRandomAccess(descriptor: Int): Boolean {
        val wrapper = ParcelFileDescriptor.adoptFd(descriptor)
        return try {
            Os.lseek(wrapper.fileDescriptor, 0, OsConstants.SEEK_CUR)
            true
        } catch (_: Exception) {
            false
        } finally {
            wrapper.detachFd()
        }
    }

    private fun queryMetadata(uri: Uri): Pair<Long?, Long> {
        val projection = arrayOf(
            OpenableColumns.SIZE,
            DocumentsContract.Document.COLUMN_LAST_MODIFIED,
        )
        contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val sizeColumn = cursor.getColumnIndex(OpenableColumns.SIZE)
                val modifiedColumn = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_LAST_MODIFIED)
                val size = if (sizeColumn >= 0 && !cursor.isNull(sizeColumn)) cursor.getLong(sizeColumn) else null
                val modified =
                    if (modifiedColumn >= 0 && !cursor.isNull(modifiedColumn)) cursor.getLong(modifiedColumn) else 0L
                return size to modified
            }
        }
        return null to 0L
    }

    private fun queryModified(uri: Uri): Long = queryMetadata(uri).second

    private fun displayName(uri: Uri): String {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) return cursor.getString(0)
        }
        return uri.lastPathSegment ?: "未命名文件"
    }

    private fun defaultReceiveDirectory(): File {
        val base = getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS) ?: filesDir
        return File(base, "传输助手").apply { mkdirs() }
    }

    private fun configuredReceiveDirectory(): String {
        val treeUri = getSharedPreferences(RECEIVE_PREFERENCES, MODE_PRIVATE)
            .getString(RECEIVE_TREE_URI, null)
        return if (treeUri == null) defaultReceiveDirectory().absolutePath else "android-saf:$treeUri"
    }

    private fun requestNetworkPermissions() {
        val permissions = buildList {
            if (Build.VERSION.SDK_INT >= 33 &&
                checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
            ) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        if (permissions.isNotEmpty()) requestPermissions(permissions.toTypedArray(), REQUEST_PERMISSIONS)
    }

    private fun openAndroidFile(rawPath: String) {
        if (rawPath.isBlank()) return
        try {
            var uri: Uri? = null

            if (rawPath.startsWith("android-saf:")) {
                uri = Uri.parse(rawPath.removePrefix("android-saf:"))
            } else if (rawPath.startsWith("content://")) {
                uri = Uri.parse(rawPath)
            } else {
                var file = File(rawPath)
                if (!file.exists()) {
                    val defaultFile = File(defaultReceiveDirectory(), rawPath)
                    if (defaultFile.exists()) {
                        file = defaultFile
                    }
                }
                if (file.exists()) {
                    uri = androidx.core.content.FileProvider.getUriForFile(
                        this,
                        "$packageName.fileprovider",
                        file,
                    )
                }
            }

            if (uri == null) {
                android.util.Log.w("MainActivity", "无法解析文件 URI: $rawPath")
                return
            }

            val mimeType = resolveMimeType(rawPath, uri)

            // 1. 尝试直接以 ACTION_VIEW 打开
            val viewIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mimeType)
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                        Intent.FLAG_ACTIVITY_NEW_TASK,
                )
            }

            try {
                startActivity(viewIntent)
                return
            } catch (_: Exception) {
                // 2. 回退到系统打开方式选择器 Chooser
                try {
                    val chooserIntent = Intent.createChooser(viewIntent, "选择打开方式").apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    }
                    startActivity(chooserIntent)
                    return
                } catch (_: Exception) {}
            }

            // 3. 回退到系统分享面板打开
            try {
                val sendIntent = Intent(Intent.ACTION_SEND).apply {
                    type = mimeType
                    putExtra(Intent.EXTRA_STREAM, uri)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(
                    Intent.createChooser(sendIntent, "打开或发送文件").apply {
                        addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    },
                )
            } catch (e: Exception) {
                android.util.Log.e("MainActivity", "打开文件全部失败: $rawPath", e)
            }
        } catch (e: Exception) {
            android.util.Log.e("MainActivity", "打开文件异常: $rawPath", e)
        }
    }

    private fun resolveMimeType(pathOrUri: String, uri: Uri?): String {
        if (uri != null) {
            try {
                val queried = contentResolver.getType(uri)
                if (!queried.isNullOrBlank() && queried != "*/*") {
                    return queried
                }
            } catch (_: Exception) {}
        }

        val cleanName = pathOrUri.substringBefore('?').substringBefore('#')
        val ext = if (cleanName.contains('.')) {
            cleanName.substringAfterLast('.').lowercase()
        } else ""

        return when (ext) {
            "apk" -> "application/vnd.android.package-archive"
            "png" -> "image/png"
            "jpg", "jpeg" -> "image/jpeg"
            "gif" -> "image/gif"
            "webp" -> "image/webp"
            "bmp" -> "image/bmp"
            "svg" -> "image/svg+xml"
            "mp4" -> "video/mp4"
            "mkv" -> "video/x-matroska"
            "avi" -> "video/x-msvideo"
            "mov" -> "video/quicktime"
            "flv" -> "video/x-flv"
            "mp3" -> "audio/mpeg"
            "wav" -> "audio/x-wav"
            "flac" -> "audio/flac"
            "aac" -> "audio/aac"
            "m4a" -> "audio/mp4"
            "ogg" -> "audio/ogg"
            "pdf" -> "application/pdf"
            "txt", "log" -> "text/plain"
            "json" -> "application/json"
            "html", "htm" -> "text/html"
            "zip" -> "application/zip"
            "rar" -> "application/x-rar-compressed"
            "7z" -> "application/x-7z-compressed"
            "tar" -> "application/x-tar"
            "gz" -> "application/gzip"
            "doc" -> "application/msword"
            "docx" -> "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            "xls" -> "application/vnd.ms-excel"
            "xlsx" -> "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            "ppt" -> "application/vnd.ms-powerpoint"
            "pptx" -> "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            else -> {
                if (ext.isNotBlank()) {
                    android.webkit.MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
                } else "*/*"
            }
        }
    }

    private fun openAndroidDirectory(rawPath: String) {
        // 1. 如果配置了 SAF treeUri，优先通过 DocumentsContract 打开对应目录
        if (rawPath.startsWith("android-saf:")) {
            try {
                val treeUri = Uri.parse(rawPath.removePrefix("android-saf:"))
                val docId = DocumentsContract.getTreeDocumentId(treeUri)
                val docUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, docId)
                val intent = Intent(Intent.ACTION_VIEW).apply {
                    setDataAndType(docUri, DocumentsContract.Document.MIME_TYPE_DIR)
                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(intent)
                return
            } catch (_: Exception) {}
        }

        // 2. 尝试拉起各手机厂商内置系统「文件管理」应用 (Vivo、小米、华为、OPPO、三星、原生等)
        val knownFileManagerPackages = listOf(
            "com.android.filemanager",               // Vivo / 小米 / 原生
            "com.vivo.filemanager",                  // Vivo 专有
            "com.mi.android.globalFileexplorer",     // 小米国际版
            "com.huawei.hidisk",                     // 华为
            "com.hihonor.filemanager",               // 荣耀
            "com.coloros.filemanager",               // OPPO / Realme / OnePlus
            "com.oneplus.filemanager",               // 一加
            "com.sec.android.app.myfiles",           // 三星
            "com.google.android.documentsui",        // Google 原生 Files
            "com.android.documentsui",               // AOSP 文档
            "com.rarlab.rar",                        // RAR / 文件管理
            "com.estrongs.android.pop",              // ES 文件浏览器
            "pl.solidexplorer2"                      // Solid Explorer
        )

        for (pkg in knownFileManagerPackages) {
            try {
                val launchIntent = packageManager.getLaunchIntentForPackage(pkg)
                if (launchIntent != null) {
                    launchIntent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                    startActivity(launchIntent)
                    return
                }
            } catch (_: Exception) {}
        }

        // 3. 尝试使用系统下载管理界面
        try {
            val downloadIntent = Intent(android.app.DownloadManager.ACTION_VIEW_DOWNLOADS).apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(downloadIntent)
            return
        } catch (_: Exception) {}

        // 4. 尝试通用 Document Root 浏览
        try {
            val rootIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(
                    Uri.parse("content://com.android.externalstorage.documents/root/primary"),
                    "vnd.android.document/root",
                )
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(rootIntent)
        } catch (e: Exception) {
            android.util.Log.e("MainActivity", "打开文件管理器失败: $rawPath", e)
        }
    }

    private fun installAndroidApk(rawPath: String) {
        if (rawPath.isBlank()) return
        try {
            var uri: Uri? = null
            if (rawPath.startsWith("android-saf:") || rawPath.startsWith("content://")) {
                val s = if (rawPath.startsWith("android-saf:")) rawPath.removePrefix("android-saf:") else rawPath
                uri = Uri.parse(s)
            } else {
                var file = File(rawPath)
                if (!file.exists()) {
                    val defaultFile = File(defaultReceiveDirectory(), rawPath)
                    if (defaultFile.exists()) file = defaultFile
                }
                if (file.exists()) {
                    uri = androidx.core.content.FileProvider.getUriForFile(
                        this,
                        "$packageName.fileprovider",
                        file,
                    )
                }
            }
            if (uri == null) return

            val installIntent = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, "application/vnd.android.package-archive")
                addFlags(
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_ACTIVITY_NEW_TASK or
                        Intent.FLAG_ACTIVITY_CLEAR_TOP,
                )
            }
            startActivity(installIntent)
        } catch (e: Exception) {
            android.util.Log.e("MainActivity", "安装 APK 异常: $rawPath", e)
        }
    }

    private fun shareAndroidFile(rawPath: String) {
        if (rawPath.isBlank()) return
        try {
            var uri: Uri? = null
            if (rawPath.startsWith("android-saf:") || rawPath.startsWith("content://")) {
                val s = if (rawPath.startsWith("android-saf:")) rawPath.removePrefix("android-saf:") else rawPath
                uri = Uri.parse(s)
            } else {
                var file = File(rawPath)
                if (!file.exists()) {
                    val defaultFile = File(defaultReceiveDirectory(), rawPath)
                    if (defaultFile.exists()) file = defaultFile
                }
                if (file.exists()) {
                    uri = androidx.core.content.FileProvider.getUriForFile(
                        this,
                        "$packageName.fileprovider",
                        file,
                    )
                }
            }
            if (uri == null) return
            val mimeType = resolveMimeType(rawPath, uri)
            val shareIntent = Intent(Intent.ACTION_SEND).apply {
                type = mimeType
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            startActivity(Intent.createChooser(shareIntent, "分享文件").apply {
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            })
        } catch (e: Exception) {
            android.util.Log.e("MainActivity", "分享文件异常: $rawPath", e)
        }
    }

    private enum class PickerMode { FILES, SOURCE_DIRECTORY, RECEIVE_DIRECTORY }

    companion object {
        private const val CHANNEL = "transassist/platform"
        private const val REQUEST_PICK = 4107
        private const val REQUEST_PERMISSIONS = 4108
        private const val RECEIVE_PREFERENCES = "receive_storage"
        private const val RECEIVE_TREE_URI = "tree_uri"
    }
}
