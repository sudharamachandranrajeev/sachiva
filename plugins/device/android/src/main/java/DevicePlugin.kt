package dev.sachiva.device

import android.app.Activity
import android.app.AlarmManager
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

@InvokeArg
class KeyArgs {
    var mayCreate: Boolean = false
    var key: String? = null
}

// The database key is random, and is stored only wrapped (AES-GCM) by a key
// that never leaves the Android Keystore.
@TauriPlugin
class DevicePlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs = activity.getSharedPreferences("sachiva-device", Context.MODE_PRIVATE)
    private val alias = "sachiva-db-wrap"

    private fun wrapKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    private fun store(hex: String) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, wrapKey())
        val sealed = cipher.iv + cipher.doFinal(hex.toByteArray(Charsets.US_ASCII))
        // commit, not apply: the key must be on disk before the database is made.
        check(prefs.edit().putString("wrapped", Base64.encodeToString(sealed, Base64.NO_WRAP)).commit())
    }

    private fun load(): String? {
        val sealed = prefs.getString("wrapped", null)?.let { Base64.decode(it, Base64.NO_WRAP) } ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, wrapKey(), GCMParameterSpec(128, sealed, 0, 12))
        return String(cipher.doFinal(sealed, 12, sealed.size - 12), Charsets.US_ASCII)
    }

    @Command
    fun dbKey(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        try {
            val result = JSObject()
            val existing = load()
            if (existing != null) {
                result.put("key", existing)
                result.put("created", false)
            } else if (args.mayCreate) {
                val raw = ByteArray(32).also { SecureRandom().nextBytes(it) }
                val hex = raw.joinToString("") { "%02x".format(it) }
                store(hex)
                result.put("key", hex)
                result.put("created", true)
            } else {
                result.put("created", false)
            }
            invoke.resolve(result)
        } catch (e: Exception) {
            // Never replace a key that cannot be read: the database would be lost.
            invoke.reject("the stored database key cannot be read: ${e.message}")
        }
    }

    @Command
    fun setDbKey(invoke: Invoke) {
        val args = invoke.parseArgs(KeyArgs::class.java)
        val hex = args.key
        if (hex == null || hex.length != 64) {
            invoke.reject("a key is 64 hex digits")
            return
        }
        try {
            store(hex)
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("could not store the key: ${e.message}")
        }
    }

    @Command
    fun alarmAccess(invoke: Invoke) {
        val alarms = activity.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val power = activity.getSystemService(Context.POWER_SERVICE) as PowerManager
        val result = JSObject()
        result.put("exact", Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms())
        result.put("batteryUnrestricted", power.isIgnoringBatteryOptimizations(activity.packageName))
        invoke.resolve(result)
    }

    @Command
    fun openAlarmSettings(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            open(Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, Uri.parse("package:" + activity.packageName)))
        }
        invoke.resolve()
    }

    @Command
    fun openBatterySettings(invoke: Invoke) {
        val ask = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + activity.packageName))
        if (!open(ask)) open(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
        invoke.resolve()
    }

    private fun open(intent: Intent): Boolean =
        try {
            activity.startActivity(intent)
            true
        } catch (e: ActivityNotFoundException) {
            false
        }
}
