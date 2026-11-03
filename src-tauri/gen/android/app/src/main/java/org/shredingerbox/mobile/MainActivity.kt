package org.shredingerbox.mobile

import android.os.Bundle
import android.util.Log
import androidx.activity.enableEdgeToEdge
import io.nekohasekai.libbox.Libbox

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Проверка, что ядро подключилось: видно в `adb logcat -s ShredingerBox`.
    Log.i("ShredingerBox", "sing-box ${Libbox.version()}")
  }
}
