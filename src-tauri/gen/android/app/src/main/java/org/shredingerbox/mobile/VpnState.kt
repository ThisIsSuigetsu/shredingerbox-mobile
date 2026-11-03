package org.shredingerbox.mobile

import android.app.ActivityManager
import android.content.ComponentName
import android.content.Context
import android.os.Build
import android.service.quicksettings.TileService
import org.json.JSONObject
import java.io.File

/**
 * Состояние VPN. Сервис, плитка и автозапуск живут в отдельном процессе «:vpn»,
 * а интерфейс - в основном: Tauri завершает основной процесс, когда закрывают окно,
 * и VPN не должен падать вместе с ним. Поэтому состояние хранится в памяти процесса
 * «:vpn» и дублируется в файл, который читает интерфейс.
 */
object VpnState {
  enum class State { STOPPED, STARTING, STARTED, STOPPING }

  /** Снимок для интерфейса. */
  data class Snapshot(val state: State, val error: String?, val api: Pair<Int, String>?)

  @Volatile
  var state = State.STOPPED
    private set

  /** Почему ядро остановилось само; сбрасывается при следующем запуске. */
  @Volatile
  var error: String? = null
    private set

  /** Clash API запущенного ядра «порт» to «пароль» - Rust подхватывает его по нему. */
  @Volatile
  var api: Pair<Int, String>? = null

  /** Контекст приложения: чтобы записать файл и обновить плитку в шторке. */
  @Volatile
  var app: Context? = null

  private fun file(ctx: Context) = File(ctx.filesDir, "vpn-state.json")

  fun set(state: State, error: String? = null) {
    this.state = state
    this.error = error
    if (state == State.STOPPED) api = null
    val ctx = app ?: return
    write(ctx)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
      runCatching { TileService.requestListeningState(ctx, ComponentName(ctx, BoxTileService::class.java)) }
    }
  }

  /** Атомарно: интерфейс не должен прочитать файл наполовину записанным. */
  private fun write(ctx: Context) {
    val json = JSONObject()
      .put("state", state.name)
      .put("error", error)
      .put("at", System.currentTimeMillis())
    api?.let { (port, secret) -> json.put("apiPort", port).put("apiSecret", secret) }
    runCatching {
      val tmp = File(ctx.filesDir, "vpn-state.json.tmp")
      tmp.writeText(json.toString())
      tmp.renameTo(file(ctx))
    }
  }

  /** Прочитать состояние из процесса интерфейса. */
  fun read(ctx: Context): Snapshot {
    val json = runCatching { JSONObject(file(ctx).readText()) }.getOrNull()
      ?: return Snapshot(State.STOPPED, null, null)
    var state = runCatching { State.valueOf(json.optString("state")) }.getOrDefault(State.STOPPED)
    val error = if (json.isNull("error")) null else json.optString("error")
    // Процесс «:vpn» могли убить (сбой, нехватка памяти) - тогда файл врёт.
    // «Подключаемся» сразу после нажатия ещё может не застать службу запущенной.
    val fresh = System.currentTimeMillis() - json.optLong("at") < 10_000
    if (state != State.STOPPED && !(state == State.STARTING && fresh) && !serviceRunning(ctx)) {
      state = State.STOPPED
    }
    val api = if (state == State.STARTED && json.has("apiPort")) {
      json.optInt("apiPort") to json.optString("apiSecret")
    } else null
    return Snapshot(state, if (state == State.STOPPED) error else null, api)
  }

  @Suppress("DEPRECATION")
  private fun serviceRunning(ctx: Context): Boolean {
    val manager = ctx.getSystemService(ActivityManager::class.java) ?: return false
    // Свои службы getRunningServices по-прежнему возвращает.
    return manager.getRunningServices(Int.MAX_VALUE).any { it.service.className == BoxVpnService::class.java.name }
  }
}
