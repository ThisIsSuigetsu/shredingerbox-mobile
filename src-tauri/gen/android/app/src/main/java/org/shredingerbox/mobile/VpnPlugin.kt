package org.shredingerbox.mobile

import android.app.Activity
import android.net.VpnService
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class StartArgs {
  lateinit var config: String
}

/** Команды VPN для Rust-части (src-tauri/src/vpn.rs). */
@TauriPlugin
class VpnPlugin(private val activity: Activity) : Plugin(activity) {
  /** Разрешение на VPN: системное окно появляется, только если его ещё не давали. */
  @Command
  fun prepare(invoke: Invoke) {
    val intent = VpnService.prepare(activity)
    if (intent == null) {
      invoke.resolve(JSObject().put("granted", true))
    } else {
      startActivityForResult(invoke, intent, "prepareResult")
    }
  }

  @ActivityCallback
  fun prepareResult(invoke: Invoke, result: ActivityResult) {
    invoke.resolve(JSObject().put("granted", result.resultCode == Activity.RESULT_OK))
  }

  @Command
  fun start(invoke: Invoke) {
    val args = invoke.parseArgs(StartArgs::class.java)
    BoxVpnService.start(activity, args.config)
    invoke.resolve()
  }

  @Command
  fun stop(invoke: Invoke) {
    BoxVpnService.stop(activity)
    invoke.resolve()
  }

  @Command
  fun status(invoke: Invoke) {
    invoke.resolve(
      JSObject()
        .put("state", VpnState.state.name.lowercase())
        .put("error", VpnState.error),
    )
  }
}
