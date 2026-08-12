package com.transassist.transfer_assistant

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Notification
import android.app.Service
import android.content.Context
import android.content.Intent
import android.net.wifi.WifiManager
import android.os.IBinder
import android.os.PowerManager

class TransferForegroundService : Service() {
    private var multicastLock: WifiManager.MulticastLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var wakeLock: PowerManager.WakeLock? = null

    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL_ID, "后台接收", NotificationManager.IMPORTANCE_LOW),
        )
        val notification = Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.mipmap.ic_launcher)
            .setContentTitle("传输助手")
            .setContentText("可接收同一局域网设备的文件")
            .setOngoing(true)
            .build()
        startForeground(NOTIFICATION_ID, notification)

        val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager
        multicastLock = wifi.createMulticastLock("transassist-discovery").apply {
            setReferenceCounted(false)
            acquire()
        }
        wifiLock = wifi.createWifiLock(WifiManager.WIFI_MODE_FULL_HIGH_PERF, "transassist-transfer").apply {
            setReferenceCounted(false)
            acquire()
        }
        val power = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = power.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "transassist:transfer").apply {
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
        private const val CHANNEL_ID = "background_receive"
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
            if (shouldRun) context.startForegroundService(intent) else context.stopService(intent)
        }

        private const val PREFERENCES = "service_state"
        private const val BACKGROUND_KEY = "background_receive"
        private const val TRANSFER_KEY = "active_transfer"
    }
}
