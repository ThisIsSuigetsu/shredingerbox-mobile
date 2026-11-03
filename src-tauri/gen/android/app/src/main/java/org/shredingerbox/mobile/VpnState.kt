package org.shredingerbox.mobile

import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.service.quicksettings.TileService

/** Состояние VPN, общее для сервиса, плагина и плитки: все живут в одном процессе. */
object VpnState {
  enum class State { STOPPED, STARTING, STARTED, STOPPING }

  @Volatile
  var state = State.STOPPED
    private set

  /** Почему ядро остановилось само; сбрасывается при следующем запуске. */
  @Volatile
  var error: String? = null
    private set

  /** Контекст приложения — чтобы плитка в шторке узнала о смене состояния. */
  @Volatile
  var app: Context? = null

  fun set(state: State, error: String? = null) {
    this.state = state
    this.error = error
    val ctx = app ?: return
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
      runCatching { TileService.requestListeningState(ctx, ComponentName(ctx, BoxTileService::class.java)) }
    }
  }
}
