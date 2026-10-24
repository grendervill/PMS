package dev.pms.vault

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  private var multicastLock: WifiManager.MulticastLock? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    instance = this
    applySecureScreen(isSecureScreenEnabled())
    val wifi = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
    multicastLock = wifi?.createMulticastLock("pms-mdns")?.apply {
      setReferenceCounted(false)
      acquire()
    }
  }

  override fun onResume() {
    super.onResume()
    instance = this
    applySecureScreen(isSecureScreenEnabled())
  }

  override fun onDestroy() {
    if (instance === this) instance = null
    multicastLock?.release()
    multicastLock = null
    super.onDestroy()
  }

  /** Called from Rust after the vault setting changes. */
  fun setSecureScreen(enabled: Boolean) {
    getSharedPreferences(PREFS, Context.MODE_PRIVATE)
      .edit()
      .putBoolean(SECURE_SCREEN, enabled)
      .apply()
    runOnUiThread {
      applySecureScreen(enabled)
    }
  }

  private fun isSecureScreenEnabled(): Boolean =
    getSharedPreferences(PREFS, Context.MODE_PRIVATE).getBoolean(SECURE_SCREEN, false)

  private fun applySecureScreen(enabled: Boolean) {
    if (enabled) {
      window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    } else {
      window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }
  }

  companion object {
    @Volatile
    private var instance: MainActivity? = null

    @JvmStatic
    fun current(): MainActivity? = instance

    private const val PREFS = "pms.android"
    private const val SECURE_SCREEN = "secure_screen"
  }
}
