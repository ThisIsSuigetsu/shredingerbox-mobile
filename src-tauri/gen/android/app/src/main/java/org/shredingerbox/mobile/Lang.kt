package org.shredingerbox.mobile

import android.content.Context
import java.io.File

/**
 * Язык текстов Android-части: уведомление, плитка, ошибки. Выбирается в настройках
 * приложения; лежит в файле, потому что служба живёт в своём процессе «:vpn» и
 * работает без открытого приложения. До выбора - по языку системы.
 */
object Lang {
  private fun file(ctx: Context) = File(ctx.filesDir, "language")

  fun set(ctx: Context, lang: String) = file(ctx).writeText(lang)

  fun en(ctx: Context): Boolean {
    val saved = runCatching { file(ctx).readText().trim() }.getOrNull()
    if (!saved.isNullOrEmpty()) return saved == "en"
    return ctx.resources.configuration.locales[0].language !in setOf("ru", "uk", "be", "kk")
  }

  /** Строка на текущем языке. */
  fun t(ctx: Context, ru: String, en: String) = if (en(ctx)) en else ru
}
