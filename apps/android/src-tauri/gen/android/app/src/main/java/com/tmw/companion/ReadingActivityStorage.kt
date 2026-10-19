package com.tmw.companion

import android.database.sqlite.SQLiteDatabase
import org.json.JSONArray
import org.json.JSONObject
import java.text.SimpleDateFormat
import java.util.Locale
import java.util.TimeZone
import java.util.UUID
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** App-owned ledger, independent of history/progress queues. All mutations are transactional. */
internal class ReadingActivityStorage(private val db:SQLiteDatabase,private val namespace:()->String?,
    private val request:(JSONObject)->JSONObject,automatic:Boolean=true) {
    private val lock=Any()
    private val busy=AtomicBoolean(false)
    private val worker=Executors.newSingleThreadScheduledExecutor { r -> Thread(r,"tmw-activity-sync").apply {isDaemon=true} }
    @Volatile private var message="Local activity"
    init {
        ensureSchema(db)
        if(automatic)worker.scheduleWithFixedDelay({if(db.isOpen && namespace()!=null)start()},10,30,TimeUnit.SECONDS)
    }
    companion object {
        fun ensureSchema(db:SQLiteDatabase) {
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_state(ns TEXT PRIMARY KEY,epoch TEXT,cursor INTEGER NOT NULL DEFAULT 0,reset TEXT,token TEXT NOT NULL)")
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_events(ns TEXT NOT NULL,id TEXT NOT NULL,seq INTEGER NOT NULL DEFAULT 0,pending INTEGER NOT NULL,json TEXT NOT NULL,PRIMARY KEY(ns,id))")
            db.execSQL("CREATE INDEX IF NOT EXISTS activity_delivery ON activity_events(ns,pending,seq)")
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_totals(ns TEXT NOT NULL,day TEXT NOT NULL,book TEXT NOT NULL,title TEXT NOT NULL,milliseconds INTEGER NOT NULL DEFAULT 0,characters INTEGER NOT NULL DEFAULT 0,lookups INTEGER NOT NULL DEFAULT 0,PRIMARY KEY(ns,day,book))")
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_time(ns TEXT NOT NULL,a INTEGER NOT NULL,b INTEGER NOT NULL,PRIMARY KEY(ns,a))")
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_coverage(ns TEXT NOT NULL,book TEXT NOT NULL,version TEXT NOT NULL,spine INTEGER NOT NULL,a INTEGER NOT NULL,b INTEGER NOT NULL,PRIMARY KEY(ns,book,version,spine,a))")
            db.execSQL("CREATE TABLE IF NOT EXISTS activity_settings(k TEXT PRIMARY KEY,v TEXT NOT NULL)")
            db.execSQL("INSERT OR IGNORE INTO activity_settings VALUES('enabled','true')")
        }
    }
    private fun dayNumber(day:String):Long {
        require(day.matches(Regex("[0-9]{4}-[0-9]{2}-[0-9]{2}"))) {"Invalid activity calendar date"}
        val parser=SimpleDateFormat("yyyy-MM-dd",Locale.ROOT).apply {isLenient=false;timeZone=TimeZone.getTimeZone("UTC")}
        return requireNotNull(parser.parse(day)).time/86400000
    }
    private fun uuid()=UUID.randomUUID().toString().replace("-","")
    private fun tx(block:()->Unit) {db.beginTransaction();try {block();db.setTransactionSuccessful()}finally {db.endTransaction()}}
    private fun ns(args:JSONObject)=args.optString("ns",namespace()?:"local-proof").also {require(it=="local-proof"||it.matches(Regex("[a-f0-9]{32}")))}
    private fun state(ns:String):JSONObject {
        db.execSQL("INSERT OR IGNORE INTO activity_state(ns,token) VALUES(?,?)",arrayOf(ns,uuid()))
        return db.rawQuery("SELECT epoch,cursor,reset,token FROM activity_state WHERE ns=?",arrayOf(ns)).use {it.moveToFirst();JSONObject().put("epoch",if(it.isNull(0))JSONObject.NULL else it.getString(0)).put("cursor",it.getLong(1)).put("reset",if(it.isNull(2))JSONObject.NULL else it.getString(2)).put("token",it.getString(3))}
    }
    private fun enabled()=db.rawQuery("SELECT v FROM activity_settings WHERE k='enabled'",null).use {it.moveToFirst();it.getString(0)=="true"}
    fun command(args:JSONObject):JSONObject=synchronized(lock) {
        val ns=ns(args)
        when(args.getString("action")) {
            "activityStart" -> state(ns).put("enabled",enabled()).put("foreground",MainActivity.readingForeground)
            "activitySettings" -> {if(args.has("enabled")) {require(args.get("enabled") is Boolean);db.execSQL("UPDATE activity_settings SET v=? WHERE k='enabled'",arrayOf(args.getBoolean("enabled").toString()))};JSONObject().put("enabled",enabled())}
            "activityClear" -> {tx {
                val reset=uuid();state(ns);db.execSQL("DELETE FROM activity_events WHERE ns=?",arrayOf(ns));clearTotals(ns);clearTotals(ns+":canonical")
                db.execSQL("UPDATE activity_state SET reset=?,token=?,cursor=0 WHERE ns=?",arrayOf(reset,uuid(),ns))
            };JSONObject()}
            "activityRecord" -> {
                val st=state(ns)
                if(!enabled()||args.optString("token")!=st.getString("token"))return@synchronized JSONObject()
                val op=JSONObject(args.getJSONObject("event").toString())
                val book=args.optString("id",args.optString("localBookId",""))
                require(book.length<=128)
                op.put("id",op.optString("id").ifEmpty {uuid()}).put("bookId",book).put("title",args.optString("title").take(1000))
                op.put("generation",st.opt("epoch")?:JSONObject.NULL)
                if(!st.isNull("reset"))op.put("resetId",st.getString("reset"))
                if(args.has("version"))op.put("version",args.getString("version"))
                if(op.optString("kind")=="characters") {
                    val version=op.getString("version");require(version.matches(Regex("sha256-[a-f0-9]{64}")))
                    if(ns!="local-proof")require(db.rawQuery("SELECT 1 FROM downloads WHERE ns=? AND id=? AND version=?",arrayOf(ns,book,version)).use {it.moveToFirst()}) {"Book content changed"}
                }
                tx {
                    val pieces=if(op.optString("kind")=="time") {
                        val a=op.getLong("startMs");val b=op.getLong("endMs");val ms=op.getLong("milliseconds")
                        require(a>=0 && b>a && b-a<=15000 && ms in 1..15000 && ms<=b-a+1000) {"Invalid reading checkpoint duration"}
                        // Accept older frontend checkpoints with sub-millisecond clock skew,
                        // conservatively clamping before lifecycle clipping and durable storage.
                        val duration=minOf(ms,b-a)
                        // Native lifecycle boundaries protect against delayed WebView pause events.
                        MainActivity.activeIntervals(a,b).map {(start,end)->JSONObject(op.toString()).put("id",op.getString("id")+":$start:$end").put("startMs",start).put("endMs",end).put("milliseconds",duration*(end-start)/(b-a))}.filter {it.getLong("milliseconds")>0}
                    } else listOf(op)
                    for(piece in pieces)if(!db.rawQuery("SELECT 1 FROM activity_events WHERE ns=? AND id=?",arrayOf(ns,piece.getString("id"))).use {it.moveToFirst()}) {
                        apply(ns,piece)
                        db.execSQL("INSERT INTO activity_events VALUES(?,?,0,?,?)",arrayOf<Any>(ns,piece.getString("id"),if(ns=="local-proof")0 else 1,piece.toString()))
                    }
                };JSONObject()
            }
            "activitySummary" -> summary(ns,args)
            else -> error("Unknown activity action")
        }
    }
    private fun clearTotals(ns:String) {for(table in listOf("activity_totals","activity_time","activity_coverage"))db.execSQL("DELETE FROM $table WHERE ns=?",arrayOf(ns))}
    private fun apply(ns:String,op:JSONObject) {
        val day=op.getString("day");dayNumber(day)
        val book=op.optString("bookId");var ms=0L;var chars=0L;var lookups=0L
        when(op.getString("kind")) {
            "versionAlias" -> {
                val old=op.getString("oldVersion");val version=op.getString("version")
                val rows=mutableListOf<Triple<Int,Long,Long>>()
                db.rawQuery("SELECT spine,a,b FROM activity_coverage WHERE ns=? AND book=? AND version=?",arrayOf(ns,book,old)).use {while(it.moveToNext())rows.add(Triple(it.getInt(0),it.getLong(1),it.getLong(2)))}
                for((spine,a,b) in rows)apply(ns,JSONObject().put("kind","coverage").put("day",day).put("bookId",book).put("version",version).put("spine",spine).put("startOffset",a).put("endOffset",b))
                db.execSQL("DELETE FROM activity_coverage WHERE ns=? AND book=? AND version=?",arrayOf(ns,book,old))
            }
            "baseline" -> {ms=op.optLong("milliseconds");chars=op.optLong("characters");lookups=op.optLong("lookups")}
            "lookup" -> lookups=1
            "time" -> {
                val a=op.getLong("startMs");val b=op.getLong("endMs");val duration=op.getLong("milliseconds")
                require(a>=0 && b>a && b-a<=15000 && duration in 1..(b-a)) {"Invalid synced reading interval"}
                var start=a;var end=b;var overlap=0L
                db.rawQuery("SELECT a,b FROM activity_time WHERE ns=? AND b>=? AND a<=?",arrayOf(ns,a.toString(),b.toString())).use {while(it.moveToNext()) {val x=it.getLong(0);val y=it.getLong(1);overlap+=(minOf(b,y)-maxOf(a,x)).coerceAtLeast(0);start=minOf(start,x);end=maxOf(end,y)}}
                ms=minOf(duration,(b-a-overlap).coerceAtLeast(0))
                db.execSQL("DELETE FROM activity_time WHERE ns=? AND b>=? AND a<=?",arrayOf<Any>(ns,a,b));db.execSQL("INSERT INTO activity_time VALUES(?,?,?)",arrayOf<Any>(ns,start,end))
            }
            "characters","coverage" -> {
                val version=op.getString("version");require(version.length in 1..128)
                val spine=op.getInt("spine");val a=op.getLong("startOffset");val b=op.getLong("endOffset")
                require(spine in 0..100000 && a>=0 && b>a && b<=100000000 && (op.getString("kind")=="coverage"||b-a<=100000))
                var start=a;var end=b;var overlap=0L;var matches=0
                db.rawQuery("SELECT a,b FROM activity_coverage WHERE ns=? AND book=? AND version=? AND spine=? AND b>=? AND a<=?",arrayOf(ns,book,version,spine.toString(),a.toString(),b.toString())).use {while(it.moveToNext()) {matches++;val x=it.getLong(0);val y=it.getLong(1);overlap+=(minOf(b,y)-maxOf(a,x)).coerceAtLeast(0);start=minOf(start,x);end=maxOf(end,y)}}
                val count=db.rawQuery("SELECT count(*) FROM activity_coverage WHERE ns=? AND book=? AND version=?",arrayOf(ns,book,version)).use {it.moveToFirst();it.getInt(0)}
                if(matches==0&&count>=4096)return
                if(op.getString("kind")=="characters")chars=b-a-overlap
                db.execSQL("DELETE FROM activity_coverage WHERE ns=? AND book=? AND version=? AND spine=? AND b>=? AND a<=?",arrayOf(ns,book,version,spine,a,b))
                db.execSQL("INSERT INTO activity_coverage VALUES(?,?,?,?,?,?)",arrayOf(ns,book,version,spine,start,end))
            }
            else -> error("Invalid activity event")
        }
        if(ms+chars+lookups>0) {
            db.execSQL("INSERT OR IGNORE INTO activity_totals(ns,day,book,title) VALUES(?,?,?,?)",arrayOf(ns,day,book,op.optString("title")))
            db.execSQL("UPDATE activity_totals SET milliseconds=milliseconds+?,characters=characters+?,lookups=lookups+? WHERE ns=? AND day=? AND book=?",arrayOf(ms,chars,lookups,ns,day,book))
        }
    }
    private fun rebuild(ns:String) {
        clearTotals(ns)
        val canonical=ns+":canonical"
        db.execSQL("INSERT INTO activity_totals SELECT ?,day,book,title,milliseconds,characters,lookups FROM activity_totals WHERE ns=?",arrayOf(ns,canonical))
        db.execSQL("INSERT INTO activity_time SELECT ?,a,b FROM activity_time WHERE ns=?",arrayOf(ns,canonical))
        db.execSQL("INSERT INTO activity_coverage SELECT ?,book,version,spine,a,b FROM activity_coverage WHERE ns=?",arrayOf(ns,canonical))
        // Only the offline overlay is replayed. Canonical history is applied once per page.
        db.rawQuery("SELECT json FROM activity_events WHERE ns=? AND pending>0 ORDER BY rowid",arrayOf(ns)).use {while(it.moveToNext())apply(ns,JSONObject(it.getString(0)))}
    }
    fun status():JSONObject=synchronized(lock) {
        val ns=namespace()
        val counts=if(ns==null)0L to 0L else db.rawQuery("SELECT coalesce(sum(CASE WHEN pending>0 THEN 1 ELSE 0 END),0),coalesce(sum(CASE WHEN pending<0 THEN 1 ELSE 0 END),0) FROM activity_events WHERE ns=?",arrayOf(ns)).use {it.moveToFirst();it.getLong(0) to it.getLong(1)}
        JSONObject().put("busy",busy.get()).put("message",message).put("pending",counts.first).put("rejected",counts.second)
    }
    fun start():JSONObject {
        if(namespace()==null) {message="Local activity; no PC paired";return status()}
        if(busy.compareAndSet(false,true))worker.execute {try {message=if(sync())"Activity synced" else "More activity pending"}catch(e:Exception){message=e.message?:"Activity waiting for PC"}finally {busy.set(false)}}
        return status()
    }
    internal fun sync():Boolean {
        val ns=namespace()?:return false
        repeat(64) {
            val body=synchronized(lock) {val s=state(ns);val ops=JSONArray();db.rawQuery("SELECT json FROM activity_events WHERE ns=? AND pending=1 ORDER BY rowid LIMIT 64",arrayOf(ns)).use {while(it.moveToNext())ops.put(JSONObject(it.getString(0)).apply {remove("title")})};s.put("catalogId",ns).put("operations",ops).apply {remove("token")}}
            val response=request(body)
            require(response.getInt("protocolVersion")==1 && response.getString("catalogId")==ns)
            var more=false
            synchronized(lock) {tx {
                val current=state(ns)
                // A clear during the request supersedes its response completely.
                if(current.optString("reset")!=body.optString("reset"))return@tx
                val generation=response.getString("epoch")
                val changed=current.optString("epoch")!=generation
                val ownReset=current.optString("reset")==generation
                if(changed && !current.isNull("epoch") && !ownReset)db.execSQL("DELETE FROM activity_events WHERE ns=?",arrayOf(ns))
                else if(changed)db.execSQL("DELETE FROM activity_events WHERE ns=? AND pending<>1",arrayOf(ns))
                if(changed) {clearTotals(ns);clearTotals(ns+":canonical")}
                val ack=response.getJSONArray("acknowledged");for(i in 0 until ack.length())db.execSQL("UPDATE activity_events SET pending=2 WHERE ns=? AND id=? AND pending=1",arrayOf(ns,ack.getString(i)))
                val rejected=response.getJSONArray("rejected");for(i in 0 until rejected.length())db.execSQL("UPDATE activity_events SET pending=-1 WHERE ns=? AND id=?",arrayOf(ns,rejected.getString(i)))
                // Server provides sequence numbers separately from immutable operation IDs.
                val events=response.getJSONArray("events")
                for(i in 0 until events.length()) {
                    val op=events.getJSONObject(i)
                    val applied=db.rawQuery("SELECT 1 FROM activity_events WHERE ns=? AND id=? AND pending=0 AND seq>0",arrayOf(ns,op.getString("id"))).use {it.moveToFirst()}
                    if(!applied)apply(ns+":canonical",op)
                    db.execSQL("INSERT OR REPLACE INTO activity_events VALUES(?,?,?,0,?)",arrayOf<Any>(ns,op.getString("id"),op.getLong("seq"),op.toString()))
                }
                db.execSQL("UPDATE activity_state SET epoch=?,cursor=?,reset=NULL,token=? WHERE ns=?",arrayOf<Any>(generation,response.getLong("cursor"),if(changed&&!ownReset&&!current.isNull("epoch"))uuid() else current.getString("token"),ns))
                rebuild(ns)
                more=response.getBoolean("hasMore")||db.rawQuery("SELECT 1 FROM activity_events WHERE ns=? AND pending=1 LIMIT 1",arrayOf(ns)).use {it.moveToFirst()}
            }}
            if(!more)return true
        }
        return false
    }
    private fun summary(ns:String,args:JSONObject):JSONObject {
        val start=args.getString("startDay");val end=args.getString("endDay");val selected=args.getString("selectedDay");val week=args.getString("weekDay")
        val today=dayNumber(end);require(today-dayNumber(start) in 0..366);dayNumber(selected);dayNumber(week)
        val days=JSONArray();db.rawQuery("SELECT day,sum(milliseconds),sum(characters),sum(lookups) FROM activity_totals WHERE ns=? AND day BETWEEN ? AND ? GROUP BY day ORDER BY day",arrayOf(ns,start.toString(),end.toString())).use {while(it.moveToNext())days.put(JSONObject().put("day",it.getString(0)).put("milliseconds",it.getLong(1)).put("characters",it.getLong(2)).put("lookups",it.getLong(3)))}
        val books=JSONArray();db.rawQuery("SELECT book,title,milliseconds,characters,lookups FROM activity_totals WHERE ns=? AND day=? ORDER BY milliseconds DESC LIMIT 200",arrayOf(ns,selected.toString())).use {while(it.moveToNext())books.put(JSONObject().put("id",it.getString(0)).put("title",it.getString(1).ifEmpty {"Dictionary"}).put("milliseconds",it.getLong(2)).put("characters",it.getLong(3)).put("lookups",it.getLong(4)))}
        var last:Long?=null;var run=0;var longest=0
        db.rawQuery("SELECT day FROM activity_totals WHERE ns=? AND day<=? GROUP BY day HAVING sum(milliseconds)>=60000 ORDER BY day",arrayOf(ns,end.toString())).use {while(it.moveToNext()) {val day=dayNumber(it.getString(0));run=if(last?.plus(1)==day)run+1 else 1;longest=maxOf(longest,run);last=day}}
        val finished=db.rawQuery("SELECT count(*) FROM catalog WHERE ns=? AND gen=(SELECT v FROM settings WHERE k='generation-'||?) AND json_extract(json,'$.readingStatus')='finished' AND CAST(json_extract(json,'$.completedAt') AS INTEGER)>=? AND CAST(json_extract(json,'$.completedAt') AS INTEGER)<?",arrayOf(ns,ns,args.getLong("weekStart").toString(),args.getLong("nowSeconds").toString())).use {it.moveToFirst();it.getInt(0)}
        return JSONObject().put("enabled",enabled()).put("days",days).put("books",books).put("finished",finished).put("streak",if(last==today||last==today-1)run else 0).put("longestStreak",longest).put("syncMessage",message).put("activitySync",status()).put("resetPending",!state(ns).isNull("reset")).put("weekDay",week.toString())
    }
}
