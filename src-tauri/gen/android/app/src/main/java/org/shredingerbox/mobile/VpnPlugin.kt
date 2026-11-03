package org.shredingerbox.mobile

import android.app.Activity
import android.app.StatusBarManager
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Intent
import android.graphics.drawable.Icon
import android.os.Build
import android.provider.Settings
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.appcompat.app.AppCompatActivity
import android.util.Log
import com.google.android.gms.common.moduleinstall.ModuleInstall
import com.google.android.gms.common.moduleinstall.ModuleInstallRequest
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.codescanner.GmsBarcodeScanner
import com.google.mlkit.vision.codescanner.GmsBarcodeScannerOptions
import com.google.mlkit.vision.codescanner.GmsBarcodeScanning
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.drawable.Drawable
import android.util.Base64
import androidx.core.content.FileProvider
import app.tauri.plugin.JSArray
import java.io.ByteArrayOutputStream
import java.io.File
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

@InvokeArg
class InstallArgs {
  lateinit var path: String
}

@InvokeArg
class FlagArgs {
  var on: Boolean = false
}

/** Команды VPN для Rust-части (src-tauri/src/vpn.rs). */
@TauriPlugin
class VpnPlugin(private val activity: Activity) : Plugin(activity) {
  /** Ссылка shredingerbox://add?url=…, которой открыли приложение; забирается командой takeLink. */
  @Volatile private var pendingLink: String? = null

  override fun load(webView: WebView) {
    VpnState.app = activity.applicationContext
    remember(activity.intent)
    // «Назад» при включённом VPN сворачивает приложение, а не закрывает: так оно
    // открывается мгновенно. С выключенным - закрывается как обычно. Добавлен позже
    // обработчиков Tauri, поэтому срабатывает первым.
    val dispatcher = (activity as AppCompatActivity).onBackPressedDispatcher
    dispatcher.addCallback(activity, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (VpnState.read(activity).state != VpnState.State.STOPPED) {
          activity.moveTaskToBack(true)
        } else {
          isEnabled = false
          dispatcher.onBackPressed()
          isEnabled = true
        }
      }
    })
  }

  override fun onNewIntent(intent: Intent) = remember(intent)

  private fun remember(intent: Intent?) {
    val uri = intent?.data ?: return
    if (intent.action != Intent.ACTION_VIEW || uri.scheme != "shredingerbox") return
    pendingLink = uri.getQueryParameter("url")?.trim()?.takeIf { it.isNotEmpty() }
    // Повторный поворот экрана не должен предложить ту же ссылку ещё раз.
    intent.data = null
  }

  @Command
  fun takeLink(invoke: Invoke) {
    val link = pendingLink
    pendingLink = null
    invoke.resolve(JSObject().put("text", link))
  }

  /** Свежий конфиг для плитки, автозапуска и Always-on - без подключения. */
  @Command
  fun saveConfig(invoke: Invoke) {
    val args = invoke.parseArgs(StartArgs::class.java)
    BoxVpnService.saveConfig(activity, args.config)
    invoke.resolve()
  }

  @Command
  fun setAutostart(invoke: Invoke) {
    BootReceiver.setAutostart(activity, invoke.parseArgs(FlagArgs::class.java).on)
    invoke.resolve()
  }

  /** Системные настройки VPN - там включается Always-on («Постоянная VPN»). */
  @Command
  fun openVpnSettings(invoke: Invoke) {
    try {
      activity.startActivity(Intent(Settings.ACTION_VPN_SETTINGS))
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject("Не удалось открыть настройки VPN: ${e.message}")
    }
  }

  /** Попросить систему добавить плитку в шторку (Android 13+); раньше - только вручную. */
  @Command
  fun addTile(invoke: Invoke) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) {
      invoke.resolve(JSObject().put("supported", false))
      return
    }
    val manager = activity.getSystemService(StatusBarManager::class.java)
    manager.requestAddTileService(
      ComponentName(activity, BoxTileService::class.java),
      "ShredingerBox",
      Icon.createWithResource(activity, R.drawable.ic_vpn),
      activity.mainExecutor,
    ) { }
    invoke.resolve(JSObject().put("supported", true))
  }

  /**
   * Отсканировать QR-код с ключом или подпиской. null - пользователь закрыл сканер.
   * Сам сканер Google Play докачивает отдельным модулем при первом использовании. Пока модуля
   * нет, startScan падает с непонятной ошибкой, поэтому сначала проверяем, скачан ли он.
   */
  @Command
  fun scanQr(invoke: Invoke) {
    try {
      val options = GmsBarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build()
      val scanner = GmsBarcodeScanning.getClient(activity, options)
      val modules = ModuleInstall.getClient(activity)
      modules.areModulesAvailable(scanner)
        .addOnSuccessListener { status ->
          if (status.areModulesAvailable()) {
            startScan(scanner, invoke)
          } else {
            modules.installModules(ModuleInstallRequest.newBuilder().addApi(scanner).build())
            invoke.reject(
              "Сканер QR-кодов ещё скачивается через Google Play. Подождите минуту и попробуйте снова. " +
                "Если не скачивается, включите VPN и повторите.",
            )
          }
        }
        .addOnFailureListener { scannerUnavailable(invoke, it) }
    } catch (e: Throwable) {
      scannerUnavailable(invoke, e)
    }
  }

  private fun startScan(scanner: GmsBarcodeScanner, invoke: Invoke) {
    try {
      scanner.startScan()
        .addOnSuccessListener { invoke.resolve(JSObject().put("text", it.rawValue)) }
        .addOnCanceledListener { invoke.resolve(JSObject().put("text", null as String?)) }
        .addOnFailureListener { scannerUnavailable(invoke, it) }
    } catch (e: Throwable) {
      scannerUnavailable(invoke, e)
    }
  }

  private fun scannerUnavailable(invoke: Invoke, e: Throwable) {
    Log.e("ShredingerBox", "сканер QR-кодов", e)
    invoke.reject("Сканер QR-кодов недоступен - нужны сервисы Google Play. Ключ можно вставить вручную.")
  }
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
   * Текст из буфера обмена - для предложения добавить ключ. С Android 10 буфер
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

  /** Приложения с иконкой на рабочем столе - для правил «через VPN» и «напрямую». */
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

  /**
   * Открыть системный установщик для скачанного обновления. Если установка из
   * этого приложения ещё не разрешена, Android сам предложит её разрешить.
   */
  @Command
  fun install(invoke: Invoke) {
    val args = invoke.parseArgs(InstallArgs::class.java)
    try {
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", File(args.path))
      val intent = Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, "application/vnd.android.package-archive")
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject("Не удалось открыть установщик: ${e.message}")
    }
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
    // Служба живёт в другом процессе - состояние берём из её файла.
    val snapshot = VpnState.read(activity)
    val result = JSObject()
      .put("state", snapshot.state.name.lowercase())
      .put("error", snapshot.error)
    // Clash API ядра - Rust подхватывает его, если подключились без приложения.
    snapshot.api?.let { (port, secret) ->
      result.put("api", JSObject().put("port", port).put("secret", secret))
    }
    invoke.resolve(result)
  }
}
