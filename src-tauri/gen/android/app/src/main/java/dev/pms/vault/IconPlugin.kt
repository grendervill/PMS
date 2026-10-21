package dev.pms.vault

import android.app.Activity
import android.content.ComponentName
import android.content.pm.PackageManager
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class IconArgs {
  var icon: String? = null
}

@TauriPlugin
class IconPlugin(private val activity: Activity) : Plugin(activity) {
  @Command
  fun setIcon(invoke: Invoke) {
    val icon = invoke.parseArgs(IconArgs::class.java).icon ?: "green-lock"
    val aliases = mapOf(
      "green-lock" to ".IconGreen",
      "blue-shield" to ".IconBlue",
      "gold-key" to ".IconGold",
    )
    val chosen = aliases[icon]
    if (chosen == null) {
      invoke.reject("Unknown icon: $icon")
      return
    }
    val packageManager = activity.packageManager
    for (alias in aliases.values) {
      val component = ComponentName(activity.packageName, activity.packageName + alias)
      val state = if (alias == chosen) {
        PackageManager.COMPONENT_ENABLED_STATE_ENABLED
      } else {
        PackageManager.COMPONENT_ENABLED_STATE_DISABLED
      }
      packageManager.setComponentEnabledSetting(
        component,
        state,
        PackageManager.DONT_KILL_APP,
      )
    }
    invoke.resolve(JSObject())
  }
}
