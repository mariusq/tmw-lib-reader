package com.tmw.companion

import android.app.Activity
import android.content.Context
import android.content.SharedPreferences
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.junit.Test
import org.junit.Assert.*

/** Isolated test preferences; never writes production pairing, reader, or dictionary records. */
class ConnectionStorageTest {
    private class StorageActivity(private val context: Context): Activity() {
        override fun getSharedPreferences(name: String, mode: Int): SharedPreferences =
            context.getSharedPreferences("phase3-test-$name", mode)
    }
    @Test fun encryptedCredentialSurvivesNativeRecreationAndRejectsUnsafeEndpoints() {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        lateinit var activity: StorageActivity
        InstrumentationRegistry.getInstrumentation().runOnMainSync { activity = StorageActivity(context) }
        val first = ConnectionPlugin(activity)
        val save = ConnectionPlugin::class.java.getDeclaredMethod("save", JSONObject::class.java).apply { isAccessible = true }
        val load = ConnectionPlugin::class.java.getDeclaredMethod("load").apply { isAccessible = true }
        val endpoint = ConnectionPlugin::class.java.getDeclaredMethod("endpoint", String::class.java).apply { isAccessible = true }
        val token = "ab".repeat(32)
        val value = JSONObject().put("url", "https://pc.test.ts.net").put("token", token).put("deviceId", "test-only")
        save.invoke(first, value)
        val prefs = activity.getSharedPreferences("private-connection-v1", Context.MODE_PRIVATE)
        assertFalse(prefs.all.toString().contains(token))
        val reloaded = load.invoke(ConnectionPlugin(activity)) as JSONObject
        assertEquals(token, reloaded.getString("token"))
        assertEquals("https://pc.test.ts.net", endpoint.invoke(first, "https://pc.test.ts.net/"))
        for (unsafe in listOf("http://pc.test.ts.net", "https://example.com", "https://pc.test.ts.net/path", "https://user@pc.test.ts.net", "https://pc.test.ts.net?path=secret", "https://pc.test.ts.net:8443")) {
            try { endpoint.invoke(first, unsafe); fail("Unsafe endpoint accepted") } catch (expected: java.lang.reflect.InvocationTargetException) { assertTrue(expected.cause is IllegalArgumentException) }
        }
        // Remove only these explicitly isolated test preferences; production credentials remain intact.
        assertTrue(prefs.edit().clear().commit())
    }
}

