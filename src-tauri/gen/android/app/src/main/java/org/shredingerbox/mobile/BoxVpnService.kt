package org.shredingerbox.mobile

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.VpnService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.os.Process
import android.system.OsConstants
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import io.nekohasekai.libbox.BridgeOptions
import io.nekohasekai.libbox.BridgeSession
import io.nekohasekai.libbox.CommandServer
import io.nekohasekai.libbox.CommandServerHandler
import io.nekohasekai.libbox.ConnectionOwner
import io.nekohasekai.libbox.InterfaceUpdateListener
import io.nekohasekai.libbox.Libbox
import io.nekohasekai.libbox.LocalDNSTransport
import io.nekohasekai.libbox.NeighborUpdateListener
import io.nekohasekai.libbox.NetworkInterfaceIterator
import io.nekohasekai.libbox.OverrideOptions
import io.nekohasekai.libbox.PlatformInterface
import io.nekohasekai.libbox.PlatformUser
import io.nekohasekai.libbox.RoutePrefix
import io.nekohasekai.libbox.RoutePrefixIterator
import io.nekohasekai.libbox.SetupOptions
import io.nekohasekai.libbox.ShellSession
import io.nekohasekai.libbox.StringIterator
import io.nekohasekai.libbox.SystemProxyStatus
import io.nekohasekai.libbox.TunOptions
import io.nekohasekai.libbox.WIFIState
import org.json.JSONObject
import java.io.File
import java.net.Inet6Address
import java.net.InetSocketAddress
import java.net.NetworkInterface
import java.util.concurrent.Executors
import io.nekohasekai.libbox.Notification as BoxNotification
import io.nekohasekai.libbox.NetworkInterface as BoxNetworkInterface

/**
 * VPN-сервис: держит ядро sing-box (libbox) и TUN-интерфейс.
 * Ядро само решает, какие адреса и маршруты нужны, и просит интерфейс через openTun -
 * сервис строит его системным VpnService.Builder и возвращает дескриптор.
 * Устроено по образцу sing-box-for-android, без Tailscale, root и прочего лишнего.
 */
class BoxVpnService : VpnService(), PlatformInterface, CommandServerHandler {
  companion object {
    private const val TAG = "ShredingerBox"
    private const val ACTION_STOP = "org.shredingerbox.mobile.STOP"
    private const val ACTION_START = "org.shredingerbox.mobile.START"
    private const val CHANNEL = "vpn"
    private const val NOTIFICATION_ID = 1

    /** Libbox.setup вызывается один раз на процесс. */
    private var libboxReady = false

    fun configFile(context: Context) = File(context.filesDir, "config.json")

    /** Журнал ядра: перезаписывается при каждом подключении, показывается на экране «Журнал». */
    fun logFile(context: Context) = File(context.filesDir, "box.log")

    /**
     * Порт локального прокси, если ядро запущено в режиме «Прокси» (без TUN), иначе 0.
     * В этом режиме VPN не поднимается: ядро просто слушает 127.0.0.1.
     */
    @Volatile private var proxyPort = 0

    /**
     * Сохранить конфиг, не запуская ядро. Rust присылает свежий конфиг после каждого
     * изменения настроек - по нему подключаются плитка, автозапуск и Always-on.
     */
    fun saveConfig(context: Context, config: String) {
      // Путь к журналу знает только Android-сторона, поэтому дописываем его в конфиг здесь.
      val json = JSONObject(config)
      val log = json.optJSONObject("log") ?: JSONObject().also { json.put("log", it) }
      log.put("output", logFile(context).path)
      configFile(context).writeText(json.toString())
    }

    fun start(context: Context, config: String) {
      saveConfig(context, config)
      startSaved(context)
    }

    /** Режим «Прокси» в сохранённом конфиге: VPN не нужен. */
    private fun readProxyPort(json: JSONObject): Int {
      val inbounds = json.optJSONArray("inbounds")
      val list = (0 until (inbounds?.length() ?: 0)).map { inbounds!!.getJSONObject(it) }
      return if (list.any { it.optString("type") == "tun" }) 0
        else list.firstOrNull { it.optString("type") == "mixed" }?.optInt("listen_port") ?: 0
    }

    /** Нужно ли сначала дать разрешение на VPN (окно показывает только приложение). */
    fun needsPermission(context: Context): Boolean {
      val file = configFile(context)
      if (!file.exists()) return false
      val proxy = runCatching { readProxyPort(JSONObject(file.readText())) > 0 }.getOrDefault(false)
      return !proxy && VpnService.prepare(context) != null
    }

    /**
     * Подключиться по последнему сохранённому конфигу. false - конфига ещё нет.
     * Можно звать из любого процесса: конфиг разбирает уже сама служба в «:vpn».
     */
    fun startSaved(context: Context): Boolean {
      if (!configFile(context).exists()) return false
      // Сразу «подключаемся»: интерфейс не должен успеть увидеть старое «выключено» с прошлой ошибкой.
      VpnState.app = context.applicationContext
      VpnState.set(VpnState.State.STARTING)
      try {
        ContextCompat.startForegroundService(
          context, Intent(context, BoxVpnService::class.java).setAction(ACTION_START),
        )
      } catch (e: Exception) {
        VpnState.set(VpnState.State.STOPPED, e.message)
        throw e
      }
      return true
    }

    /** Разобрать сохранённый конфиг и перейти в «подключаемся» - уже внутри службы. */
    private fun prepareSaved(context: Context): Boolean {
      val file = configFile(context)
      if (!file.exists()) return false
      val json = JSONObject(file.readText())
      proxyPort = readProxyPort(json)
      val clash = json.optJSONObject("experimental")?.optJSONObject("clash_api")
      VpnState.api = clash?.let {
        val port = it.optString("external_controller").substringAfterLast(':').toIntOrNull()
        port?.let { p -> p to it.optString("secret") }
      }
      logFile(context).delete()
      VpnState.set(VpnState.State.STARTING)
      return true
    }

    fun stop(context: Context) {
      context.startService(Intent(context, BoxVpnService::class.java).setAction(ACTION_STOP))
    }
  }

