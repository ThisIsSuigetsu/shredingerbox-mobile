package org.shredingerbox.mobile

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log

/** Подключиться после включения телефона, если это включено в настройках приложения. */
class BootReceiver : BroadcastReceiver() {
  companion object {
    private const val PREFS = "shredingerbox"
    private const val AUTOSTART = "autostart"

    fun setAutostart(context: Context, on: Boolean) {
      context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putBoolean(AUTOSTART, on).apply()
    }
  }

  override fun onReceive(context: Context, intent: Intent) {
    if (intent.action != Intent.ACTION_BOOT_COMPLETED) return
    if (!context.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(AUTOSTART, false)) return
    // Без выданного разрешения VPN не поднять — пользователь подключится сам.
    if (BoxVpnService.needsPermission(context)) return
    VpnState.app = context.applicationContext
    runCatching { BoxVpnService.startSaved(context) }
      .onFailure { Log.e("ShredingerBox", "автозапуск не удался", it) }
  }
}
