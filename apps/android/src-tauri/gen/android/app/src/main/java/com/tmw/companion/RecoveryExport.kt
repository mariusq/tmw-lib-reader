package com.tmw.companion

import android.database.Cursor
import android.database.sqlite.SQLiteDatabase
import android.util.JsonWriter
import org.json.JSONObject
import java.io.OutputStream
import java.io.OutputStreamWriter
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** Portable logical export, never a live SQLite/WAL copy or a sync payload. */
internal object RecoveryExport {
    val tables = listOf("settings", "catalog", "downloads", "user_records", "user_queue",
        "user_sync", "user_rejected", "user_device", "user_epoch", "user_provisional",
        "lookup_history", "lookup_settings")

    fun write(db: SQLiteDatabase, output: OutputStream, webState: JSONObject) {
        require(webState.toString().toByteArray(Charsets.UTF_8).size <= 2_000_000) { "Reader settings exceed export limit" }
        ZipOutputStream(output).use { zip ->
            // One consistent snapshot, including offline operations and their ordering counters.
            db.beginTransactionNonExclusive()
            try {
                for (table in tables) {
                    zip.putNextEntry(ZipEntry("tables/$table.json"))
                    val writer = JsonWriter(OutputStreamWriter(zip, Charsets.UTF_8))
                    writer.beginArray()
                    db.rawQuery("SELECT * FROM $table", null).use { cursor ->
                        while (cursor.moveToNext()) {
                            writer.beginObject()
                            for (i in 0 until cursor.columnCount) {
                                writer.name(cursor.getColumnName(i))
                                when (cursor.getType(i)) {
                                    Cursor.FIELD_TYPE_NULL -> writer.nullValue()
                                    Cursor.FIELD_TYPE_INTEGER -> writer.value(cursor.getLong(i))
                                    Cursor.FIELD_TYPE_FLOAT -> writer.value(cursor.getDouble(i))
                                    Cursor.FIELD_TYPE_STRING -> writer.value(cursor.getString(i))
                                    else -> error("Unsupported export column")
                                }
                            }
                            writer.endObject()
                        }
                    }
                    writer.endArray(); writer.flush(); zip.closeEntry()
                }
            } finally { db.endTransaction() }
            zip.putNextEntry(ZipEntry("reader-settings.json"))
            zip.write(webState.toString().toByteArray(Charsets.UTF_8)); zip.closeEntry()
            // Written last: a truncated archive must never be treated as complete.
            zip.putNextEntry(ZipEntry("manifest.json"))
            zip.write(JSONObject().put("format", "tmw-phone-recovery").put("version", 1)
                .put("appVersion", BuildConfig.VERSION_NAME).put("createdAt", System.currentTimeMillis())
                .put("complete", true).put("includesEpubs", false).put("includesCredentials", false)
                .toString().toByteArray(Charsets.UTF_8))
            zip.closeEntry()
        }
    }
}
