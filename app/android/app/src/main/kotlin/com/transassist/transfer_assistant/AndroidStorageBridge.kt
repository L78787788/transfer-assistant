package com.transassist.transfer_assistant

import android.content.Context
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.system.Os
import android.system.OsConstants
import org.json.JSONArray
import org.json.JSONObject

object AndroidStorageBridge {
    private lateinit var context: Context

    init {
        System.loadLibrary("transfer_core")
    }

    fun initialize(context: Context) {
        this.context = context.applicationContext
        nativeRegister()
    }

    external fun nativeRegister()

    @Suppress("unused")
    fun prepareTargets(treeUriText: String, transferId: String, manifestJson: String): String {
        check(::context.isInitialized) { "SAF 桥尚未初始化" }
        val opened = mutableListOf<Int>()
        fun closeOpened() {
            opened.forEach { fd ->
                runCatching { ParcelFileDescriptor.adoptFd(fd).close() }
            }
        }
        try {
            val treeUri = Uri.parse(treeUriText)
            val rootId = DocumentsContract.getTreeDocumentId(treeUri)
            val rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, rootId)
            val entries = JSONArray(manifestJson)
            val rootNames = linkedMapOf<String, String>()
            val output = JSONArray()

            for (index in 0 until entries.length()) {
                val entry = entries.getJSONObject(index)
                val relativePath = entry.getString("relative_path")
                val segments = relativePath.split('/')
                val sourceRoot = segments.first()
                val targetRoot = rootNames.getOrPut(sourceRoot) {
                    uniqueChildName(rootUri, sourceRoot, entry.getBoolean("is_directory"))
                }
                val targetSegments = listOf(targetRoot) + segments.drop(1)
                if (entry.getBoolean("is_directory")) {
                    ensureDirectoryPath(rootUri, targetSegments)
                    continue
                }

                val parent = ensureDirectoryPath(rootUri, targetSegments.dropLast(1))
                val finalName = targetSegments.last()
                val temporaryName = ".$finalName.transassist-$transferId-${entry.getString("id")}.part"
                val existing = findChild(parent, temporaryName)
                val temporaryUri = existing?.uri ?: DocumentsContract.createDocument(
                    context.contentResolver,
                    parent,
                    "application/octet-stream",
                    temporaryName,
                ) ?: error("无法创建临时文档 $temporaryName")
                val descriptor = context.contentResolver.openFileDescriptor(temporaryUri, "rw")
                    ?: error("文档提供程序没有返回写入描述符")
                val detached = descriptor.detachFd()
                opened += detached
                output.put(
                    JSONObject()
                        .put("id", entry.getString("id"))
                        .put("fd", detached)
                        .put("temporary_uri", temporaryUri.toString())
                        .put("final_name", finalName)
                        .put("final_path", targetSegments.joinToString("/"))
                        .put("existed", existing != null)
                        .put("random_access", isRandomAccess(detached)),
                )
            }
            return output.toString()
        } catch (error: Exception) {
            // 中途失败时关闭所有已 detach 但尚未移交给 Rust 的文件描述符，
            // 防止进程内文件描述符泄漏。
            closeOpened()
            return JSONObject()
                .put("error", error.message ?: error.javaClass.simpleName)
                .toString()
        }
    }

    @Suppress("unused")
    fun finalizeTarget(temporaryUriText: String, finalName: String): Boolean {
        check(::context.isInitialized) { "SAF 桥尚未初始化" }
        return DocumentsContract.renameDocument(
            context.contentResolver,
            Uri.parse(temporaryUriText),
            finalName,
        ) != null
    }

    @Suppress("unused")
    fun deleteTarget(temporaryUriText: String): Boolean {
        check(::context.isInitialized) { "SAF 桥尚未初始化" }
        return DocumentsContract.deleteDocument(
            context.contentResolver,
            Uri.parse(temporaryUriText),
        )
    }

    @Suppress("unused")
    fun openSource(uriText: String): String {
        check(::context.isInitialized) { "SAF 桥尚未初始化" }
        return try {
            val uri = Uri.parse(uriText)
            val descriptor = context.contentResolver.openFileDescriptor(uri, "r")
                ?: error("无法打开源文件: $uriText")
            val detached = descriptor.detachFd()
            val random = isRandomAccess(detached)
            JSONObject()
                .put("fd", detached)
                .put("random_access", random)
                .toString()
        } catch (error: Exception) {
            JSONObject()
                .put("error", error.message ?: error.javaClass.simpleName)
                .toString()
        }
    }

    @Suppress("unused")
    fun sourceRevision(uriText: String): String {
        check(::context.isInitialized) { "SAF 桥尚未初始化" }
        return try {
            val uri = Uri.parse(uriText)
            context.contentResolver.query(
                uri,
                arrayOf(
                    "_size",
                    "last_modified",
                ),
                null,
                null,
                null,
            )?.use { cursor ->
                if (cursor.moveToFirst()) {
                    val size = when {
                        cursor.getColumnIndex("_size") >= 0 -> cursor.getLong(cursor.getColumnIndexOrThrow("_size"))
                        else -> 0L
                    }
                    val modified = when {
                        cursor.getColumnIndex("last_modified") >= 0 -> cursor.getLong(cursor.getColumnIndexOrThrow("last_modified"))
                        else -> 0L
                    }
                    return "$size:$modified"
                }
            }
            error("无法查询源文件元数据: $uriText")
        } catch (error: Exception) {
            JSONObject()
                .put("error", error.message ?: error.javaClass.simpleName)
                .toString()
        }
    }

    private fun ensureDirectoryPath(root: Uri, segments: List<String>): Uri {
        var current = root
        for (segment in segments) {
            val existing = findChild(current, segment)
            current = when {
                existing == null -> DocumentsContract.createDocument(
                    context.contentResolver,
                    current,
                    DocumentsContract.Document.MIME_TYPE_DIR,
                    segment,
                ) ?: error("无法创建目录 $segment")
                existing.isDirectory -> existing.uri
                else -> error("目标目录被同名文件占用: $segment")
            }
        }
        return current
    }

    private fun uniqueChildName(parent: Uri, requested: String, directory: Boolean): String {
        for (index in 0..Int.MAX_VALUE) {
            val candidate = if (index == 0) requested else suffixedName(requested, index, directory)
            if (findChild(parent, candidate) == null) return candidate
        }
        error("无法分配不重名的目标名称")
    }

    private fun suffixedName(name: String, index: Int, directory: Boolean): String {
        if (directory) return "$name ($index)"
        val dot = name.lastIndexOf('.')
        return if (dot > 0) {
            "${name.substring(0, dot)} ($index)${name.substring(dot)}"
        } else {
            "$name ($index)"
        }
    }

    private fun findChild(parent: Uri, name: String): ChildDocument? {
        val parentId = DocumentsContract.getDocumentId(parent)
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(parent, parentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
        )
        context.contentResolver.query(childrenUri, projection, null, null, null)?.use { cursor ->
            val idColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = cursor.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            while (cursor.moveToNext()) {
                if (cursor.getString(nameColumn) == name) {
                    val uri = DocumentsContract.buildDocumentUriUsingTree(
                        parent,
                        cursor.getString(idColumn),
                    )
                    return ChildDocument(
                        uri,
                        cursor.getString(mimeColumn) == DocumentsContract.Document.MIME_TYPE_DIR,
                    )
                }
            }
        }
        return null
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

    private data class ChildDocument(val uri: Uri, val isDirectory: Boolean)
}
