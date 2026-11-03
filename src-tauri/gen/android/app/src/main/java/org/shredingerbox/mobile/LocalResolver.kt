package org.shredingerbox.mobile

import android.net.DnsResolver
import android.os.Build
import android.os.CancellationSignal
import android.system.ErrnoException
import androidx.annotation.RequiresApi
import io.nekohasekai.libbox.ExchangeContext
import io.nekohasekai.libbox.LocalDNSTransport
import java.net.InetAddress
import java.net.UnknownHostException
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors

/**
 * Системный DNS телефона для DNS-сервера `local` в конфиге: ядро спрашивает
 * через Android DnsResolver в обход VPN. Переписано с sing-box-for-android без корутин.
 */
class LocalResolver(private val network: NetworkMonitor) : LocalDNSTransport {
  private val executor = Executors.newCachedThreadPool()

  override fun raw(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

  @RequiresApi(Build.VERSION_CODES.Q)
  override fun exchange(ctx: ExchangeContext, message: ByteArray) {
    val defaultNetwork = network.defaultNetwork ?: error("нет сети")
    await(ctx) { signal, done ->
      DnsResolver.getInstance().rawQuery(
        defaultNetwork, message, DnsResolver.FLAG_NO_RETRY, executor, signal,
        object : DnsResolver.Callback<ByteArray> {
          override fun onAnswer(answer: ByteArray, rcode: Int) {
            if (rcode == 0) ctx.rawSuccess(answer) else ctx.errorCode(rcode)
            done(null)
          }

          override fun onError(error: DnsResolver.DnsException) = done(handleError(ctx, error))
        },
      )
    }
  }

  override fun lookup(ctx: ExchangeContext, network: String, domain: String) {
    val defaultNetwork = this.network.defaultNetwork ?: error("нет сети")
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
      val answer = try {
        defaultNetwork.getAllByName(domain)
      } catch (e: UnknownHostException) {
        ctx.errorCode(RCODE_NXDOMAIN)
        return
      }
      ctx.success(answer.mapNotNull { it.hostAddress }.joinToString("\n"))
      return
    }
    await(ctx) { signal, done ->
      val callback = object : DnsResolver.Callback<List<InetAddress>> {
        override fun onAnswer(answer: List<InetAddress>, rcode: Int) {
          if (rcode == 0) {
            ctx.success(answer.mapNotNull { it.hostAddress }.joinToString("\n"))
          } else {
            ctx.errorCode(rcode)
          }
          done(null)
        }

        override fun onError(error: DnsResolver.DnsException) = done(handleError(ctx, error))
      }
      val resolver = DnsResolver.getInstance()
      val flags = DnsResolver.FLAG_NO_RETRY
      when {
        network.endsWith("4") ->
          resolver.query(defaultNetwork, domain, DnsResolver.TYPE_A, flags, executor, signal, callback)
        network.endsWith("6") ->
          resolver.query(defaultNetwork, domain, DnsResolver.TYPE_AAAA, flags, executor, signal, callback)
        else -> resolver.query(defaultNetwork, domain, flags, executor, signal, callback)
      }
    }
  }

  /** Ошибки сокета отдаём ядру кодом, остальные пробрасываем. */
  @RequiresApi(Build.VERSION_CODES.Q)
  private fun handleError(ctx: ExchangeContext, error: DnsResolver.DnsException): Throwable? {
    val cause = error.cause
    if (cause is ErrnoException) {
      ctx.errnoCode(cause.errno)
      return null
    }
    return error
  }

  /** Запускает асинхронный запрос и ждёт ответа или отмены со стороны ядра. */
  private fun await(
    ctx: ExchangeContext,
    query: (CancellationSignal, (Throwable?) -> Unit) -> Unit,
  ) {
    val latch = CountDownLatch(1)
    val signal = CancellationSignal()
    var failure: Throwable? = null
    ctx.onCancel {
      signal.cancel()
      latch.countDown()
    }
    query(signal) {
      failure = it
      latch.countDown()
    }
    latch.await()
    failure?.let { throw it }
  }

  private companion object {
    const val RCODE_NXDOMAIN = 3
  }
}
