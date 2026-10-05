package com.tmw.companion

import android.database.sqlite.SQLiteDatabase
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.util.zip.ZipInputStream

class RecoveryExportTest {
    @Test fun exportRetainsPhoneOnlyPendingRejectedAndDeletedRecords() {
        val db=SQLiteDatabase.create(null)
        try {
            // Fixture represents every durable table without requiring a PC or credentials.
            for(table in RecoveryExport.tables) {
                db.execSQL("CREATE TABLE $table(id INTEGER,json TEXT)")
                db.execSQL("INSERT INTO $table VALUES(1,?)",arrayOf("日本語・offline-$table"))
            }
            val out=ByteArrayOutputStream()
            RecoveryExport.write(db,out,JSONObject().put("tmw-local-cfi-proof","epubcfi(/6/2)"))
            val entries=mutableMapOf<String,String>()
            ZipInputStream(out.toByteArray().inputStream()).use {zip ->
                while(true) {val entry=zip.nextEntry?:break;entries[entry.name]=zip.readBytes().toString(Charsets.UTF_8)}
            }
            assertEquals(RecoveryExport.tables.size+2,entries.size)
            for(table in RecoveryExport.tables) {
                assertEquals("日本語・offline-$table",JSONArray(entries["tables/$table.json"]).getJSONObject(0).getString("json"))
                assertEquals(1L,db.compileStatement("SELECT count(*) FROM $table").simpleQueryForLong())
            }
            assertTrue(JSONObject(entries["manifest.json"]).getBoolean("complete"))
            assertFalse(JSONObject(entries["manifest.json"]).getBoolean("includesCredentials"))
            assertEquals("epubcfi(/6/2)",JSONObject(entries["reader-settings.json"]).getString("tmw-local-cfi-proof"))
        } finally {db.close()}
    }
    @Test fun incompatibleSyncCannotAcknowledgeQueuedPhoneChanges() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32)
        try {
            val store=UserSyncStorage(db,{ns},{JSONObject().put("protocolVersion",99)},false)
            store.historyRecord(JSONObject().put("ns",ns).put("fields",JSONObject().put("surface","猫").put("dictionaryId","fixture")))
            val before=db.compileStatement("SELECT count(*) FROM user_queue").simpleQueryForLong()
            assertTrue(before>0)
            try {store.sync();fail("Expected incompatible protocol")} catch(e:IllegalArgumentException) {
                assertTrue(e.message!!.contains("Incompatible PC sync protocol"))
                assertTrue(e.message!!.contains("retained"))
            }
            assertEquals(before,db.compileStatement("SELECT count(*) FROM user_queue").simpleQueryForLong())
            assertEquals(1L,db.compileStatement("SELECT count(*) FROM lookup_history").simpleQueryForLong())
        } finally {db.close()}
    }
}
