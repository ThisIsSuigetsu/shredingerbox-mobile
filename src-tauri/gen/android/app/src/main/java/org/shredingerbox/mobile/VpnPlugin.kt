package org.shredingerbox.mobile

import android.app.Activity
import android.content.ClipboardManager
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.Drawable
import android.util.Base64
import app.tauri.plugin.JSArray
import java.io.ByteArrayOutputStream
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

  /** Последние строки журнала ядра. */
  @Command
  fun log(invoke: Invoke) {
    val file = BoxVpnService.logFile(activity)
    val text = if (file.exists()) file.readLines().takeLast(300).joinToString("\n") else ""
    invoke.resolve(JSObject().put("text", text))
  }

  /**
   * Текст из буфера обмена — для предложения добавить ключ. С Android 10 буфер
   * читается, только пока окно приложения в фокусе; иначе придёт null.
   */
  @Command
  fun clipboard(invoke: Invoke) {
    activity.runOnUiThread {
      val text = try {
        val cm = activity.getSystemService(ClipboardManager::class.java)
        cm.primaryClip?.takeIf { it.itemCount > 0 }?.getItemAt(0)?.coerceToText(activity)?.toString()
      } catch (e: Exception) {
        null
      }
      invoke.resolve(JSObject().put("text", text))
    }
  }

  /** Приложения с иконкой на рабочем столе — для правил «через VPN» и «напрямую». */
  @Command
  fun apps(invoke: Invoke) {
    Thread {
      try {
        val pm = activity.packageManager
        val launcher = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER)
        val list = pm.queryIntentActivities(launcher, 0)
          .map { it.activityInfo.applicationInfo }
          .distinctBy { it.packageName }
          .filter { it.packageName != activity.packageName }
          .map { info -> Triple(info.loadLabel(pm).toString(), info.packageName, info) }
          .sortedBy { it.first.lowercase() }
        val apps = JSArray()
        for ((name, pkg, info) in list) {
          apps.put(
            JSObject()
              .put("name", name)
              .put("package", pkg)
              .put("icon", iconData(info.loadIcon(pm))),
          )
        }
        invoke.resolve(JSObject().put("apps", apps))
      } catch (e: Exception) {
        invoke.reject("Не удалось получить список приложений: ${e.message}")
      }
    }.start()
  }

  /** Иконка 64×64 в виде data:-ссылки для <img>. */
  private fun iconData(drawable: Drawable): String? = runCatching {
    val size = 64
    val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
    drawable.setBounds(0, 0, size, size)
    drawable.draw(Canvas(bitmap))
    val out = ByteArrayOutputStream()
    bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
    "data:image/png;base64," + Base64.encodeToString(out.toByteArray(), Base64.NO_WRAP)
  }.getOrNull()

  @Command
  fun status(invoke: Invoke) {
    invoke.resolve(
      JSObject()
        .put("state", VpnState.state.name.lowercase())
        .put("error", VpnState.error),
    )
  }
}