  private val connectivity by lazy { getSystemService(ConnectivityManager::class.java) }
  private val network by lazy { NetworkMonitor(connectivity) }
  private val resolver by lazy { LocalResolver(network) }

  /** Ядро запускается и останавливается в одном фоновом потоке, по очереди. */
  private val worker = Executors.newSingleThreadExecutor()
  private var server: CommandServer? = null
  private var tun: ParcelFileDescriptor? = null

  /** Ядро уже запускается или работает в этом экземпляре службы. */
  private var running = false

  override fun onCreate() {
    super.onCreate()
    VpnState.app = applicationContext
  }

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    if (intent?.action == ACTION_STOP) {
      if (running) stopVpn(null) else stopSelf()
      return START_NOT_STICKY
    }
    // Запуск из приложения, плитки, автозапуска (ACTION_START) или Always-on - тогда
    // система сама шлёт SERVICE_INTERFACE. Повторный запуск во время работы игнорируем.
    if (intent?.action != ACTION_START && intent?.action != SERVICE_INTERFACE) return START_NOT_STICKY
    if (running) return START_NOT_STICKY
    if (!prepareSaved(this)) {
      VpnState.set(VpnState.State.STOPPED, "Нет сохранённого конфига - откройте приложение и подключитесь")
      stopSelf()
      return START_NOT_STICKY
    }
    running = true
    showNotification("Подключаемся…")
    worker.execute { startCore() }
    return START_NOT_STICKY
  }

  private fun startCore() {
    try {
      if (!libboxReady) {
        Libbox.setup(SetupOptions().apply {
          basePath = filesDir.path
          workingPath = File(filesDir, "core").apply { mkdirs() }.path
          tempPath = cacheDir.path
          fixAndroidStack = Build.VERSION.SDK_INT <= Build.VERSION_CODES.N_MR1
          logMaxLines = 1000
        })
        libboxReady = true
      }
      network.start()
      val server = CommandServer(this, this)
      server.start()
      this.server = server
      server.startOrReloadService(configFile(this).readText(), OverrideOptions())
      VpnState.set(VpnState.State.STARTED)
      showNotification(if (proxyPort > 0) "Прокси работает на 127.0.0.1:$proxyPort" else "Подключено")
    } catch (e: Exception) {
      Log.e(TAG, "не удалось запустить ядро", e)
      appendLog("ERROR не удалось запустить ядро: $e")
      shutdown(e.message ?: e.toString())
    }
  }

  /** Остановка по кнопке, из уведомления, по отзыву VPN системой или по просьбе ядра. */
  private fun stopVpn(error: String?) {
    val state = VpnState.state
    if (state == VpnState.State.STOPPED || state == VpnState.State.STOPPING) return
    VpnState.set(VpnState.State.STOPPING)
    worker.execute { shutdown(error) }
  }

  private fun shutdown(error: String?) {
    server?.let {
      runCatching { it.closeService() }
      runCatching { it.close() }
    }
    server = null
    runCatching { tun?.close() }
    tun = null
    network.stop()
    running = false
    VpnState.set(VpnState.State.STOPPED, error)
    Handler(Looper.getMainLooper()).post {
      stopForeground(STOP_FOREGROUND_REMOVE)
      stopSelf()
    }
  }

  override fun onRevoke() = stopVpn("VPN отключён системой или другим приложением")

  override fun onDestroy() {
    if (VpnState.state != VpnState.State.STOPPED) worker.execute { shutdown(null) }
    worker.shutdown()
    super.onDestroy()
  }

  // ---------- уведомление ----------

  private fun showNotification(text: String) {
    val manager = getSystemService(NotificationManager::class.java)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      manager.createNotificationChannel(
        NotificationChannel(CHANNEL, "VPN", NotificationManager.IMPORTANCE_LOW),
      )
    }
    val open = PendingIntent.getActivity(
      this, 0,
      Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_IMMUTABLE,
    )
    val stop = PendingIntent.getService(
      this, 0,
      Intent(this, BoxVpnService::class.java).setAction(ACTION_STOP),
      PendingIntent.FLAG_IMMUTABLE,
    )
    val notification: Notification = NotificationCompat.Builder(this, CHANNEL)
      .setSmallIcon(R.drawable.ic_vpn)
      .setContentTitle("ShredingerBox")
      .setContentText(text)
      .setContentIntent(open)
      .addAction(0, "Отключить", stop)
      .setOngoing(true)
      .setOnlyAlertOnce(true)
      .setShowWhen(false)
      .build()
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
      // systemExempted положен только VPN; без TUN ядро - обычный локальный прокси.
      val type = if (proxyPort > 0) ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE
        else ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED
      startForeground(NOTIFICATION_ID, notification, type)
    } else {
      startForeground(NOTIFICATION_ID, notification)
    }
  }

  // ---------- TUN ----------

  override fun openTun(options: TunOptions): Int {
    if (prepare(this) != null) error("нет разрешения на VPN")
    val builder = Builder().setSession("ShredingerBox").setMtu(options.mtu)
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) builder.setMetered(false)

    options.inet4Address.forEach { builder.addAddress(it.address(), it.prefix()) }
    options.inet6Address.forEach { builder.addAddress(it.address(), it.prefix()) }

    if (options.autoRoute) {
      if (options.dnsMode.value != Libbox.DNSModeDisabled) {
        options.dnsServerAddress.forEach { builder.addDnsServer(it) }
      }
      if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
        var any4 = false
        options.inet4RouteAddress.forEach { builder.addRoute(it.toIpPrefix()); any4 = true }
        if (!any4 && options.inet4Address.hasNext()) builder.addRoute("0.0.0.0", 0)
        var any6 = false
        options.inet6RouteAddress.forEach { builder.addRoute(it.toIpPrefix()); any6 = true }
        if (!any6 && options.inet6Address.hasNext()) builder.addRoute("::", 0)
        options.inet4RouteExcludeAddress.forEach { builder.excludeRoute(it.toIpPrefix()) }
        options.inet6RouteExcludeAddress.forEach { builder.excludeRoute(it.toIpPrefix()) }
      } else {
        options.inet4RouteRange.forEach { builder.addRoute(it.address(), it.prefix()) }
        options.inet6RouteRange.forEach { builder.addRoute(it.address(), it.prefix()) }
      }
      options.includePackage.forEach { pkg ->
        runCatching { builder.addAllowedApplication(pkg) }
      }
      options.excludePackage.forEach { pkg ->
        runCatching { builder.addDisallowedApplication(pkg) }
      }
    }

    val pfd = builder.establish() ?: error("Android не дал создать VPN-интерфейс")
    tun = pfd
    return pfd.fd
  }

  /** Сокеты самого ядра идут мимо VPN, иначе трафик к серверу зациклится. */
  override fun usePlatformAutoDetectInterfaceControl() = true

  override fun autoDetectInterfaceControl(fd: Int) {
    protect(fd)
  }

  // ---------- сеть ----------

  override fun startDefaultInterfaceMonitor(listener: InterfaceUpdateListener) = network.setListener(listener)

  override fun closeDefaultInterfaceMonitor(listener: InterfaceUpdateListener) = network.setListener(null)

  override fun localDNSTransport(): LocalDNSTransport = resolver

  override fun getInterfaces(): NetworkInterfaceIterator {
    val system = NetworkInterface.getNetworkInterfaces().toList()
    val result = mutableListOf<BoxNetworkInterface>()
    @Suppress("DEPRECATION")
    for (net in connectivity.allNetworks) {
      val link = connectivity.getLinkProperties(net) ?: continue
      val caps = connectivity.getNetworkCapabilities(net) ?: continue
      val iface = system.find { it.name == link.interfaceName } ?: continue
      result += BoxNetworkInterface().apply {
        name = link.interfaceName
        index = iface.index
        runCatching { mtu = iface.mtu }
        dnsServer = StringArray(link.dnsServers.mapNotNull { it.hostAddress })
        gateway = StringArray(
          link.routes.filter { it.destination.prefixLength == 0 }
            .mapNotNull { it.gateway }
            .filterNot { it.isAnyLocalAddress }
            .mapNotNull { it.hostAddress },
        )
        addresses = StringArray(
          iface.interfaceAddresses.map {
            val host = if (it.address is Inet6Address) {
              Inet6Address.getByAddress(it.address.address).hostAddress
            } else {
              it.address.hostAddress
            }
            "$host/${it.networkPrefixLength}"
          },
        )
        type = when {
          caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> Libbox.InterfaceTypeWIFI
          caps.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> Libbox.InterfaceTypeCellular
          caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> Libbox.InterfaceTypeEthernet
          else -> Libbox.InterfaceTypeOther
        }
        var f = 0
        if (caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) {
          f = OsConstants.IFF_UP or OsConstants.IFF_RUNNING
        }
        if (iface.isLoopback) f = f or OsConstants.IFF_LOOPBACK
        if (iface.isPointToPoint) f = f or OsConstants.IFF_POINTOPOINT
        if (iface.supportsMulticast()) f = f or OsConstants.IFF_MULTICAST
        flags = f
        metered = !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED)
      }
    }
    return object : NetworkInterfaceIterator {
      val it = result.iterator()
      override fun hasNext() = it.hasNext()
      override fun next(): BoxNetworkInterface = it.next()
    }
  }

  /** Чьё соединение - понадобится для правил по приложениям. */
  override fun findConnectionOwner(
    ipProtocol: Int,
    sourceAddress: String,
    sourcePort: Int,
    destinationAddress: String,
    destinationPort: Int,
  ): ConnectionOwner {
    val owner = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      val uid = connectivity.getConnectionOwnerUid(
        ipProtocol,
        InetSocketAddress(sourceAddress, sourcePort),
        InetSocketAddress(destinationAddress, destinationPort),
      )
      if (uid == Process.INVALID_UID) error("владелец соединения не найден")
      ConnectionOwner().also { it.userId = uid }
    } else {
      Libbox.findConnectionOwner(ipProtocol, sourceAddress, sourcePort, destinationAddress, destinationPort)
    }
    val packages = packageManager.getPackagesForUid(owner.userId)?.toList().orEmpty()
    owner.userName = packages.firstOrNull() ?: ""
    owner.setAndroidPackageNames(StringArray(packages))
    return owner
  }

  // ---------- команды ядра ----------

  override fun serviceStop() = stopVpn(null)

  override fun serviceReload() {
    server?.startOrReloadService(configFile(this).readText(), OverrideOptions())
  }

  override fun getSystemProxyStatus() = SystemProxyStatus()

  override fun setSystemProxyEnabled(enabled: Boolean) {}

  override fun triggerNativeCrash() {}

  override fun writeDebugMessage(message: String) {
    Log.d(TAG, message)
  }

  override fun connectSSHAgent() = -1

  // ---------- не нужно на телефоне без root ----------

  override fun underNetworkExtension() = false
  override fun includeAllNetworks() = false
  override fun readWIFIState(): WIFIState? = null
  override fun clearDNSCache() {}
  override fun sendNotification(notification: BoxNotification) {}
  override fun cancelNotification(identifier: String, typeID: Int) {}
  override fun startNeighborMonitor(listener: NeighborUpdateListener?) {}
  override fun closeNeighborMonitor(listener: NeighborUpdateListener?) {}
  override fun registerMyInterface(name: String?) {}
  override fun usePlatformShell() = false
  override fun checkPlatformShell() = error("не поддерживается")
  override fun openShellSession(
    user: PlatformUser?,
    command: String?,
    environ: StringIterator?,
    term: String?,
    rows: Int,
    cols: Int,
  ): ShellSession = error("не поддерживается")
  override fun lookupUser(username: String?): PlatformUser = error("не поддерживается")
  override fun lookupSFTPServer(): String = error("не поддерживается")
  override fun readSystemSSHHostKey(): String = error("не поддерживается")
  override fun tailscaleHostname() = ""
  override fun usePlatformBridge() = false
  override fun createBridge(options: BridgeOptions?): BridgeSession = error("не поддерживается")

  // ---------- помощники ----------

  /** Ошибки Android-стороны пишем в тот же журнал, что и ядро. */
  private fun appendLog(line: String) {
    runCatching { logFile(this).appendText("$line\n") }
  }

  private class StringArray(list: List<String>) : StringIterator {
    private val it = list.iterator()
    override fun len() = 0
    override fun hasNext() = it.hasNext()
    override fun next(): String = it.next()
  }

  private inline fun RoutePrefixIterator.forEach(action: (RoutePrefix) -> Unit) {
    while (hasNext()) action(next())
  }

  private inline fun StringIterator.forEach(action: (String) -> Unit) {
    while (hasNext()) action(next())
  }

  @androidx.annotation.RequiresApi(Build.VERSION_CODES.TIRAMISU)
  private fun RoutePrefix.toIpPrefix() =
    android.net.IpPrefix(java.net.InetAddress.getByName(address()), prefix())
}
