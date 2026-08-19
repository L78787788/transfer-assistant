package com.transassist.transfer_assistant

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.graphics.BitmapFactory
import android.net.wifi.WifiManager
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.widget.RemoteViews

class TransferForegroundService : Service() {
    private var multicastLock: WifiManager.MulticastLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            try {
                manager.deleteNotificationChannel("background_receive")
                manager.deleteNotificationChannel("transassist_bg_channel_v2")
                manager.deleteNotificationChannel("transassist_bg_channel_v3")
            } catch (_: Exception) {}
            val channel = NotificationChannel(
                CHANNEL_ID,
                "后台服务",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "保持局域网发现与文件传输连接"
                setShowBadge(false)
            }
            manager.createNotificationChannel(channel)
        }
        val appIconBitmap = try {
            BitmapFactory.decodeResource(resources, R.mipmap.ic_launcher)
        } catch (_: Exception) { null }

        val notification = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(this)
        }
            .setSmallIcon(R.drawable.ic_notification)
            .apply {
                if (appIconBitmap != null) {
                    setLargeIcon(appIconBitmap)
                }
            }
            .setColor(0xFF0284C7.toInt())
            .setContentTitle("传输助手")
            .setContentText("局域网极速发现与传输服务运行中")
            .setOngoing(true)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                    setCategory(Notification.CATEGORY_SERVICE)
                }
            }
            .build()
        startForegroundCompat(NOTIFICATION_ID, notification)

        acquireMulticastLock()
        checkActiveLocks()
    }

    private fun startForegroundCompat(id: Int, notification: Notification) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val serviceType = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC or
                    android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE
            } else {
                android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC
            }
            try {
                startForeground(id, notification, serviceType)
            } catch (_: Exception) {
                startForeground(id, notification)
            }
        } else {
            startForeground(id, notification)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        acquireMulticastLock()
        checkActiveLocks()
        return START_STICKY
    }

    private fun checkActiveLocks() {
        val preferences = getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        val active = preferences.getBoolean(TRANSFER_KEY, false)
        if (active) {
            if (wifiLock?.isHeld != true) {
                val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
                wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "transassist-transfer").apply {
                    setReferenceCounted(false)
                    acquire()
                }
            }
            if (wakeLock?.isHeld != true) {
                val power = getSystemService(Context.POWER_SERVICE) as PowerManager
                wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "transassist:transfer").apply {
                    setReferenceCounted(false)
                    acquire()
                }
            }
        } else {
            wifiLock?.takeIf { it.isHeld }?.release()
            wifiLock = null
            wakeLock?.takeIf { it.isHeld }?.release()
            wakeLock = null
        }
    }

    private fun acquireMulticastLock() {
        if (multicastLock?.isHeld == true) return
        multicastLock?.takeIf { it.isHeld }?.release()
        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("transassist-discovery").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    override fun onDestroy() {
        multicastLock?.takeIf { it.isHeld }?.release()
        multicastLock = null
        wifiLock?.takeIf { it.isHeld }?.release()
        wifiLock = null
        wakeLock?.takeIf { it.isHeld }?.release()
        wakeLock = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    companion object {
        private const val CHANNEL_ID = "transassist_bg_channel_v4"
        private const val NOTIFICATION_ID = 53317

        fun setBackgroundEnabled(context: Context, enabled: Boolean) {
            updateReason(context, BACKGROUND_KEY, enabled)
        }

        fun setTransferActive(context: Context, active: Boolean) {
            updateReason(context, TRANSFER_KEY, active)
        }

        private fun updateReason(context: Context, key: String, enabled: Boolean) {
            val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
            preferences.edit().putBoolean(key, enabled).apply()
            val shouldRun = enabled || when (key) {
                BACKGROUND_KEY -> preferences.getBoolean(TRANSFER_KEY, false)
                else -> preferences.getBoolean(BACKGROUND_KEY, false)
            }
            val intent = Intent(context, TransferForegroundService::class.java)
            if (shouldRun) {
                try {
                    context.startForegroundService(intent)
                } catch (_: Exception) {
                    context.startService(intent)
                }
            } else {
                context.stopService(intent)
            }
        }

        private const val PREFERENCES = "service_state"
        private const val BACKGROUND_KEY = "background_receive"
        private const val TRANSFER_KEY = "active_transfer"
        private const val EVENTS_CHANNEL_ID = "transassist_event_channel_v3"

        fun updateProgressNotification(
            context: Context,
            title: String,
            speedText: String,
            percent: Int,
            active: Boolean,
        ) {
            val manager = context.getSystemService(NotificationManager::class.java) ?: return
            val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            val pendingIntent = if (launchIntent != null) {
                android.app.PendingIntent.getActivity(
                    context,
                    0,
                    launchIntent,
                    android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE,
                )
            } else null

            val appIconBitmap = try {
                BitmapFactory.decodeResource(context.resources, R.mipmap.ic_launcher)
            } catch (_: Exception) { null }

            val notification = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                Notification.Builder(context, CHANNEL_ID)
            } else {
                @Suppress("DEPRECATION")
                Notification.Builder(context)
            }
                .setSmallIcon(R.drawable.ic_notification)
                .apply {
                    if (appIconBitmap != null) {
                        setLargeIcon(appIconBitmap)
                    }
                }
                .setColor(0xFF0284C7.toInt())
                .setContentTitle(if (active) "传输进行中 · $title" else "传输助手")
                .setContentText(if (active) "$speedText · ${percent}%" else "局域网极速发现与传输服务运行中")
                .setOngoing(true)
                .apply {
                    if (active && percent in 0..100) {
                        setProgress(100, percent, false)
                    }
                    if (pendingIntent != null) {
                        setContentIntent(pendingIntent)
                    }
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                        setCategory(Notification.CATEGORY_PROGRESS)
                    }
                }
                .build()
            manager.notify(NOTIFICATION_ID, notification)
        }

        fun showTransferNotification(context: Context, title: String, body: String) {
            val manager = context.getSystemService(NotificationManager::class.java) ?: return
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                val channel = NotificationChannel(
                    EVENTS_CHANNEL_ID,
                    "传输通知",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ).apply {
                    description = "接收与发送文件的进度与完成提醒"
                }
                manager.createNotificationChannel(channel)
            }
            val launchIntent = context.packageManager.getLaunchIntentForPackage(context.packageName)
            val pendingIntent = if (launchIntent != null) {
                android.app.PendingIntent.getActivity(
                    context,
                    0,
                    launchIntent,
                    android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE,
                )
            } else null

            val appIconBitmap = try {
                BitmapFactory.decodeResource(context.resources, R.mipmap.ic_launcher)
            } catch (_: Exception) { null }

            val notification = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                Notification.Builder(context, EVENTS_CHANNEL_ID)
            } else {
                @Suppress("DEPRECATION")
                Notification.Builder(context)
            }
                .setSmallIcon(R.drawable.ic_notification)
                .apply {
                    if (appIconBitmap != null) {
                        setLargeIcon(appIconBitmap)
                    }
                }
                .setColor(0xFF0284C7.toInt())
                .setContentTitle(title)
                .setContentText(body)
                .setAutoCancel(true)
                .apply {
                    if (pendingIntent != null) {
                        setContentIntent(pendingIntent)
                    }
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                        setCategory(Notification.CATEGORY_EVENT)
                    }
                }
                .build()
            manager.notify((System.currentTimeMillis() % 10000).toInt() + 1000, notification)
        }
    }
}
