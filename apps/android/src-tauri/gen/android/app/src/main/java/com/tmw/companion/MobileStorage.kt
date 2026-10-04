package com.tmw.companion

import android.app.Activity
import android.content.ContentValues
import android.database.sqlite.SQLiteDatabase
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.os.StatFs
import android.util.Base64
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.io.FileOutputStream
import java.security.MessageDigest
import java.text.Normalizer
import java.util.Locale
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.zip.ZipFile
import javax.net.ssl.HttpsURLConnection

/** Entirely app-owned storage. No filesystem path is accepted from JS/network. */
internal class MobileStorage(activity: Activity,
    directory:File=File(activity.filesDir,"tmw-mobile"),
    private val connect: (String, JSONObject?, String?) -> HttpsURLConnection) {
    private val root=directory.apply { mkdirs() }
    private val books=File(root,"books").apply { mkdirs() }
    private val covers=File(root,"covers").apply { mkdirs() }
    private val db=SQLiteDatabase.openOrCreateDatabase(File(root,"catalog.sqlite3"),null)
    private val local=ThreadPoolExecutor(1,1,0,TimeUnit.SECONDS,ArrayBlockingQueue<Runnable>(32))
    private val network=ThreadPoolExecutor(1,1,0,TimeUnit.SECONDS,ArrayBlockingQueue<Runnable>(1))
    private val coverWorker=ThreadPoolExecutor(2,2,0,TimeUnit.SECONDS,ArrayBlockingQueue<Runnable>(24))
    private val canceled=AtomicBoolean(false)
    private val running=AtomicBoolean(false)
    @Volatile private var socket: HttpsURLConnection?=null
    @Volatile private var status=JSONObject().put("busy",false).put("message","Ready")
    private val coverGeneration=java.util.concurrent.atomic.AtomicLong(0)
    private val coverSockets=java.util.concurrent.ConcurrentHashMap<String,HttpsURLConnection>()
    private val obsolete=java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
    private val activeCovers=java.util.concurrent.ConcurrentHashMap.newKeySet<String>()
    private val cacheLock=Any()
    init {
        db.enableWriteAheadLogging()
        db.execSQL("CREATE TABLE IF NOT EXISTS settings(k TEXT PRIMARY KEY,v TEXT NOT NULL)")
        db.execSQL("INSERT OR IGNORE INTO settings VALUES('budget','100000000')")
        db.execSQL("CREATE TABLE IF NOT EXISTS catalog(ns TEXT,gen TEXT,id TEXT,json TEXT,title TEXT,creator TEXT,series TEXT,search TEXT,available INTEGER,deleted INTEGER DEFAULT 0,modified INTEGER,dateAdded INTEGER,PRIMARY KEY(ns,gen,id))")
        db.execSQL("CREATE INDEX IF NOT EXISTS catalog_title ON catalog(ns,title,id)")
        db.execSQL("CREATE INDEX IF NOT EXISTS catalog_creator ON catalog(ns,creator,id)")
        db.execSQL("CREATE INDEX IF NOT EXISTS catalog_date ON catalog(ns,dateAdded,id)")
        db.execSQL("CREATE TABLE IF NOT EXISTS stage(id TEXT PRIMARY KEY,json TEXT)")
        db.execSQL("CREATE TABLE IF NOT EXISTS downloads(ns TEXT,id TEXT,version TEXT,file TEXT,bytes INTEGER,PRIMARY KEY(ns,id))")
        db.execSQL("CREATE TABLE IF NOT EXISTS pending(ns TEXT,id TEXT,version TEXT,file TEXT,bytes INTEGER,previous TEXT,PRIMARY KEY(ns,id))")
        db.execSQL("CREATE TABLE IF NOT EXISTS cache(k TEXT PRIMARY KEY,bytes INTEGER,touched INTEGER)")
        db.execSQL("PRAGMA user_version=1")
        // Fixed temporary names, owned directories only. Preserve all completed files.
        books.listFiles()?.filter { it.name.endsWith(".part") }?.forEach { it.delete() }
        db.rawQuery("SELECT ns,id,version,file,bytes,previous FROM pending",null).use {c ->while(c.moveToNext()) {
            val file=File(books,c.getString(3))
            if(file.isFile && file.length()==c.getLong(4)) {
                val digest=MessageDigest.getInstance("SHA-256");file.inputStream().use {input ->val buf=ByteArray(65536);while(true) {val n=input.read(buf);if(n<0)break;digest.update(buf,0,n)}}
                if("sha256-"+digest.digest().joinToString("") {"%02x".format(it)}==c.getString(2)) {
                    db.execSQL("INSERT OR REPLACE INTO downloads VALUES(?,?,?,?,?)",arrayOf<Any>(c.getString(0),c.getString(1),c.getString(2),c.getString(3),c.getLong(4)))
                    if(!c.isNull(5) && c.getString(5)!=c.getString(3))File(books,c.getString(5)).delete()
                }
            }
        }}
        db.execSQL("DELETE FROM pending")
        covers.listFiles()?.filter { it.name.endsWith(".part") }?.forEach { it.delete() }
        synchronized(cacheLock) { enforce(0) }
    }
    private fun setting(k:String):String?=db.rawQuery("SELECT v FROM settings WHERE k=?",arrayOf(k)).use { if(it.moveToFirst())it.getString(0) else null }
    private fun set(k:String,v:String) { db.execSQL("INSERT OR REPLACE INTO settings VALUES(?,?)",arrayOf(k,v)) }
    private fun transaction(block:()->Unit) { db.beginTransaction();try {block();db.setTransactionSuccessful()} finally {db.endTransaction()} }
    private fun valid(id:String):String {require(id.matches(Regex("[a-f0-9]{32}")));return id}
    private fun hash(bytes:ByteArray)=MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") {"%02x".format(it)}
    private fun checkCancel() {check(!canceled.get()) {"Canceled; completed copies and catalog are preserved"} }
    private fun progress(message:String,done:Long=0,total:Long=0) {status=JSONObject().put("busy",true).put("message",message).put("done",done).put("total",total)}
    private fun bytes(c:HttpsURLConnection,max:Int):ByteArray {
        require(c.responseCode==200) {if(c.responseCode==409)"restart_snapshot" else "PC request failed (${c.responseCode}); check pairing/service"}
        val out=java.io.ByteArrayOutputStream();c.inputStream.use { input -> val buf=ByteArray(8192);while(true) {val n=input.read(buf);if(n<0)break;require(out.size()+n<=max);out.write(buf,0,n)} };return out.toByteArray()
    }
    private fun json(path:String,body:JSONObject?=null):JSONObject {
        checkCancel(); val c=connect(path,body,null);socket=c
        try {return JSONObject(String(bytes(c,1_000_000),Charsets.UTF_8))} finally {socket=null;c.disconnect()}
    }
    private fun sync() {
        // A saved stage remains invisible until the entire fixed-revision sequence commits.
        var checkpoint=setting("checkpoint")?.let {JSONObject(it)}
        if(checkpoint!=null && System.currentTimeMillis()-checkpoint.optLong("started")>900000) checkpoint=null
        var attempts=0
        while(true) {
            checkCancel()
            if(checkpoint==null) {
                db.execSQL("DELETE FROM stage")
                val previous=setting("cursor")?.let {JSONObject(it)}
                checkpoint=JSONObject().put("epoch",previous?.optString("epoch")).put("since",previous?.optLong("revision")?:0)
                    .put("delta",previous!=null).put("after","").put("started",System.currentTimeMillis())
                set("checkpoint",checkpoint.toString())
            }
            val cp=checkpoint
            if(cp.optBoolean("complete")) {finish(cp);return}
            val req=JSONObject(cp.toString()).apply {remove("started");remove("catalogId");remove("complete")}
            val response=try {json("/v2/catalog",req)} catch(e:Exception) {
                if(e.message=="restart_snapshot" && attempts++<3) {
                    db.execSQL("DELETE FROM settings WHERE k IN ('checkpoint','cursor')");checkpoint=null;continue
                };throw e
            }
            require(response.getInt("protocolVersion")==2)
            require(StatFs(root.path).availableBytes>response.toString().length*4L+32_000_000) {"Insufficient storage for catalog page"}
            val ns=valid(response.getString("catalogId"));val items=response.getJSONArray("items");require(items.length()<=50)
            if(cp.has("catalogId")) require(cp.getString("catalogId")==ns)
            cp.put("catalogId",ns).put("epoch",response.getString("epoch")).put("revision",response.getLong("revision"))
            if(response.has("expires"))cp.put("expires",response.getLong("expires"))
            cp.put("after",if(response.isNull("next")) "" else response.getString("next"))
            cp.put("complete",response.isNull("next"))
            transaction {
                for(i in 0 until items.length()) {val row=items.getJSONObject(i);valid(row.getString("id"));db.execSQL("INSERT OR REPLACE INTO stage VALUES(?,?)",arrayOf(row.getString("id"),row.toString()))}
                set("checkpoint",cp.toString())
            }
            val count=db.rawQuery("SELECT count(*) FROM stage",null).use {it.moveToFirst();it.getLong(0)}
            progress("Catalog staged: $count records",count)
            if(response.isNull("next")) {
                checkCancel()
                finish(cp)
                return
            }
        }
    }
    private fun finish(cp:JSONObject) {
        val ns=cp.getString("catalogId")
        val gen=hash(cp.toString().toByteArray())
        val old=setting("generation-$ns")?:""
        // Build an invisible generation in 250-record commits. Only the final
        // pointer switch is visible; cancellation/restart keeps the old catalog.
        var after=""
        while(true) {
            checkCancel();var count=0
            transaction {db.rawQuery("SELECT id,json,deleted FROM catalog WHERE ns=? AND gen=? AND id>? ORDER BY id LIMIT 250",arrayOf(ns,old,after)).use {c ->while(c.moveToNext()) {
                val row=JSONObject(c.getString(1));if(!cp.getBoolean("delta")||c.getInt(2)==1)row.put("deleted",true)
                apply(ns,gen,row);after=c.getString(0);count++
            }}}
            if(count<250)break
        }
        after=""
        while(true) {
            checkCancel();var count=0
            transaction {db.rawQuery("SELECT id,json FROM stage WHERE id>? ORDER BY id LIMIT 250",arrayOf(after)).use {c ->while(c.moveToNext()) {apply(ns,gen,JSONObject(c.getString(1)));after=c.getString(0);count++}}}
            if(count<250)break
        }
        checkCancel()
        transaction {
            set("generation-$ns",gen)
            set("namespace",ns);set("cursor",JSONObject().put("epoch",cp.getString("epoch")).put("revision",cp.getLong("revision")).toString())
            db.execSQL("DELETE FROM stage");db.execSQL("DELETE FROM settings WHERE k='checkpoint'")
        }
        // Superseded extracted metadata only; downloaded files/user storage are separate.
        while(true) {
            val count=db.compileStatement("DELETE FROM catalog WHERE rowid IN (SELECT rowid FROM catalog WHERE ns=? AND gen<>? LIMIT 250)").use {s ->s.bindString(1,ns);s.bindString(2,gen);s.executeUpdateDelete()}
            if(count<250)break
        }
    }
    private fun apply(ns:String,gen:String,row:JSONObject) {
        val id=valid(row.getString("id"))
        if(row.optBoolean("deleted") && !row.has("title")) {db.execSQL("UPDATE catalog SET deleted=1,available=0 WHERE ns=? AND gen=? AND id=?",arrayOf(ns,gen,id));return}
        val v=ContentValues().apply {
            put("ns",ns);put("gen",gen);put("id",id);put("json",row.toString());put("title",row.optString("title"));put("creator",row.optString("creator"));put("series",row.optString("series"));put("search",row.optString("search"));put("available",if(row.optBoolean("available")&&!row.optBoolean("deleted"))1 else 0);put("deleted",if(row.optBoolean("deleted"))1 else 0);put("modified",row.optLong("modified"));put("dateAdded",row.optLong("dateAdded"))
        };check(db.insertWithOnConflict("catalog",null,v,SQLiteDatabase.CONFLICT_REPLACE)>=0)
    }
    private fun download(ns:String,id:String) {
        require(ns==setting("namespace")) {"Select the current paired catalog"}
        val info=json("/v1/books/${valid(id)}/content")
        val version=info.getString("contentVersion");require(version.matches(Regex("sha256-[a-f0-9]{64}")))
        val length=info.getLong("bytes");require(length in 1..64_000_000) {"Reader supports copies up to 64 MB, not the PC's 512 MB maximum"}
        require(StatFs(books.path).availableBytes>length+32_000_000) {"Insufficient free storage (32 MB reserve required)"}
        val name=hash("$ns:$id:$version".toByteArray())+".epub";val part=File(books,"transfer.part");val target=File(books,name)
        val c=connect("/v1/books/$id/epub",null,version);socket=c
        try {
            require(c.responseCode==200) {"Download rejected (${c.responseCode}); source may have changed"}
            require(c.contentLengthLong==length && c.getHeaderField("ETag")?.trim('"')==version) {"Source version/length changed"}
            val digest=MessageDigest.getInstance("SHA-256");var total=0L;var tick=0L;val deadline=System.nanoTime()+180_000_000_000L
            FileOutputStream(part).use {out ->c.inputStream.use {input ->val buf=ByteArray(65536);while(true) {
                checkCancel();require(System.nanoTime()<deadline) {"Transfer deadline exceeded"};val n=input.read(buf);if(n<0)break
                total+=n;require(total<=length);digest.update(buf,0,n);out.write(buf,0,n)
                if(System.currentTimeMillis()-tick>200) {progress("Downloading",total,length);tick=System.currentTimeMillis()}
            }};out.fd.sync()}
            require(total==length && "sha256-"+digest.digest().joinToString("") {"%02x".format(it)}==version) {"Download integrity mismatch; existing copy retained"}
            validateBook(part);checkCancel()
            val old=downloadRow(ns,id)
            db.execSQL("INSERT OR REPLACE INTO pending VALUES(?,?,?,?,?,?)",arrayOf<Any?>(ns,id,version,name,length,old?.getString("file")))
            require(part.renameTo(target)) {"Atomic completion failed"}
            transaction { db.execSQL("INSERT OR REPLACE INTO downloads VALUES(?,?,?,?,?)",arrayOf<Any>(ns,id,version,name,length)) }
            if(old!=null && old.getString("file")!=name) File(books,old.getString("file")).delete()
            db.execSQL("DELETE FROM pending WHERE ns=? AND id=?",arrayOf(ns,id))
        } finally {socket=null;c.disconnect();part.delete()}
    }
    private fun validateBook(file:File) {
        ZipFile(file).use {zip ->require(zip.size()<=4096);var inflated=0L
            val entries=zip.entries();while(entries.hasMoreElements()) {checkCancel();val e=entries.nextElement();require(e.size in 0..16_000_000);inflated+=e.size;require(inflated<=128_000_000)
                // Read through each entry once with fixed memory; reject dishonest lengths/CRC.
                var actual=0L;val crc=java.util.zip.CRC32();zip.getInputStream(e).use {input ->val buf=ByteArray(65536);while(true) {checkCancel();val n=input.read(buf);if(n<0)break;actual+=n;require(actual<=e.size);crc.update(buf,0,n)}}
                require(actual==e.size && (e.isDirectory || crc.value==e.crc)) {"Corrupt EPUB entry"}
            }
            require(zip.getEntry("META-INF/container.xml")!=null) {"Invalid EPUB container"}
        }
    }
    private fun downloadRow(ns:String,id:String):JSONObject?=db.rawQuery("SELECT version,file,bytes FROM downloads WHERE ns=? AND id=?",arrayOf(ns,id)).use {if(!it.moveToFirst())null else JSONObject().put("version",it.getString(0)).put("file",it.getString(1)).put("bytes",it.getLong(2))}
    private fun browse(args:JSONObject):JSONObject {
        val ns=args.optString("ns",setting("namespace")?:""); val offset=args.optInt("offset",0);require(offset in 0..1_000_000)
        val query=args.optString("query");require(query.length<=512)
        val normalized=Normalizer.normalize(query,Normalizer.Form.NFKC).lowercase(Locale.ROOT).trim().replace(Regex("\\s+")," ")
        val params=mutableListOf(ns)
        var where="c.ns=? AND c.gen=(SELECT v FROM settings WHERE k='generation-'||c.ns)"
        if(normalized.isNotEmpty()) {where+=" AND (instr(lower(c.search),?)>0 OR instr(lower(c.search),?)>0)";params.add(normalized);params.add(args.optString("romajiQuery",normalized))}
        when(args.optString("filter")) {"downloaded"->where+=" AND d.id IS NOT NULL";"available"->where+=" AND c.available=1 AND c.deleted=0";"unavailable"->where+=" AND (c.available=0 OR c.deleted=1)";"finished"->where+=" AND json_extract(c.json,'$.readingStatus')='finished'"}
        for(key in listOf("tags","collections")) {val name=args.optString(key);if(name.isNotEmpty()) {where+=" AND EXISTS(SELECT 1 FROM json_each(c.json,'$.$key') m WHERE json_extract(m.value,'$.name')=?)";params.add(name)}}
        val sort=when(args.optString("sort")) {"author"->"c.creator COLLATE NOCASE,c.title COLLATE NOCASE";"series"->"c.series COLLATE NOCASE,c.title COLLATE NOCASE";"modified"->"c.modified DESC";"dateAdded"->"c.dateAdded DESC";else->"c.title COLLATE NOCASE"}
        val rows=JSONArray();db.rawQuery("SELECT c.json,c.deleted,c.available,d.version,d.file,d.bytes FROM catalog c LEFT JOIN downloads d ON d.ns=c.ns AND d.id=c.id WHERE $where ORDER BY $sort,c.id LIMIT 25 OFFSET $offset",params.toTypedArray()).use {c ->while(c.moveToNext()) {
            val row=JSONObject(c.getString(0)).put("ns",ns).put("deleted",c.getInt(1)==1).put("available",c.getInt(2)==1)
            if(!c.isNull(3) && File(books,c.getString(4)).isFile) row.put("download",JSONObject().put("version",c.getString(3)).put("file",c.getString(4)).put("bytes",c.getLong(5)))
            rows.put(row)
        }}
        val namespaces=JSONArray();db.rawQuery("SELECT DISTINCT ns FROM catalog ORDER BY ns",null).use {while(it.moveToNext())namespaces.put(it.getString(0))}
        return JSONObject().put("items",rows).put("ns",ns).put("namespaces",namespaces).put("next",if(rows.length()==25)offset+25 else JSONObject.NULL)
    }
    private fun usage()=covers.listFiles()?.sumOf {it.length()}?:0L
    private fun budget()=(setting("budget")?:"100000000").toLong()
    private fun enforce(reserve:Long) {
        val limit=budget();require(reserve<=limit)
        // Recover orphan completed cache entries too, never traverse another directory.
        covers.listFiles()?.filter {it.name.endsWith(".jpg") }?.forEach {f ->db.rawQuery("SELECT 1 FROM cache WHERE k=?",arrayOf(f.name.removeSuffix(".jpg"))).use {if(!it.moveToFirst())f.delete()} }
        var used=usage()
        db.rawQuery("SELECT k FROM cache ORDER BY touched,k",null).use {c ->while(used+reserve>limit && c.moveToNext()) {val f=File(covers,c.getString(0)+".jpg");val size=f.length();if(!f.exists()||f.delete()) {used-=size;db.execSQL("DELETE FROM cache WHERE k=?",arrayOf(c.getString(0)))}}}
        require(usage()+reserve<=limit) {"Cover budget exhausted"}
    }
    private fun validateCover(data:ByteArray) {
        require(data.size<=262144)
        val bounds=BitmapFactory.Options().apply {inJustDecodeBounds=true};BitmapFactory.decodeByteArray(data,0,data.size,bounds)
        require(bounds.outWidth in 1..240 && bounds.outHeight in 1..360 && bounds.outMimeType=="image/jpeg") {"Invalid bounded thumbnail"}
        BitmapFactory.decodeByteArray(data,0,data.size)?.recycle() ?: error("Invalid thumbnail")
    }
    private fun embedded(file:File):ByteArray? {
        // OPF cover-image or legacy cover meta. XML forbids external entities.
        ZipFile(file).use {zip ->
            fun xml(name:String):org.w3c.dom.Document {
                val e=zip.getEntry(name)?:error("Missing EPUB metadata");require(e.size in 1..1_000_000)
                val data=zip.getInputStream(e).use {it.readBytes()};require(data.size<=1_000_000)
                // Android's Harmony parser does not implement the Xerces DTD flag.
                // Reject DTD syntax before parsing (including UTF-16/32 ASCII),
                // and reject every external entity resolution independently.
                require(!String(data,Charsets.UTF_8).replace("\u0000","").contains("<!DOCTYPE",ignoreCase=true))
                val builder=javax.xml.parsers.DocumentBuilderFactory.newInstance().apply {isNamespaceAware=true;isExpandEntityReferences=false}.newDocumentBuilder()
                builder.setEntityResolver {_,_->throw org.xml.sax.SAXException("External entities forbidden")}
                return builder.parse(java.io.ByteArrayInputStream(data))
            }
            val container=xml("META-INF/container.xml");val opf=container.getElementsByTagNameNS("*","rootfile").item(0)?.attributes?.getNamedItem("full-path")?.nodeValue?:return null
            val doc=xml(opf);var legacy="";val metas=doc.getElementsByTagNameNS("*","meta")
            for(i in 0 until metas.length) {val m=metas.item(i).attributes;if(m.getNamedItem("name")?.nodeValue=="cover")legacy=m.getNamedItem("content")?.nodeValue?:""}
            val items=doc.getElementsByTagNameNS("*","item")
            for(i in 0 until items.length) {val a=items.item(i).attributes
                if(a.getNamedItem("properties")?.nodeValue?.split(' ')?.contains("cover-image")==true || (legacy.isNotEmpty() && a.getNamedItem("id")?.nodeValue==legacy)) {
                    val href=a.getNamedItem("href")?.nodeValue?:return null
                    val path=java.net.URI(opf).resolve(href).normalize().path;require(!path.startsWith("/") && !path.startsWith("../"))
                    val entry=zip.getEntry(path)?:return null;require(entry.size in 1..8_000_000)
                    val data=zip.getInputStream(entry).use {it.readBytes()}
                    val bounds=BitmapFactory.Options().apply {inJustDecodeBounds=true};BitmapFactory.decodeByteArray(data,0,data.size,bounds)
                    require(bounds.outWidth in 1..8192 && bounds.outHeight in 1..8192)
                    var sample=1;while(bounds.outWidth/sample>480 || bounds.outHeight/sample>720)sample*=2
                    val decoded=BitmapFactory.decodeByteArray(data,0,data.size,BitmapFactory.Options().apply {inSampleSize=sample})?:return null
                    val scale=minOf(1.0,240.0/decoded.width,360.0/decoded.height)
                    val small=Bitmap.createScaledBitmap(decoded,maxOf(1,(decoded.width*scale).toInt()),maxOf(1,(decoded.height*scale).toInt()),true)
                    val out=java.io.ByteArrayOutputStream();small.compress(Bitmap.CompressFormat.JPEG,75,out)
                    if(small!==decoded)small.recycle();decoded.recycle();return out.toByteArray()
                }
            };return null
        }
    }
    private fun cover(args:JSONObject):JSONObject {
        val gen=args.getLong("generation");require(gen==coverGeneration.get()) {"Obsolete cover"}
        val token=args.optString("token");require(!obsolete.contains(token)) {"Obsolete cover"}
        val ns=valid(args.getString("ns"));val id=valid(args.getString("id"))
        val row=db.rawQuery("SELECT json FROM catalog WHERE ns=? AND gen=? AND id=?",arrayOf(ns,setting("generation-$ns")?:"",id)).use {require(it.moveToFirst());JSONObject(it.getString(0))}
        val downloaded=downloadRow(ns,id)
        val version=if(downloaded!=null)downloaded.getString("version") else row.optString("coverVersion","")
        if(version.isEmpty() || version=="null")return JSONObject()
        val k=hash("$ns:$id:$version:jpeg75-v1".toByteArray());val file=File(covers,"$k.jpg")
        synchronized(cacheLock) {if(file.isFile) {
            try {require(file.length()<=262144);val data=file.readBytes();validateCover(data);db.execSQL("UPDATE cache SET touched=? WHERE k=?",arrayOf<Any>(System.currentTimeMillis(),k));return JSONObject().put("data","data:image/jpeg;base64,"+Base64.encodeToString(data,Base64.NO_WRAP))}
            catch(_:Exception) {file.delete();db.execSQL("DELETE FROM cache WHERE k=?",arrayOf(k))}
        }}
        var data:ByteArray?=null
        if(downloaded!=null) data=try {embedded(File(books,downloaded.getString("file")))} catch(_:Exception) {null}
        if(data==null && ns==setting("namespace")) {
            val c=connect("/v2/books/$id/cover",null,row.optString("coverVersion"))
            coverSockets[token]=c
            try {require(!obsolete.contains(token));data=bytes(c,262144);require(c.getHeaderField("ETag")?.trim('"')=="cover-v1-"+hash(data)) {"Thumbnail integrity mismatch"}} finally {coverSockets.remove(token);c.disconnect()}
        }
        if(data==null)return JSONObject()
        validateCover(data);require(gen==coverGeneration.get() && !obsolete.contains(token)) {"Obsolete cover"}
        synchronized(cacheLock) {
            if(budget()>0 && data.size<=budget()) {
                enforce(data.size.toLong());val part=File(covers,"$k.part")
                try {FileOutputStream(part).use {it.write(data);it.fd.sync()};require(gen==coverGeneration.get() && !obsolete.contains(token));require(part.renameTo(file));db.execSQL("INSERT OR REPLACE INTO cache VALUES(?,?,?)",arrayOf<Any>(k,data.size,System.currentTimeMillis()))} finally {part.delete()}
                enforce(0)
            }
        }
        return JSONObject().put("data","data:image/jpeg;base64,"+Base64.encodeToString(data,Base64.NO_WRAP)).put("bytes",data.size)
    }
    fun command(invoke:Invoke) {
        val args=invoke.getArgs();val action=args.optString("action")
        fun resolve(value:JSONObject) {invoke.resolve(JSObject(value.toString()))}
        if(action=="cancel") {canceled.set(true);socket?.disconnect();resolve(JSONObject().put("message","Cancel requested"));return}
        if(action=="coverGeneration") {coverGeneration.set(args.getLong("generation"));coverSockets.values.forEach {it.disconnect()};resolve(JSONObject());return}
        if(action=="coverCancel") {val token=args.getString("token");if(activeCovers.contains(token))obsolete.add(token);coverSockets[token]?.disconnect();resolve(JSONObject());return}
        if(action=="status") {resolve(JSONObject(status.toString()));return}
        if(action=="sync" || action=="download") {
            if(!running.compareAndSet(false,true)) {invoke.reject("A catalog/transfer job is already running");return}
            canceled.set(false);progress("Starting")
            network.execute {try {if(action=="sync")sync() else download(valid(args.getString("ns")),valid(args.getString("id")));status=JSONObject().put("busy",false).put("message","Complete")}
                catch(e:Exception) {status=JSONObject().put("busy",false).put("message",e.message?:"Operation failed")}
                finally {running.set(false)}}
            resolve(JSONObject().put("busy",true));return
        }
        val executor=if(action=="cover")coverWorker else local
        if(action=="cover")activeCovers.add(args.optString("token"))
        try {executor.execute {try {
            val result=when(action) {
                "browse"->browse(args)
                "cover"->cover(args)
                "remove"->removeCopy(args)
                "cacheSettings"->cacheSettings(args)
                else->error("Unknown storage action")
            };resolve(result)
        }catch(e:Exception) {invoke.reject(e.message?:"Mobile storage operation failed")} finally {if(action=="cover") {obsolete.remove(args.optString("token"));activeCovers.remove(args.optString("token"))}}}} catch(_:java.util.concurrent.RejectedExecutionException) {activeCovers.remove(args.optString("token"));invoke.reject("Storage queue busy")}
    }
    private fun removeCopy(args:JSONObject):JSONObject {
        require(!running.get());val ns=valid(args.getString("ns"));val id=valid(args.getString("id"));val old=downloadRow(ns,id)
        if(old!=null)require(!File(books,old.getString("file")).exists()||File(books,old.getString("file")).delete()) {"Could not remove local copy"}
        db.execSQL("DELETE FROM downloads WHERE ns=? AND id=?",arrayOf(ns,id))
        return JSONObject()
    }
    private fun cacheSettings(args:JSONObject):JSONObject=synchronized(cacheLock) {
        if(args.has("budget")) {val b=args.getLong("budget");require(b in 0..1_000_000_000);set("budget",b.toString());enforce(0)}
        if(args.optBoolean("clear")) {coverGeneration.incrementAndGet();coverSockets.values.forEach {it.disconnect()};covers.listFiles()?.forEach {require(it.delete())};db.execSQL("DELETE FROM cache")}
        JSONObject().put("budget",budget()).put("usage",usage())
    }
}
