package com.tmw.companion

import android.database.sqlite.SQLiteDatabase
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class UserSyncStorageTest {
    @Test fun historyCapabilityUpgradeReplaysSkippedJournalOnce() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32)
        try {
            UserSyncStorage(db,{ns},{error("not used")},false)
            db.execSQL("INSERT INTO user_sync(ns,cursor) VALUES(?,99)",arrayOf(ns))
            db.execSQL("DELETE FROM lookup_settings WHERE k='capability'")
            UserSyncStorage(db,{ns},{error("not used")},false)
            assertEquals(0L,db.compileStatement("SELECT cursor FROM user_sync").simpleQueryForLong())
            db.execSQL("UPDATE user_sync SET cursor=75")
            UserSyncStorage(db,{ns},{error("not used")},false)
            assertEquals(75L,db.compileStatement("SELECT cursor FROM user_sync").simpleQueryForLong())
        } finally {db.close()}
    }
    @Test fun historyClearSurvivesRestoredEpochAndRequeuesTombstone() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32);var reset=false;val accepted=mutableListOf<JSONObject>()
        val store=UserSyncStorage(db,{ns},{body ->
            if(reset && body.optString("epoch")=="old")throw IllegalStateException("cursor_reset")
            val ops=body.getJSONArray("operations");val ack=JSONArray();val changes=JSONArray()
            for(i in 0 until ops.length()){val op=ops.getJSONObject(i);accepted.add(op);ack.put(op.getString("id"));changes.put(op)}
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("epoch",if(reset)"new" else "old").put("cursor",accepted.size).put("hasMore",false).put("acknowledged",ack).put("rejected",JSONArray()).put("changes",changes)
        },false)
        try {
            store.historyRecord(JSONObject().put("ns",ns).put("fields",JSONObject().put("surface","\u732b").put("dictionaryId","jmdict-eng:fixture")))
            store.sync();store.historyClear();store.sync();assertEquals(2,accepted.size)
            reset=true;store.sync();assertEquals(3,accepted.size);assertTrue(accepted.last().getBoolean("deleted"))
            assertEquals(0,store.historyList(JSONObject()).getJSONArray("rows").length())
        } finally {db.close()}
    }
    @Test fun historyOfflineRepeatedLookupRetentionDisabledAndSyncTombstones() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32);val book="b".repeat(32)
        val changes=linkedMapOf<String,JSONObject>();var interrupted=true;var deliveries=0
        val store=UserSyncStorage(db,{ns},{body ->
            assertEquals(1,body.getInt("historyVersion"))
            val ops=body.getJSONArray("operations");val ack=JSONArray()
            for(i in 0 until ops.length()) {val op=ops.getJSONObject(i);deliveries++;changes[op.getString("entityId")]=JSONObject(op.toString());ack.put(op.getString("id"))}
            if(interrupted)throw java.io.IOException("Response lost")
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("cursor",deliveries).put("hasMore",false).put("acknowledged",ack).put("rejected",JSONArray()).put("changes",JSONArray(changes.values.toList()))
        },false)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",JSONObject.NULL).put("fields",JSONObject().put("surface","\u732b").put("headword","\u732b").put("reading","\u306d\u3053").put("dictionaryId","jmdict-eng:fixture").put("dictionaryEntryId",JSONObject.NULL).put("sentence","\u732b\u304c\u3044\u308b"))
        try {
            val recordStart=System.nanoTime();store.historyRecord(args);store.historyRecord(args)
            android.util.Log.i("TMWHistoryTest","Two offline history commits micros="+((System.nanoTime()-recordStart)/1000))
            assertEquals(2,store.historyList(JSONObject().put("query","\u306d\u3053")).getJSONArray("rows").length())
            try {store.sync();fail("Expected transport loss")}catch(_:java.io.IOException){}
            interrupted=false;store.sync();assertEquals(4,deliveries)
            assertEquals(2,store.historyList(JSONObject()).getJSONArray("rows").length())
            assertEquals(0,store.state(JSONObject().put("ns",ns).put("id",book).put("version",JSONObject.NULL)).getJSONArray("passages").length())
            store.historySettings(JSONObject().put("retentionLimit",1));assertEquals(1,store.historyList(JSONObject()).getJSONArray("rows").length())
            store.sync();assertEquals(1,changes.values.count {!it.optBoolean("deleted")})
            store.historySettings(JSONObject().put("enabled",false));assertFalse(store.historyRecord(args).getBoolean("recorded"))
            assertEquals(1,store.historyClear().getInt("cleared"));store.sync();assertEquals(0,store.historyList(JSONObject()).getJSONArray("rows").length())
            store.historySettings(JSONObject().put("enabled",true));store.historyRecord(JSONObject(args.toString()).removeIdentityForProof())
            assertEquals("local-proof",store.historyList(JSONObject()).getJSONArray("rows").getJSONObject(0).getString("ns"))
            assertEquals(0,store.status().getInt("pending"))
        } finally {db.close()}
    }
    private fun JSONObject.removeIdentityForProof():JSONObject {remove("ns");remove("id");put("localBookId","proof-book");return this}

    @Test fun restoredEpochArchivesOldProvisionalWatermarkAndReplaysCanonical() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32);val book="b".repeat(32);val ver="sha256-"+"c".repeat(64)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",ver)
        val old=JSONObject().put("id","old-ack").put("sequence",1).put("bookId",book).put("kind","progress").put("contentVersion",ver).put("fields",JSONObject().put("locationCfi","epubcfi(/6/100)"))
        val store=UserSyncStorage(db,{ns},{body ->
            if(body.optString("epoch")=="old")throw IllegalStateException("cursor_reset")
            val canonical=JSONObject(old.toString()).put("fields",JSONObject().put("locationCfi","epubcfi(/6/20)"))
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("epoch","restored").put("cursor",20).put("highWater",20).put("hasMore",false).put("acknowledged",JSONArray()).put("rejected",JSONArray()).put("changes",JSONArray().put(canonical))
        },false)
        try {
            db.execSQL("INSERT INTO user_sync(ns,cursor) VALUES(?,50)",arrayOf(ns));db.execSQL("INSERT INTO user_epoch VALUES(?,'old')",arrayOf(ns))
            db.execSQL("INSERT INTO user_provisional VALUES(?,?,?,100)",arrayOf(ns,"old-ack",old.toString()))
            assertEquals("epubcfi(/6/100)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            store.sync()
            assertEquals("epubcfi(/6/20)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            assertEquals(0L,db.compileStatement("SELECT count(*) FROM user_provisional").simpleQueryForLong())
            val archived=store.status().getJSONArray("rejected").getJSONObject(0)
            assertEquals("catalog_restored",archived.getString("reason"));assertFalse(archived.getBoolean("active"))
            assertEquals("epubcfi(/6/100)",archived.getJSONObject("operation").getJSONObject("fields").getString("locationCfi"))
        } finally {db.close()}
    }
    @Test fun acknowledgedOverlaySurvivesRestartUntilPaginatedJournalCatchesUp() {
        val root=File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,"user-provisional-${System.nanoTime()}").apply {mkdirs()}
        val ns="a".repeat(32);val book="b".repeat(32);val ver="sha256-"+"c".repeat(64)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",ver)
        var interrupted=true
        val transport:(JSONObject)->JSONObject={body ->
            val first=body.getLong("cursor")==0L
            if(!first&&interrupted)throw java.io.IOException("Interrupted next pull page")
            val changes=JSONArray();val ack=JSONArray()
            if(first) {
                val ops=body.getJSONArray("operations");for(i in 0 until ops.length())ack.put(ops.getJSONObject(i).getString("id"))
                for(i in 1..50)changes.put(JSONObject().put("bookId",book).put("kind","progress").put("entityId","").put("contentVersion",ver).put("fields",JSONObject().put("locationCfi","epubcfi(/6/$i)")))
            } else changes.put(JSONObject().put("bookId",book).put("kind","progress").put("entityId","").put("contentVersion",ver).put("fields",JSONObject().put("locationCfi","epubcfi(/6/100)")))
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("cursor",if(first)50 else 100).put("highWater",100).put("hasMore",first).put("acknowledged",ack).put("rejected",JSONArray()).put("changes",changes)
        }
        var db=SQLiteDatabase.openOrCreateDatabase(File(root,"fixture.sqlite"),null);var store=UserSyncStorage(db,{ns},transport,false)
        try {
            store.save(JSONObject(args.toString()).put("kind","progress").put("fields",JSONObject().put("locationCfi","epubcfi(/6/100)")))
            try {store.sync();fail("Expected interrupted pull")}catch(_:java.io.IOException){}
            assertEquals(0,store.status().getInt("pending"));db.close()
            db=SQLiteDatabase.openOrCreateDatabase(File(root,"fixture.sqlite"),null);store=UserSyncStorage(db,{ns},transport,false)
            assertEquals("epubcfi(/6/100)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            assertEquals(1L,db.compileStatement("SELECT count(*) FROM user_provisional").simpleQueryForLong())
            interrupted=false;store.sync()
            assertEquals(0L,db.compileStatement("SELECT count(*) FROM user_provisional").simpleQueryForLong())
            assertEquals("epubcfi(/6/100)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
        } finally {if(db.isOpen)db.close();root.deleteRecursively()}
    }
    @Test fun deliberateNewSaveSupersedesRejectedOverlayWithoutErasingRecoveryCopy() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32);val book="b".repeat(32);val ver="sha256-"+"c".repeat(64)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",ver)
        var reject=true
        val store=UserSyncStorage(db,{ns},{body ->
            val ops=body.getJSONArray("operations");val ack=JSONArray();val rejected=JSONArray();val changes=JSONArray()
            for(i in 0 until ops.length()) {val op=ops.getJSONObject(i)
                if(reject)rejected.put(JSONObject().put("id",op.getString("id")).put("reason","source_replaced"))
                else {ack.put(op.getString("id"));changes.put(JSONObject(op.toString()).put("entityId",op.optString("entityId")))}
            }
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("cursor",if(reject)0 else 1).put("hasMore",false).put("acknowledged",ack).put("rejected",rejected).put("changes",changes)
        },false)
        try {
            store.save(JSONObject(args.toString()).put("kind","progress").put("fields",JSONObject().put("locationCfi","epubcfi(/6/2)")))
            store.sync();assertEquals("epubcfi(/6/2)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            store.save(JSONObject(args.toString()).put("kind","progress").put("fields",JSONObject().put("locationCfi","epubcfi(/6/4)")))
            reject=false;store.sync()
            assertEquals("epubcfi(/6/4)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            val archived=store.status().getJSONArray("rejected").getJSONObject(0)
            assertFalse(archived.getBoolean("active"));assertEquals("epubcfi(/6/2)",archived.getJSONObject("operation").getJSONObject("fields").getString("locationCfi"))
        } finally {db.close()}
    }
    @Test fun rejectedChangesRemainLocalLegacyNotesAndPagination() {
        val db=SQLiteDatabase.create(null);val ns="a".repeat(32);val book="b".repeat(32);val ver="sha256-"+"c".repeat(64)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",ver)
        val store=UserSyncStorage(db,{ns},{body ->
            val rejected=JSONArray();val ops=body.getJSONArray("operations")
            for(i in 0 until ops.length())rejected.put(JSONObject().put("id",ops.getJSONObject(i).getString("id")).put("reason","source_replaced"))
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("cursor",0).put("hasMore",false).put("acknowledged",JSONArray()).put("rejected",rejected).put("changes",JSONArray())
        },false)
        try {
            store.save(JSONObject(args.toString()).put("kind","progress").put("fields",JSONObject().put("locationCfi","epubcfi(/6/2)")))
            store.sync();assertEquals(0,store.status().getInt("pending"));assertEquals(1,store.status().getJSONArray("rejected").length())
            assertEquals("epubcfi(/6/2)",store.state(args).getJSONObject("progress").getJSONObject("fields").getString("locationCfi"))
            for(i in 1..55) {val entity="%032x".format(i);val row=JSONObject().put("bookId",book).put("entityId",entity).put("kind","passage").put("contentVersion",JSONObject.NULL).put("fields",JSONObject().put("surface","猫").put("note","legacy $i")).put("deleted",false)
                db.execSQL("INSERT INTO user_records VALUES(?,?,?,?,?,?)",arrayOf(ns,book,"passage",entity,"",row.toString()))}
            val offlineArgs=JSONObject(args.toString()).put("version",JSONObject.NULL)
            val first=store.state(offlineArgs);assertEquals(50,first.getJSONArray("passages").length());assertEquals(50,first.getInt("next"));assertTrue(first.isNull("progress"))
            val second=store.state(JSONObject(offlineArgs.toString()).put("offset",50));assertEquals(5,second.getJSONArray("passages").length());assertTrue(second.isNull("next"))
            store.save(JSONObject(offlineArgs.toString()).put("kind","passage").put("entityId","%032x".format(1)).put("fields",JSONObject().put("note","edited legacy")))
            assertEquals("edited legacy",store.state(offlineArgs).getJSONArray("passages").getJSONObject(0).getJSONObject("fields").getString("note"))
        } finally {db.close()}
    }
    @Test fun localChangesRestartOrderedRetryAndVersionIsolation() {
        val root=File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir,"user-sync-${System.nanoTime()}").apply {mkdirs()}
        val ns="a".repeat(32);val book="b".repeat(32);val ver="sha256-"+"c".repeat(64)
        val args=JSONObject().put("ns",ns).put("id",book).put("version",ver)
        val accepted=linkedMapOf<String,JSONObject>();var interrupted=true;var deliveries=0
        val transport:(JSONObject)->JSONObject={body ->
            val ack=JSONArray();val ops=body.getJSONArray("operations");var prior=0L
            for(i in 0 until ops.length()) {val op=ops.getJSONObject(i);assertTrue(op.getLong("sequence")>prior);prior=op.getLong("sequence");deliveries++
                accepted.putIfAbsent(op.getString("id"),JSONObject(op.toString()).put("entityId",op.optString("entityId")))
                ack.put(op.getString("id"))
            }
            if(interrupted)throw java.io.IOException("Response lost after server commit")
            val changes=JSONArray();accepted.values.forEach {changes.put(it)}
            JSONObject().put("protocolVersion",3).put("catalogId",ns).put("cursor",accepted.size).put("hasMore",false).put("acknowledged",ack).put("rejected",JSONArray()).put("changes",changes)
        }
        var db=SQLiteDatabase.openOrCreateDatabase(File(root,"fixture.sqlite"),null)
        var store=UserSyncStorage(db,{ns},transport,false)
        try {
            val saved=store.save(JSONObject(args.toString()).put("kind","passage").put("fields",JSONObject().put("surface","猫").put("note","offline")))
            val entity=saved.getString("entityId")
            store.save(JSONObject(args.toString()).put("kind","progress").put("fields",JSONObject().put("locationCfi","epubcfi(/6/2)")))
            assertEquals(2,store.status().getInt("pending"))
            try {store.sync();fail("Response interruption accepted")}catch(_:java.io.IOException){}
            assertEquals(2,store.status().getInt("pending"));db.close()
            db=SQLiteDatabase.openOrCreateDatabase(File(root,"fixture.sqlite"),null);store=UserSyncStorage(db,{ns},transport,false)
            assertEquals("offline",store.state(args).getJSONArray("passages").getJSONObject(0).getJSONObject("fields").getString("note"))
            interrupted=false;store.sync();assertEquals(4,deliveries);assertEquals(2,accepted.size);assertEquals(0,store.status().getInt("pending"))
            assertEquals(1,store.state(args).getJSONArray("passages").length())
            assertTrue(store.state(JSONObject(args.toString()).put("version","sha256-"+"d".repeat(64))).isNull("progress"))
            store.save(JSONObject(args.toString()).put("kind","passage").put("entityId",entity).put("fields",JSONObject().put("note","changed")))
            assertEquals("猫",store.state(args).getJSONArray("passages").getJSONObject(0).getJSONObject("fields").getString("surface"))
            store.save(JSONObject(args.toString()).put("kind","passage").put("entityId",entity).put("deleted",true))
            assertEquals(0,store.state(args).getJSONArray("passages").length())
        } finally {if(db.isOpen)db.close();root.deleteRecursively()}
    }
}
