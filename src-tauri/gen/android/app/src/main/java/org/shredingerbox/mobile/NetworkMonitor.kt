package org.shredingerbox.mobile

import android.net.ConnectivityManager
import android.net.LinkProperties
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import io.nekohasekai.libbox.InterfaceUpdateListener
import java.net.NetworkInterface

/**
 * Следит за настоящей сетью телефона (Wi-Fi или мобильной) под VPN и сообщает ядру,
 * через какой интерфейс выходить наружу. Подход взят из sing-box-for-android:
 * registerDefaultNetworkCallback с Android 9 возвращает сам VPN, поэтому там
 * нужен requestNetwork / registerBestMatchingNetworkCallback.
 */
class NetworkMonitor(private val connectivity: ConnectivityManager) {
  @Volatile
  var defaultNetwork: Network? = null
    private set

  @Volatile
  private var listener: InterfaceUpdateListener? = null
  private var registered = false
  private val handler = Handler(HandlerThread("NetworkMonitor").apply { start() }.looper)

  private val callback = object : ConnectivityManager.NetworkCallback() {
    override fun onAvailable(network: Network) {
      defaultNetwork = network
      notifyListener()
    }

    override fun onLinkPropertiesChanged(network: Network, linkProperties: LinkProperties) {
      if (network == defaultNetwork) notifyListener()
    }

    override fun onLost(network: Network) {
      if (network == defaultNetwork) {
        defaultNetwork = null
        notifyListener()
      }
    }
  }

  fun start() {
    if (registered) return
    val request = NetworkRequest.Builder()
      .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
      .addCapability(NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED)
      .build()
    when {
      Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
        connectivity.registerBestMatchingNetworkCallback(request, callback, handler)
      Build.VERSION.SDK_INT >= Build.VERSION_CODES.P ->
        connectivity.requestNetwork(request, callback, handler)
      Build.VERSION.SDK_INT >= Build.VERSION_CODES.O ->
        connectivity.registerDefaultNetworkCallback(callback, handler)
      else -> connectivity.registerDefaultNetworkCallback(callback)
    }
    registered = true
    defaultNetwork = connectivity.activeNetwork
  }

  fun stop() {
    if (!registered) return
    runCatching { connectivity.unregisterNetworkCallback(callback) }
    registered = false
    defaultNetwork = null
  }

  fun setListener(listener: InterfaceUpdateListener?) {
    this.listener = listener
    notifyListener()
  }

  private fun notifyListener() {
    val listener = listener ?: return
    val network = defaultNetwork
    if (network == null) {
      listener.updateDefaultInterface("", -1, false, false)
      return
    }
    // Свойства сети появляются не сразу после onAvailable — подождём до секунды.
    repeat(10) {
      val name = connectivity.getLinkProperties(network)?.interfaceName
      val index = name?.let { runCatching { NetworkInterface.getByName(it).index }.getOrNull() }
      if (name != null && index != null) {
        listener.updateDefaultInterface(name, index, false, false)
        return
      }
      Thread.sleep(100)
    }
  }
}
