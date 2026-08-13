package com.transassist.transfer_assistant

import android.app.Activity
import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
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
            if (Build.VERSION.SDK_INT >= 36 &&
                checkSelfPermission("android.permission.ACCESS_LOCAL_NETWORK") != PackageManager.PERMISSION_GRANTED
            ) {
                add("android.permission.ACCESS_LOCAL_NETWORK")
            }
        }
        if (permissions.isNotEmpty()) requestPermissions(permissions.toTypedArray(), REQUEST_PERMISSIONS)
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
