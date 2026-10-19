package com.tmw.companion

import android.database.sqlite.SQLiteDatabase
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream
import java.util.zip.ZipInputStream

/** Generated in-memory fixtures; no live catalog, downloads or source EPUBs. */
class ReadingActivityStorageTest {
    private val ns="a".repeat(32)
    private val book="b".repeat(32)
    private val version="sha256-"+"c".repeat(64)
    private fun db():SQLiteDatabase=SQLiteDatabase.create(null).apply {
        execSQL("CREATE TABLE settings(k TEXT PRIMARY KEY,v TEXT)")
        execSQL("CREATE TABLE catalog(ns TEXT,gen TEXT,id TEXT,json TEXT)")
        execSQL("CREATE TABLE downloads(ns TEXT,id TEXT,version TEXT)")
        execSQL("INSERT INTO downloads VALUES(?,?,?)",arrayOf<Any>(ns,book,this@ReadingActivityStorageTest.version))
    }
    private fun token(store:ReadingActivityStorage)=store.command(JSONObject().put("action","activityStart").put("ns",ns)).getString("token")
    private fun record(store:ReadingActivityStorage,token:String,event:JSONObject)=store.command(JSONObject().put("action","activityRecord").put("ns",ns).put("id",book).put("title","Fixture").put("version",version).put("token",token).put("event",event))
    private fun event(id:String,kind:String)=JSONObject().put("id",id).put("kind",kind).put("day","2026-10-14").put("occurredAt",System.currentTimeMillis())
    private fun summary(store:ReadingActivityStorage,end:String="2026-10-14")=store.command(JSONObject().put("action","activitySummary").put("ns",ns).put("startDay","2026-10-01").put("endDay",end).put("selectedDay","2026-10-14").put("weekDay","2026-10-12").put("weekStart",0).put("nowSeconds",2000000000))
    private fun milliseconds(db:SQLiteDatabase)=db.compileStatement("SELECT coalesce(sum(milliseconds),0) FROM activity_totals WHERE ns='$ns'").simpleQueryForLong()
    @Test fun duplicateOverlapCharactersPauseAndHistoryIndependence() {
        val db=db();try {
            val store=ReadingActivityStorage(db,{ns},{error("offline")},false);val token=token(store)
            val first=event("one","time").put("startMs",10000).put("endMs",20000).put("milliseconds",10000)
            record(store,token,first);record(store,token,first)
            record(store,token,event("two","time").put("startMs",15000).put("endMs",25000).put("milliseconds",10000))
            assertEquals(15000L,milliseconds(db))
            record(store,token,event("fractional-clock","time").put("startMs",30000).put("endMs",39999).put("milliseconds",10000))
            assertEquals(24999L,milliseconds(db))
            val saved=JSONObject(db.rawQuery("SELECT json FROM activity_events WHERE id LIKE 'fractional-clock:%'",null).use {it.moveToFirst();it.getString(0)})
            assertEquals(9999L,saved.getLong("milliseconds"))
            for((id,a,b) in listOf(Triple("a",0,100),Triple("b",40,140),Triple("c",0,100)))record(store,token,event(id,"characters").put("spine",0).put("startOffset",a).put("endOffset",b))
            assertEquals(140L,db.compileStatement("SELECT sum(characters) FROM activity_totals WHERE ns='$ns'").simpleQueryForLong())
            // No history table exists: tracker lookup events are fully independent.
            record(store,token,event("lookup1","lookup"));record(store,token,event("lookup2","lookup"))
            assertEquals(2L,db.compileStatement("SELECT sum(lookups) FROM activity_totals WHERE ns='$ns'").simpleQueryForLong())
            store.command(JSONObject().put("action","activitySettings").put("enabled",false))
            record(store,token,event("lookup3","lookup"));assertEquals(2L,db.compileStatement("SELECT sum(lookups) FROM activity_totals WHERE ns='$ns'").simpleQueryForLong())
        } finally {db.close()}
    }
    @Test fun interruptedDeliveryRetainsQueueAndCanonicalReplayIsIdempotent() {
        val db=db();try {
            val ledger=JSONArray();var failResponse=true
            val store=ReadingActivityStorage(db,{ns},{body ->
                assertEquals(setOf("catalogId","epoch","cursor","reset","operations"),body.keys().asSequence().toSet())
                val ops=body.getJSONArray("operations");val ack=JSONArray()
                for(i in 0 until ops.length()) {val op=ops.getJSONObject(i);assertTrue(op.getLong("milliseconds")<=op.getLong("endMs")-op.getLong("startMs"));ack.put(op.getString("id"));if((0 until ledger.length()).none {ledger.getJSONObject(it).getString("id")==op.getString("id")})ledger.put(JSONObject(op.toString()).put("seq",ledger.length()+1))}
                if(failResponse) {failResponse=false;throw java.io.IOException("Interrupted after PC commit")}
                JSONObject().put("protocolVersion",1).put("catalogId",ns).put("epoch","e".repeat(32)).put("cursor",ledger.length()).put("events",ledger).put("acknowledged",ack).put("rejected",JSONArray()).put("hasMore",false)
            },false)
            record(store,token(store),event("time","time").put("startMs",10000).put("endMs",19999).put("milliseconds",10000))
            try {store.sync();fail("expected interruption")}catch(_:java.io.IOException){}
            assertEquals(1L,db.compileStatement("SELECT count(*) FROM activity_events WHERE pending=1").simpleQueryForLong())
            store.sync();store.sync();assertEquals(9999L,milliseconds(db));assertEquals(1,ledger.length())
            assertEquals(0L,store.status().getLong("pending"))
            assertEquals(0L,db.compileStatement("SELECT count(*) FROM activity_events WHERE pending=1").simpleQueryForLong())
        } finally {db.close()}
    }
    @Test fun clearInvalidatesOldSessionsAndRemoteResetCannotResurrectPendingActivity() {
        val db=db();try {
            var epoch="e".repeat(32)
            val store=ReadingActivityStorage(db,{ns},{body ->
                if(!body.isNull("reset"))epoch=body.getString("reset")
                JSONObject().put("protocolVersion",1).put("catalogId",ns).put("epoch",epoch).put("cursor",0).put("events",JSONArray()).put("acknowledged",JSONArray()).put("rejected",JSONArray()).put("hasMore",false)
            },false)
            store.sync();val old=token(store)
            record(store,old,event("old","lookup"));store.command(JSONObject().put("action","activityClear").put("ns",ns))
            record(store,old,event("stale-completion","lookup"));assertEquals(0,summary(store).getJSONArray("days").length())
            store.sync();record(store,token(store),event("new-pending","lookup"))
            epoch="f".repeat(32);store.sync();assertEquals(0,summary(store).getJSONArray("days").length())
            assertEquals(0L,db.compileStatement("SELECT count(*) FROM activity_events").simpleQueryForLong())
        } finally {db.close()}
    }
    @Test fun streakGapsAllHistoryAndRecoveryExportKeepPendingReset() {
        val db=db();try {
            val store=ReadingActivityStorage(db,{ns},{error("offline")},false);val token=token(store)
            for((index,day) in listOf("2026-10-10","2026-10-11","2026-10-13").withIndex())for(i in 0..5)record(store,token,event("$index-$i","time").put("day",day).put("startMs",(index*100000+i*10000).toLong()).put("endMs",(index*100000+(i+1)*10000).toLong()).put("milliseconds",10000))
            assertEquals(1,summary(store).getInt("streak"));assertEquals(2,summary(store).getInt("longestStreak"));assertEquals(0,summary(store,"2026-10-15").getInt("streak"))
            store.command(JSONObject().put("action","activityClear").put("ns",ns))
            // Export the real activity schemas and simple unrelated fixtures.
            for(table in RecoveryExport.tables.filter {!it.startsWith("activity_") && it !in setOf("settings","catalog","downloads")})db.execSQL("CREATE TABLE $table(id INTEGER,json TEXT)")
            val out=ByteArrayOutputStream();RecoveryExport.write(db,out,JSONObject().put("tmw-home-metric","characters"))
            val entries=mutableMapOf<String,String>();ZipInputStream(out.toByteArray().inputStream()).use {zip ->while(true) {val entry=zip.nextEntry?:break;entries[entry.name]=zip.readBytes().toString(Charsets.UTF_8)}}
            val state=JSONArray(entries["tables/activity_state.json"]).getJSONObject(0)
            assertFalse(state.isNull("reset"));assertTrue(state.getString("token")!=token)
            assertTrue(JSONObject(entries["manifest.json"]).getBoolean("complete"))
            assertEquals("characters",JSONObject(entries["reader-settings.json"]).getString("tmw-home-metric"))
        } finally {db.close()}
    }
}
