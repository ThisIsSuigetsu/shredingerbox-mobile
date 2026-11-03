package org.shredingerbox.mobile

/** Состояние VPN, общее для сервиса и плагина: оба живут в одном процессе. */
object VpnState {
  enum class State { STOPPED, STARTING, STARTED, STOPPING }

  @Volatile
  var state = State.STOPPED
    private set

  /** Почему ядро остановилось само; сбрасывается при следующем запуске. */
  @Volatile
  var error: String? = null
    private set

  fun set(state: State, error: String? = null) {
    this.state = state
    this.error = error
  }
}
