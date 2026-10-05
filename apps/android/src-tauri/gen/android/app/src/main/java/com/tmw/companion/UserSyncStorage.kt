package com.tmw.companion

import android.database.sqlite.SQLiteDatabase
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Durable local-first changes. Credentials and transport remain in the native plugin. */
internal class UserSyncStorage(private val db:SQLiteDatabase,
    private val namespace:()->String?, private val request:(JSONObject)->JSONObject,
    automatic:Boolean=true) {
    private val lock=Any()
    private val busy=AtomicBoolean(false)
    private val worker=Executors.newSingleThreadScheduledExecutor {r ->Thread(r,"tmw-user-sync").apply {isDaemon=true}}
    @Volatile private var message="Ready"
    @Volatile private var nextRetry=0L
    @Volatile private var lastSuccess=0L
    private var failures=0
    init {
        db.execSQL("CREATE TABLE IF NOT EXISTS user_records(ns TEXT,book TEXT,kind TEXT,entity TEXT,version TEXT,json TEXT NOT NULL,PRIMARY KEY(ns,book,kind,entity,version))")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_queue(n INTEGER PRIMARY KEY AUTOINCREMENT,ns TEXT NOT NULL,operation TEXT NOT NULL)")
        db.execSQL("CREATE INDEX IF NOT EXISTS user_queue_namespace ON user_queue(ns,n)")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_sync(ns TEXT PRIMARY KEY,cursor INTEGER NOT NULL DEFAULT 0,counter INTEGER NOT NULL DEFAULT 0)")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_rejected(ns TEXT,id TEXT PRIMARY KEY,json TEXT NOT NULL)")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_device(k INTEGER PRIMARY KEY,counter INTEGER NOT NULL)")
        db.execSQL("INSERT OR IGNORE INTO user_device SELECT 1,coalesce(max(counter),0) FROM user_sync")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_epoch(ns TEXT PRIMARY KEY,epoch TEXT NOT NULL)")
        db.execSQL("CREATE TABLE IF NOT EXISTS user_provisional(ns TEXT,id TEXT PRIMARY KEY,json TEXT NOT NULL,until_cursor INTEGER NOT NULL)")
        historyInitialize()
        if(automatic)worker.scheduleWithFixedDelay({if(db.isOpen && namespace()!=null && System.currentTimeMillis()>=nextRetry)start()},10,30,TimeUnit.SECONDS)
    }
    private fun transaction(block:()->Unit) {db.beginTransaction();try {block();db.setTransactionSuccessful()}finally {db.endTransaction()}}
    private fun valid(value:String):String {require(value.matches(Regex("[a-f0-9]{32}"))) {"Invalid catalog identity"};return value}
    private fun version(value:String):String {require(value.matches(Regex("sha256-[a-f0-9]{64}"))) {"A verified EPUB version is required"};return value}
    private fun recordVersion(row:JSONObject)=if(row.isNull("contentVersion"))"" else row.optString("contentVersion")
    private fun count(ns:String):Int=db.rawQuery("SELECT count(*) FROM user_queue WHERE ns=?",arrayOf(ns)).use {it.moveToFirst();it.getInt(0)}
    private fun applyFields(record:JSONObject,operation:JSONObject) {
        val fields=record.optJSONObject("fields")?:JSONObject();val patch=operation.optJSONObject("fields")?:JSONObject()
        patch.keys().forEach {key ->fields.put(key,if(patch.isNull(key)&&key in setOf("note","sentence"))"" else patch.get(key))}
        record.put("fields",fields)
        if(operation.optBoolean("deleted"))record.put("deleted",true)
    }
    fun state(args:JSONObject):JSONObject=synchronized(lock) {
        val ns=valid(args.getString("ns"));val book=valid(args.getString("id"));val ver=if(args.isNull("version"))"" else version(args.getString("version"))
        val offset=args.optInt("offset",0);require(offset in 0..1_000_000)
        val rows=linkedMapOf<String,JSONObject>()
        val selected=linkedSetOf<String>();var more=false
        val keys="SELECT entity,version FROM user_records WHERE ns=? AND book=? AND kind='passage' UNION SELECT json_extract(operation,'$.entityId'),coalesce(json_extract(operation,'$.contentVersion'),'') FROM user_queue WHERE ns=? AND json_extract(operation,'$.bookId')=? AND json_extract(operation,'$.kind')='passage' UNION SELECT json_extract(json,'$.entityId'),coalesce(json_extract(json,'$.contentVersion'),'') FROM user_provisional WHERE ns=? AND json_extract(json,'$.bookId')=? AND json_extract(json,'$.kind')='passage' UNION SELECT json_extract(json,'$.operation.entityId'),coalesce(json_extract(json,'$.operation.contentVersion'),'') FROM user_rejected WHERE ns=? AND json_extract(json,'$.operation.bookId')=? AND json_extract(json,'$.operation.kind')='passage' ORDER BY 1,2 LIMIT 51 OFFSET ?"
        db.rawQuery(keys,arrayOf(ns,book,ns,book,ns,book,ns,book,offset.toString())).use {c ->while(c.moveToNext()) {if(selected.size==50){more=true;break};selected.add("passage:"+c.getString(0)+":"+(if(c.isNull(1))"" else c.getString(1)))}}
        db.rawQuery("SELECT json FROM user_records WHERE ns=? AND book=? AND kind IN ('progress','passage')",arrayOf(ns,book)).use {c ->while(c.moveToNext()) {val row=JSONObject(c.getString(0));val key=row.getString("kind")+":"+row.optString("entityId")+":"+recordVersion(row);if(key in selected || (ver.isNotEmpty()&&row.getString("kind")=="progress"&&recordVersion(row)==ver))rows[key]=row}}
        fun overlay(op:JSONObject) {
            if(op.optString("kind") !in setOf("progress","passage"))return
            if(op.optString("bookId")!=book || (op.optString("kind")=="progress" && (ver.isEmpty()||recordVersion(op)!=ver)))return
            val key=op.getString("kind")+":"+op.optString("entityId")+":"+recordVersion(op)
            if(op.getString("kind")=="passage" && key !in selected)return
            val row=rows.getOrPut(key) {JSONObject().put("bookId",book).put("kind",op.getString("kind")).put("entityId",op.optString("entityId")).put("contentVersion",op.opt("contentVersion")?:JSONObject.NULL).put("deleted",false)}
            applyFields(row,op)
        }
        db.rawQuery("SELECT json FROM user_provisional WHERE ns=? AND json_extract(json,'$.bookId')=? ORDER BY cast(json_extract(json,'$.sequence') AS INTEGER)",arrayOf(ns,book)).use {c ->while(c.moveToNext())overlay(JSONObject(c.getString(0)))}
        db.rawQuery("SELECT json FROM user_rejected WHERE ns=? AND json_extract(json,'$.operation.bookId')=? AND coalesce(json_extract(json,'$.active'),1)=1 ORDER BY rowid",arrayOf(ns,book)).use {c ->while(c.moveToNext()) {val op=JSONObject(c.getString(0)).optJSONObject("operation");if(op!=null)overlay(op)}}
        db.rawQuery("SELECT operation FROM user_queue WHERE ns=? AND json_extract(operation,'$.bookId')=? ORDER BY n",arrayOf(ns,book)).use {c ->while(c.moveToNext()) {val op=JSONObject(c.getString(0))
            overlay(op)
        }}
        val passages=JSONArray();var progress:JSONObject?=null
        rows.values.filter {!it.optBoolean("deleted")}.forEach {if(it.getString("kind")=="progress")progress=it else passages.put(it)}
        JSONObject().put("progress",progress?:JSONObject.NULL).put("passages",passages).put("pending",count(ns)).put("next",if(more)offset+50 else JSONObject.NULL)
    }
    fun save(args:JSONObject):JSONObject=synchronized(lock) {
        val ns=valid(args.getString("ns"));val book=valid(args.getString("id"));val ver=if(args.isNull("version"))"" else version(args.getString("version"))
        val kind=args.getString("kind");require(kind=="progress"||kind=="passage")
        val entity=if(kind=="passage")valid(args.optString("entityId").ifEmpty {UUID.randomUUID().toString().replace("-","")}) else ""
        val fields=args.optJSONObject("fields")?:JSONObject();val allowed=if(kind=="progress")setOf("locationCfi") else setOf("surface","headword","reading","locationCfi","sentence","note")
        if(ver.isEmpty()) {require(kind=="passage"&&args.has("entityId"));require(fields.keys().asSequence().all {it in setOf("sentence","note")});require(db.rawQuery("SELECT 1 FROM user_records WHERE ns=? AND book=? AND kind='passage' AND entity=?",arrayOf(ns,book,entity)).use {it.moveToFirst()}) {"Unknown passage"}}
        fields.keys().forEach {key ->require(key in allowed && ((fields.isNull(key)&&key in setOf("headword","reading","note","sentence"))||fields.get(key) is String));val limit=when(key) {"surface"->256;"sentence"->4000;"note"->2000;"locationCfi"->4096;else->256};require(fields.optString(key).length<=limit);if(key=="locationCfi" && fields.getString(key).isNotEmpty())require(fields.getString(key).startsWith("epubcfi("))}
        require(!args.optBoolean("deleted")||kind=="passage");require(fields.length()>0||args.optBoolean("deleted"))
        val operation=JSONObject().put("id",UUID.randomUUID().toString()).put("bookId",book).put("kind",kind).put("contentVersion",if(ver.isEmpty())JSONObject.NULL else ver).put("fields",fields).put("deleted",args.optBoolean("deleted"))
        if(kind=="passage")operation.put("entityId",entity)
        require(operation.toString().toByteArray(Charsets.UTF_8).size<=60000) {"User change too large"}
        transaction {
            // A deliberate newer local edit supersedes this entity's rejected
            // overlay, while the complete rejected operation remains recoverable.
            db.execSQL("UPDATE user_rejected SET json=json_set(json,'$.active',json('false')) WHERE ns=? AND json_extract(json,'$.operation.bookId')=? AND json_extract(json,'$.operation.kind')=? AND coalesce(json_extract(json,'$.operation.entityId'),'')=? AND coalesce(json_extract(json,'$.operation.contentVersion'),'')=?",arrayOf(ns,book,kind,entity,ver))
            db.execSQL("INSERT OR IGNORE INTO user_sync(ns) VALUES(?)",arrayOf(ns))
            db.execSQL("UPDATE user_device SET counter=counter+1 WHERE k=1")
            val sequence=db.rawQuery("SELECT counter FROM user_device WHERE k=1",null).use {it.moveToFirst();it.getLong(0)}
            operation.put("sequence",sequence)
            db.execSQL("INSERT INTO user_queue(ns,operation) VALUES(?,?)",arrayOf(ns,operation.toString()))
        }
        nextRetry=0
        JSONObject().put("entityId",entity).put("operationId",operation.getString("id"))
    }
    fun status():JSONObject=synchronized(lock) {
        val ns=namespace();val count=if(ns==null)0 else db.rawQuery("SELECT count(*) FROM user_queue WHERE ns=?",arrayOf(ns)).use {it.moveToFirst();it.getInt(0)}
        val rejected=JSONArray();if(ns!=null)db.rawQuery("SELECT json FROM user_rejected WHERE ns=? ORDER BY rowid DESC LIMIT 50",arrayOf(ns)).use {while(it.moveToNext())rejected.put(JSONObject(it.getString(0)))}
        JSONObject().put("busy",busy.get()).put("pending",count).put("message",message).put("lastSuccess",lastSuccess).put("nextRetry",nextRetry).put("rejected",rejected)
    }
    fun start():JSONObject {
        if(busy.compareAndSet(false,true))worker.execute {try {sync();failures=0;nextRetry=0;lastSuccess=System.currentTimeMillis();message="Synced"}
            catch(e:Exception) {failures=minOf(failures+1,6);nextRetry=System.currentTimeMillis()+minOf(300000L,5000L*(1L shl failures));message=e.message?:"User sync unavailable"}
            finally {busy.set(false)}}
        return status()
    }
    /** Bounded run; a durable cursor commits with each acknowledged batch/page. */
    fun sync() {
        val ns=namespace()?:return
        message="Syncing progress, notes and lookup history"
        repeat(100) {
            val body=synchronized(lock) {
                db.execSQL("INSERT OR IGNORE INTO user_sync(ns) VALUES(?)",arrayOf(ns))
                val cursor=db.rawQuery("SELECT cursor FROM user_sync WHERE ns=?",arrayOf(ns)).use {it.moveToFirst();it.getLong(0)}
                val ops=JSONArray();var bytes=0
                db.rawQuery("SELECT operation FROM user_queue WHERE ns=? ORDER BY n LIMIT 16",arrayOf(ns)).use {c ->while(c.moveToNext()) {val raw=c.getString(0);val size=raw.toByteArray(Charsets.UTF_8).size;if(bytes+size>60000)break;ops.put(JSONObject(raw));bytes+=size}}
                val body=JSONObject().put("catalogId",ns).put("historyVersion",1).put("cursor",cursor).put("operations",ops)
                db.rawQuery("SELECT epoch FROM user_epoch WHERE ns=?",arrayOf(ns)).use {if(it.moveToFirst())body.put("epoch",it.getString(0))}
                body
            }
            val response=try {request(body)}catch(e:Exception) {if(e.message=="cursor_reset") {synchronized(lock) {transaction {
                // Old acknowledgments cannot retain an old epoch's watermark.
                // Preserve their text for explicit recovery; replay restored PC state.
                db.rawQuery("SELECT id,json FROM user_provisional WHERE ns=?",arrayOf(ns)).use {c ->while(c.moveToNext()) {
                    val archived=JSONObject().put("id",c.getString(0)).put("reason","catalog_restored").put("active",false).put("operation",JSONObject(c.getString(1)))
                    db.execSQL("INSERT OR REPLACE INTO user_rejected VALUES(?,?,?)",arrayOf(ns,c.getString(0),archived.toString()))
                }}
                db.execSQL("DELETE FROM user_provisional WHERE ns=?",arrayOf(ns))
                db.execSQL("DELETE FROM lookup_history WHERE ns=? AND deleted=0 AND entity NOT IN (SELECT json_extract(operation,'$.entityId') FROM user_queue WHERE ns=? AND json_extract(operation,'$.kind')='history')",arrayOf(ns,ns))
                db.rawQuery("SELECT json FROM lookup_history h WHERE ns=? AND deleted=1 AND NOT EXISTS(SELECT 1 FROM user_queue q WHERE q.ns=h.ns AND json_extract(q.operation,'$.kind')='history' AND json_extract(q.operation,'$.entityId')=h.entity AND json_extract(q.operation,'$.deleted')=1)",arrayOf(ns)).use {c ->while(c.moveToNext()) {
                    val old=JSONObject(c.getString(0));val tombstone=JSONObject().put("bookId",old.optString("bookId")).put("kind","history").put("entityId",old.getString("entityId")).put("contentVersion",old.opt("contentVersion")?:JSONObject.NULL).put("deleted",true).put("fields",JSONObject())
                    historyEnqueue(ns,tombstone)
                }}
                db.execSQL("UPDATE user_sync SET cursor=0 WHERE ns=?",arrayOf(ns));db.execSQL("DELETE FROM user_epoch WHERE ns=?",arrayOf(ns));db.execSQL("DELETE FROM user_records WHERE ns=?",arrayOf(ns))
            }};return@repeat};throw e}
            requireProtocol(response,3)
            require(response.getString("catalogId")==ns) {"PC catalog identity changed; local data retained"}
            val changes=response.getJSONArray("changes");require(changes.length()<=50)
            synchronized(lock) {transaction {
                val ack=response.getJSONArray("acknowledged");val rejected=response.getJSONArray("rejected")
                val delivered=mutableSetOf<String>();for(i in 0 until body.getJSONArray("operations").length())delivered.add(body.getJSONArray("operations").getJSONObject(i).getString("id"))
                fun remove(id:String) {require(id in delivered);db.execSQL("DELETE FROM user_queue WHERE ns=? AND json_extract(operation,'$.id')=?",arrayOf(ns,id))}
                val highWater=response.optLong("highWater",response.getLong("cursor"));require(highWater>=response.getLong("cursor"))
                for(i in 0 until ack.length()) {val id=ack.getString(i);val ops=body.getJSONArray("operations");for(j in 0 until ops.length())if(ops.getJSONObject(j).getString("id")==id)db.execSQL("INSERT OR REPLACE INTO user_provisional VALUES(?,?,?,?)",arrayOf<Any>(ns,id,ops.getJSONObject(j).toString(),highWater));remove(id)}
                for(i in 0 until rejected.length()) {val row=rejected.getJSONObject(i);val id=row.getString("id");val ops=body.getJSONArray("operations");for(j in 0 until ops.length())if(ops.getJSONObject(j).getString("id")==id)row.put("operation",ops.getJSONObject(j));row.put("active",true);remove(id);db.execSQL("INSERT OR REPLACE INTO user_rejected VALUES(?,?,?)",arrayOf(ns,id,row.toString()))}
                // A save may have committed while the request was in flight.
                db.execSQL("UPDATE user_rejected SET json=json_set(json,'$.active',json('false')) WHERE ns=? AND EXISTS(SELECT 1 FROM user_queue q WHERE q.ns=user_rejected.ns AND json_extract(q.operation,'$.sequence')>json_extract(user_rejected.json,'$.operation.sequence') AND json_extract(q.operation,'$.bookId')=json_extract(user_rejected.json,'$.operation.bookId') AND json_extract(q.operation,'$.kind')=json_extract(user_rejected.json,'$.operation.kind') AND coalesce(json_extract(q.operation,'$.entityId'),'')=coalesce(json_extract(user_rejected.json,'$.operation.entityId'),'') AND coalesce(json_extract(q.operation,'$.contentVersion'),'')=coalesce(json_extract(user_rejected.json,'$.operation.contentVersion'),''))",arrayOf(ns))
                for(i in 0 until changes.length()) {val row=changes.getJSONObject(i);val kind=row.getString("kind");require(kind in setOf("progress","passage","history"));val book=if(kind=="history" && row.optString("bookId").isEmpty())"" else valid(row.getString("bookId"))
                    val entity=if(kind!="progress")valid(row.getString("entityId")) else "";val ver=if(row.isNull("contentVersion"))"" else version(row.getString("contentVersion"))
                    db.execSQL("INSERT OR REPLACE INTO user_records VALUES(?,?,?,?,?,?)",arrayOf(ns,book,kind,entity,ver,row.toString()))
                    if(kind=="history")historyApply(ns,row,true)
                }
                val cursor=response.getLong("cursor");require(cursor>=body.getLong("cursor"));db.execSQL("UPDATE user_sync SET cursor=? WHERE ns=?",arrayOf<Any>(cursor,ns))
                db.execSQL("DELETE FROM user_provisional WHERE ns=? AND until_cursor<=?",arrayOf<Any>(ns,cursor))
                if(response.has("epoch"))db.execSQL("INSERT OR REPLACE INTO user_epoch VALUES(?,?)",arrayOf(ns,response.getString("epoch")))
                if(!response.getBoolean("hasMore") && count(ns)==0 && db.rawQuery("SELECT 1 FROM user_provisional WHERE ns=? LIMIT 1",arrayOf(ns)).use {!it.moveToFirst()})historyRetain(ns)
            }}
            if(!response.getBoolean("hasMore") && synchronized(lock) {count(ns)==0})return
            require(response.getJSONArray("acknowledged").length()>0 || response.getJSONArray("rejected").length()>0 || response.getLong("cursor")>body.getLong("cursor")) {"Sync made no progress"}
        }
        message="More changes pending; continuing automatically"
    }
    private val localHistoryNamespace="local-proof"
    private fun historyInitialize() {
        db.execSQL("CREATE TABLE IF NOT EXISTS lookup_history(ns TEXT NOT NULL,entity TEXT NOT NULL,book TEXT NOT NULL,json TEXT NOT NULL,search TEXT NOT NULL,stamp INTEGER NOT NULL,deleted INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(ns,entity))")
        db.execSQL("CREATE INDEX IF NOT EXISTS lookup_history_page ON lookup_history(ns,deleted,stamp DESC,entity)")
        db.execSQL("CREATE INDEX IF NOT EXISTS lookup_history_arrival ON lookup_history(stamp)")
        db.execSQL("CREATE TABLE IF NOT EXISTS lookup_settings(k TEXT PRIMARY KEY,v TEXT NOT NULL)")
        if(historySetting("capability","0")!="1")transaction {
            // Older clients advanced over filtered history; replay once without
            // discarding durable records, pending changes or provisional overlays.
            db.execSQL("UPDATE user_sync SET cursor=0")
            db.execSQL("INSERT OR REPLACE INTO lookup_settings VALUES('capability','1')")
        }
    }
    private fun historySetting(k:String,default:String):String=db.rawQuery("SELECT v FROM lookup_settings WHERE k=?",arrayOf(k)).use {if(it.moveToFirst())it.getString(0) else default}
    private fun historySettingsValue()=JSONObject().put("enabled",historySetting("enabled","true")=="true").put("retentionLimit",historySetting("limit","10000").toInt())
    /** Materialized bounded history keeps queries off the durable queue. Tombstones never resurrect. */
    private fun historyApply(ns:String,row:JSONObject,canonical:Boolean=false) {
        val entity=valid(row.getString("entityId"));val fields=row.optJSONObject("fields")?:JSONObject()
        val search=java.text.Normalizer.normalize(listOf("surface","headword","reading","sentence","dictionaryLabel").joinToString(" ") {fields.optString(it)},java.text.Normalizer.Form.NFKC).lowercase(java.util.Locale.ROOT)
        val value=JSONObject(row.toString()).put("ns",ns)
        val stamp=db.compileStatement("SELECT coalesce(max(stamp),0)+1 FROM lookup_history").simpleQueryForLong()
        db.execSQL("INSERT OR IGNORE INTO lookup_history(ns,entity,book,json,search,stamp,deleted) VALUES(?,?,?,?,?,?,?)",arrayOf<Any>(ns,entity,row.optString("bookId"),value.toString(),search,stamp,if(row.optBoolean("deleted"))1 else 0))
        if(canonical && !row.optBoolean("deleted"))db.execSQL("UPDATE lookup_history SET stamp=?,json=? WHERE ns=? AND entity=?",arrayOf<Any>(stamp,value.toString(),ns,entity))
        if(row.optBoolean("deleted"))db.execSQL("UPDATE lookup_history SET deleted=1 WHERE ns=? AND entity=?",arrayOf(ns,entity))
    }
    private fun historyEnqueue(ns:String,row:JSONObject) {
        if(ns==localHistoryNamespace)return
        db.execSQL("INSERT OR IGNORE INTO user_sync(ns) VALUES(?)",arrayOf(ns))
        db.execSQL("UPDATE user_device SET counter=counter+1 WHERE k=1")
        val seq=db.compileStatement("SELECT counter FROM user_device WHERE k=1").simpleQueryForLong()
        val op=JSONObject(row.toString()).put("id",UUID.randomUUID().toString()).put("sequence",seq)
        op.remove("ns");op.remove("localBookId")
        db.execSQL("INSERT INTO user_queue(ns,operation) VALUES(?,?)",arrayOf(ns,op.toString()))
        nextRetry=0
    }
    fun historyRecord(args:JSONObject):JSONObject=synchronized(lock) {
        if(historySetting("enabled","true")!="true")return@synchronized JSONObject().put("recorded",false)
        val ns=if(args.optString("ns").isEmpty())localHistoryNamespace else valid(args.getString("ns"))
        val book=if(ns==localHistoryNamespace)"" else args.optString("id").let {if(it.isEmpty())"" else valid(it)}
        val ver=args.optString("version").let {if(it.isEmpty()||it=="null")JSONObject.NULL else version(it)}
        val fields=JSONObject(args.getJSONObject("fields").toString())
        val limits=mapOf("surface" to 256,"headword" to 256,"reading" to 256,"dictionaryId" to 128,"dictionaryEntryId" to 128,"dictionaryLabel" to 256,"sentence" to 4000,"locationCfi" to 4096,"lookedUpAt" to 32)
        fields.keys().forEach {k ->require(k in limits);require(fields.isNull(k)||fields.get(k) is String);require(fields.optString(k).length<=limits.getValue(k))}
        require(fields.optString("surface").isNotBlank());require(fields.optString("dictionaryId").isNotBlank())
        if(!fields.has("lookedUpAt"))fields.put("lookedUpAt",(System.currentTimeMillis()/1000).toString())
        require(fields.getString("lookedUpAt").matches(Regex("[0-9]{1,16}")))
        val row=JSONObject().put("bookId",book).put("kind","history").put("entityId",UUID.randomUUID().toString().replace("-","")).put("contentVersion",ver).put("fields",fields).put("deleted",false)
        if(ns==localHistoryNamespace)row.put("localBookId",args.optString("localBookId").take(256))
        transaction {historyApply(ns,row);historyEnqueue(ns,row);historyRetain(ns)}
        JSONObject().put("recorded",true).put("entityId",row.getString("entityId"))
    }
    fun historyList(args:JSONObject):JSONObject=synchronized(lock) {
        val ns=namespace();val offset=args.optInt("offset",0);require(offset in 0..1_000_000)
        val q=java.text.Normalizer.normalize(args.optString("query").take(256),java.text.Normalizer.Form.NFKC).lowercase(java.util.Locale.ROOT)
        val rows=JSONArray();var more=false
        db.rawQuery("SELECT json FROM lookup_history WHERE (ns=? OR ns=?) AND deleted=0 AND instr(search,?)>0 ORDER BY CASE WHEN json_extract(json,'$.sequence') IS NULL THEN 1 ELSE 0 END DESC,stamp DESC,entity DESC LIMIT 51 OFFSET ?",arrayOf(ns?:"",localHistoryNamespace,q,offset.toString())).use {c ->while(c.moveToNext()) {if(rows.length()==50){more=true;break};rows.put(JSONObject(c.getString(0)))}}
        JSONObject().put("rows",rows).put("next",if(more)offset+50 else JSONObject.NULL).put("settings",historySettingsValue()).put("pending",if(ns==null)0 else count(ns))
    }
    private fun historyRemove(ns:String,entity:String) {
        val row=db.rawQuery("SELECT json FROM lookup_history WHERE ns=? AND entity=? AND deleted=0",arrayOf(ns,entity)).use {if(it.moveToFirst())JSONObject(it.getString(0)) else null}?:return
        val tombstone=JSONObject().put("bookId",row.optString("bookId")).put("kind","history").put("entityId",entity).put("contentVersion",row.opt("contentVersion")?:JSONObject.NULL).put("deleted",true).put("fields",JSONObject())
        db.execSQL("UPDATE lookup_history SET deleted=1 WHERE ns=? AND entity=?",arrayOf(ns,entity));historyEnqueue(ns,tombstone)
    }
    private fun historyRetain(ns:String) {
        val limit=historySetting("limit","10000").toInt();val ids=mutableListOf<String>()
        db.rawQuery("SELECT entity FROM lookup_history WHERE ns=? AND deleted=0 ORDER BY CASE WHEN json_extract(json,'$.sequence') IS NULL THEN 1 ELSE 0 END DESC,stamp DESC,entity DESC LIMIT -1 OFFSET ?",arrayOf(ns,limit.toString())).use {while(it.moveToNext())ids.add(it.getString(0))}
        ids.forEach {historyRemove(ns,it)}
    }
    fun historyDelete(args:JSONObject):JSONObject=synchronized(lock) {
        val ns=args.getString("ns");require(ns==localHistoryNamespace || ns==namespace());val entity=valid(args.getString("entityId"))
        transaction {historyRemove(ns,entity)};JSONObject()
    }
    fun historyClear():JSONObject=synchronized(lock) {
        val ids=mutableListOf<Pair<String,String>>()
        db.rawQuery("SELECT ns,entity FROM lookup_history WHERE (ns=? OR ns=?) AND deleted=0",arrayOf(namespace()?:"",localHistoryNamespace)).use {while(it.moveToNext())ids.add(it.getString(0) to it.getString(1))}
        transaction {ids.forEach {(ns,id)->historyRemove(ns,id)}};JSONObject().put("cleared",ids.size)
    }
    fun historySettings(args:JSONObject):JSONObject=synchronized(lock) {
        transaction {
            if(args.has("enabled")) {require(args.get("enabled") is Boolean);db.execSQL("INSERT OR REPLACE INTO lookup_settings VALUES('enabled',?)",arrayOf(args.getBoolean("enabled").toString()))}
            if(args.has("retentionLimit")) {val limit=args.getInt("retentionLimit");require(limit in 1..10000);db.execSQL("INSERT OR REPLACE INTO lookup_settings VALUES('limit',?)",arrayOf(limit.toString()));namespace()?.let {historyRetain(it)};historyRetain(localHistoryNamespace)}
        };historySettingsValue()
    }

}
