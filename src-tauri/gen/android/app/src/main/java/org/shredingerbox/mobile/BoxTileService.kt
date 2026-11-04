package org.shredingerbox.mobile

import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Intent
import android.os.Build
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService
import androidx.annotation.RequiresApi

/** Плитка в шторке быстрых настроек: включить или выключить VPN, не открывая приложение. */
@RequiresApi(Build.VERSION_CODES.N)
class BoxTileService : TileService() {
  override fun onStartListening() {
    VpnState.app = applicationContext
    refresh()
  }

  override fun onClick() {
    when (VpnState.state) {
      VpnState.State.STARTED, VpnState.State.STARTING -> BoxVpnService.stop(this)
      VpnState.State.STOPPING -> {}
      VpnState.State.STOPPED -> {
        // Разрешение на VPN и первый конфиг даёт только приложение - открываем его.
        if (BoxVpnService.needsPermission(this) || !runCatching { BoxVpnService.startSaved(this) }.getOrDefault(false)) {
          openApp()
        }
      }
    }
    refresh()
  }

  @SuppressLint("StartActivityAndCollapseDeprecated")
  private fun openApp() {
    val intent = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      startActivityAndCollapse(PendingIntent.getActivity(this, 0, intent, PendingIntent.FLAG_IMMUTABLE))
    } else {
      @Suppress("DEPRECATION")
      startActivityAndCollapse(intent)
    }
  }

  private fun refresh() {
    val tile = qsTile ?: return
    val state = VpnState.state
    tile.state = when (state) {
      VpnState.State.STARTED -> Tile.STATE_ACTIVE
      VpnState.State.STOPPED -> Tile.STATE_INACTIVE
      else -> Tile.STATE_ACTIVE
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      tile.subtitle = when (state) {
        VpnState.State.STARTED -> Lang.t(this, "Подключено", "Connected")
        VpnState.State.STARTING -> Lang.t(this, "Подключаемся…", "Connecting…")
        VpnState.State.STOPPING -> Lang.t(this, "Отключаемся…", "Disconnecting…")
        VpnState.State.STOPPED -> Lang.t(this, "Выключено", "Off")
      }
    }
    tile.updateTile()
  }
}
