package com.tmw.companion

import android.app.Activity
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Plugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONObject
import java.net.URL
import java.security.KeyStore
import java.util.concurrent.Executors
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.net.ssl.HttpsURLConnection

/** Credentials never enter JavaScript, logs, or WebView storage. */
@TauriPlugin
class ConnectionPlugin(private val activity: Activity): Plugin(activity) {
    private val storage by lazy { MobileStorage(activity) { path, body, match ->
        val saved = load() ?: error("Pair this phone first.")
        val c = URL(endpoint(saved.getString("url")) + path).openConnection() as HttpsURLConnection
        c.connectTimeout=8000; c.readTimeout=15000; c.instanceFollowRedirects=false
        c.setRequestProperty("Authorization", "Bearer ${saved.getString("token")}")
        c.setRequestProperty("Accept-Encoding", "identity")
        if (match != null) c.setRequestProperty("If-Match", match)
        if (body != null) {
            c.requestMethod="POST"; c.doOutput=true; c.setRequestProperty("Content-Type","application/json")
            val bytes=body.toString().toByteArray(Charsets.UTF_8); c.setFixedLengthStreamingMode(bytes.size)
            c.outputStream.use { it.write(bytes) }
        }
        c
    } }
    @Command
    fun mobile(invoke: Invoke) { storage.command(invoke) }
    private val worker = Executors.newSingleThreadExecutor()
    private val prefs = activity.getSharedPreferences("private-connection-v1", Activity.MODE_PRIVATE)
    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val old = store.getKey("tmw-connection-v1", null)
        if (old != null) return old as SecretKey
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder("tmw-connection-v1", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    private fun save(value: JSONObject) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val encrypted = cipher.doFinal(value.toString().toByteArray(Charsets.UTF_8))
        check(prefs.edit().putString("iv", Base64.encodeToString(cipher.iv, Base64.NO_WRAP))
            .putString("data", Base64.encodeToString(encrypted, Base64.NO_WRAP)).commit())
    }
    private fun load(): JSONObject? {
        val data = prefs.getString("data", null) ?: return null
        val iv = Base64.decode(prefs.getString("iv", null) ?: error("Credential unavailable"), Base64.NO_WRAP)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv)) }
        return JSONObject(String(cipher.doFinal(Base64.decode(data, Base64.NO_WRAP)), Charsets.UTF_8))
    }
    private fun endpoint(input: String): String {
        require(input.length <= 256)
        val url = URL(input.trim().trimEnd('/'))
        require(url.protocol == "https" && url.host.endsWith(".ts.net") && url.host.length > 7 && url.userInfo == null && url.query == null && url.ref == null && (url.path == "" || url.path == "/") && (url.port == -1 || url.port == 443))
        return "https://${url.host.lowercase()}"
    }
    private fun request(base: String, path: String, token: String? = null, body: JSONObject? = null): JSONObject {
        val c = URL(base + path).openConnection() as HttpsURLConnection
        try {
            c.connectTimeout = 8000; c.readTimeout = 8000; c.instanceFollowRedirects = false
            c.setRequestProperty("Accept", "application/json")
            if (token != null) c.setRequestProperty("Authorization", "Bearer $token")
            if (body != null) {
                c.requestMethod = "POST"; c.doOutput = true
                c.setRequestProperty("Content-Type", "application/json")
                val bytes = body.toString().toByteArray(Charsets.UTF_8)
                c.setFixedLengthStreamingMode(bytes.size)
                c.outputStream.use { it.write(bytes) }
            }
            val status = c.responseCode
            if (status == 401) error("Device unauthorized or revoked. Pair again on the PC.")
            if (status == 403) error("Pairing code expired, denied, or already used.")
            require(status in 200..299) { "PC request failed ($status)." }
            val bytes = c.inputStream.use { it.readBytesBounded(16384) }
            val response = JSONObject(String(bytes, Charsets.UTF_8))
            require(response.getInt("protocolVersion") == 1) { "Unsupported PC protocol. Update both apps." }
            return response
        } finally { c.disconnect() }
    }
    private fun java.io.InputStream.readBytesBounded(limit: Int): ByteArray {
        val out = java.io.ByteArrayOutputStream(); val buffer = ByteArray(2048)
        while (true) { val n = read(buffer); if (n < 0) break; require(out.size() + n <= limit); out.write(buffer, 0, n) }
        return out.toByteArray()
    }
    @Command
    fun connection(invoke: Invoke) {
        val args = invoke.getArgs()
        worker.execute {
            try {
                val result = JSObject()
                when (args.getString("action")) {
                    "pair" -> {
                        val base = endpoint(args.getString("url")); val code = args.getString("code").trim().lowercase()
                        require(code.matches(Regex("[a-f0-9]{12}"))) { "Enter the 12-character PC pairing code." }
                        val response = request(base, "/v1/pair", body = JSONObject().put("code", code).put("name", "Android phone"))
                        val token = response.getString("token"); require(token.matches(Regex("[a-f0-9]{64}")))
                        save(JSONObject().put("url", base).put("token", token).put("deviceId", response.getString("deviceId")))
                        result.put("paired", true); result.put("url", base); result.put("message", "Paired with PC")
                    }
                    "check" -> {
                        val saved = load() ?: error("Pair this phone first.")
                        request(endpoint(saved.getString("url")), "/v1/status", saved.getString("token"))
                        result.put("paired", true); result.put("url", saved.getString("url")); result.put("message", "PC connected · protocol 1")
                    }
                    "forget" -> {
                        check(prefs.edit().clear().commit())
                        result.put("paired", false); result.put("url", ""); result.put("message", "Local credential forgotten. Revoke this device on the PC too.")
                    }
                    "status" -> {
                        val saved = load(); result.put("paired", saved != null); result.put("url", saved?.getString("url") ?: "")
                        result.put("message", if (saved != null) "Paired · connection not checked" else "Not paired")
                    }
                    else -> error("Unknown connection action")
                }
                invoke.resolve(result)
            } catch (e: Exception) {
                // Avoid logging exception objects containing URLs, credentials, or response bodies.
                invoke.reject(when (e) {
                    is javax.net.ssl.SSLException -> "Private HTTPS certificate could not be verified."
                    is java.io.IOException -> "PC unavailable. Check Tailscale, desktop service, and PC awake state."
                    else -> e.message ?: "Connection failed. Pair again if credential storage is unavailable."
                })
            }
        }
    }
}
