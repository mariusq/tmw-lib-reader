package com.tmw.companion

import android.app.Activity
import android.database.sqlite.SQLiteDatabase
import android.graphics.Bitmap
import androidx.test.platform.app.InstrumentationRegistry
import org.json.JSONObject
import org.json.JSONArray
import org.junit.Test
import org.junit.Assert.*
import java.io.File
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.net.URL
import java.security.MessageDigest
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import javax.net.ssl.HttpsURLConnection

/** Generated isolated app-private fixtures. Never touches production data. */
class MobileStorageTest {
    private class Response(private val data:ByteArray,private val code:Int=200,private val tag:String?=null):HttpsURLConnection(URL("https://fixture.test.ts.net")) {
        override fun connect(){};override fun disconnect(){};override fun usingProxy()=false
        override fun getResponseCode()=code;override fun getInputStream()=ByteArrayInputStream(data)
        override fun getContentLengthLong()=data.size.toLong();override fun getHeaderField(name:String)=if(name=="ETag")tag else null
        override fun getCipherSuite()="fixture";override fun getLocalCertificates():Array<java.security.cert.Certificate>?=null
        override fun getServerCertificates():Array<java.security.cert.Certificate> = emptyArray()
    }
    private fun call(store:MobileStorage,name:String,vararg args:Any):Any? {
        val types=args.map { when(it) {is Long->java.lang.Long.TYPE;else->it.javaClass} }.toTypedArray()
        return try {MobileStorage::class.java.getDeclaredMethod(name,*types).apply {isAccessible=true}.invoke(store,*args)} catch(e:java.lang.reflect.InvocationTargetException) {throw e.targetException}
    }
    private fun db(s:MobileStorage)=MobileStorage::class.java.getDeclaredField("db").apply {isAccessible=true}.get(s) as SQLiteDatabase
    private fun close(s:MobileStorage) {db(s).close()}
    private fun activity():Activity {lateinit var a:Activity;InstrumentationRegistry.getInstrumentation().runOnMainSync {a=Activity()};return a}
    private val ns="a".repeat(32)
    private fun id(n:Int)="%032x".format(n)
    private fun fixture():ByteArray {
        val out=ByteArrayOutputStream();ZipOutputStream(out).use {z ->
            val bitmap=Bitmap.createBitmap(800,1200,Bitmap.Config.ARGB_8888);bitmap.eraseColor(android.graphics.Color.BLUE)
            val image=ByteArrayOutputStream();bitmap.compress(Bitmap.CompressFormat.PNG,100,image);bitmap.recycle()
            for((name,data) in listOf("mimetype" to "application/epub+zip".toByteArray(),"META-INF/container.xml" to "<container><rootfiles><rootfile full-path='OEBPS/book.opf'/></rootfiles></container>".toByteArray(),"OEBPS/book.opf" to "<package><manifest><item id='cover' href='cover.png' properties='cover-image'/><item id='chapter' href='chapter.xhtml'/></manifest><spine><itemref idref='chapter'/></spine></package>".toByteArray(),"OEBPS/chapter.xhtml" to "<html xmlns='http://www.w3.org/1999/xhtml'><body>猫</body></html>".toByteArray(),"OEBPS/cover.png" to image.toByteArray())) {z.putNextEntry(ZipEntry(name));z.write(data);z.closeEntry()}
        };return out.toByteArray()
    }
    @Test fun restartableTenThousandCatalogDownloadsAndCompleteCachePolicy() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.cacheDir,"phase4-fixture-${System.nanoTime()}").apply {mkdirs()}
        val epub=fixture();val version="sha256-"+MessageDigest.getInstance("SHA-256").digest(epub).joinToString("") {"%02x".format(it)}
        var interrupted=true;var delta=false;var mismatch=false;var changed=false;var requests=0
        val transport:(String,JSONObject?,String?)->HttpsURLConnection={path,body,match ->
            requests++
            when {
                path=="/v2/catalog" -> {
                    if(interrupted && body!!.optString("after")==id(100))throw java.io.IOException("interrupted page")
                    val items=JSONArray();val after=body!!.optString("after").ifEmpty {"0"}.toLong(16).toInt()
                    if(delta) {items.put(JSONObject().put("id",id(1)).put("deleted",true));items.put(JSONObject().put("id",id(2)).put("title","Unavailable").put("search","unavailable").put("available",false))}
                    else for(n in after+1..minOf(after+50,10000))items.put(JSONObject().put("id",id(n)).put("title","猫 $n").put("creator","Author").put("search","猫 ねこ neko $n").put("available",true).put("bytes",epub.size).put("coverVersion","fixture-v1").put("tags",JSONArray()).put("collections",JSONArray()))
                    Response(JSONObject().put("protocolVersion",2).put("catalogId",ns).put("epoch","fixture-epoch").put("revision",if(delta)2 else 1).put("items",items).put("next",if(delta||after+50>=10000)JSONObject.NULL else id(after+50)).toString().toByteArray())
                }
                path.endsWith("/content") ->Response(JSONObject().put("protocolVersion",1).put("contentVersion",version).put("bytes",epub.size).toString().toByteArray())
                path.endsWith("/epub") ->{assertEquals(version,match);Response(if(mismatch)epub.clone().apply {this[lastIndex]=(this[lastIndex].toInt() xor 1).toByte()} else epub,if(changed)412 else 200,version)}
                else->throw java.io.IOException("offline")
            }
        }
        var s=MobileStorage(activity(),root,transport)
        try {
            try {call(s,"sync");fail("Interrupted refresh succeeded")}catch(_:java.io.IOException){}
            assertEquals(0,(call(s,"browse",JSONObject()) as JSONObject).getJSONArray("items").length())
            close(s);s=MobileStorage(activity(),root,transport);interrupted=false
            val started=System.nanoTime();call(s,"sync");val syncMs=(System.nanoTime()-started)/1e6
            assertEquals(10000,db(s).compileStatement("SELECT count(*) FROM catalog").simpleQueryForLong())
            assertEquals(0,File(root,"covers").listFiles()!!.size)
            val browseStart=System.nanoTime();val page=call(s,"browse",JSONObject().put("query","neko")) as JSONObject;val browseMs=(System.nanoTime()-browseStart)/1e6
            assertEquals(25,page.getJSONArray("items").length())
            call(s,"download",ns,id(1));val copy=call(s,"downloadRow",ns,id(1)) as JSONObject
            val file=File(root,"books/${copy.getString("file")}");assertArrayEquals(epub,file.readBytes())
            mismatch=true;try {call(s,"download",ns,id(1));fail("Bad hash accepted")}catch(_:IllegalArgumentException){};mismatch=false
            changed=true;try {call(s,"download",ns,id(1));fail("Changed version accepted")}catch(_:IllegalArgumentException){};changed=false
            assertArrayEquals(epub,file.readBytes());assertFalse(File(root,"books/transfer.part").exists())
            val cancellation=MobileStorage::class.java.getDeclaredField("canceled").apply {isAccessible=true}.get(s) as java.util.concurrent.atomic.AtomicBoolean
            cancellation.set(true);try {call(s,"download",ns,id(1));fail("Cancellation ignored")}catch(_:IllegalStateException){};cancellation.set(false)
            assertArrayEquals(epub,file.readBytes())
            call(s,"download",ns,id(1));assertArrayEquals(epub,file.readBytes())
            delta=true;call(s,"sync");val local=call(s,"browse",JSONObject().put("filter","downloaded")) as JSONObject
            assertTrue(local.getJSONArray("items").getJSONObject(0).getBoolean("deleted"));assertTrue(file.isFile)
            val imageStart=System.nanoTime();val image=call(s,"embedded",file) as ByteArray;val decodeMs=(System.nanoTime()-imageStart)/1e6
            call(s,"validateCover",image)
            // Completed books/user records survive cache eviction, disabling, and clearing.
            db(s).execSQL("CREATE TABLE user_fixture(cfi TEXT,note TEXT)");db(s).execSQL("INSERT INTO user_fixture VALUES('position','note')")
            for(n in 1..3) {java.io.RandomAccessFile(File(root,"covers/${id(n)}.jpg"),"rw").use {it.setLength(40_000_000)};db(s).execSQL("INSERT INTO cache VALUES(?,?,?)",arrayOf<Any>(id(n),40_000_000,n))}
            call(s,"enforce",0L);assertFalse(File(root,"covers/${id(1)}.jpg").exists());assertEquals(80_000_000L,call(s,"usage"))
            call(s,"cacheSettings",JSONObject().put("budget",40_000_000));assertEquals(40_000_000L,call(s,"usage"))
            call(s,"cacheSettings",JSONObject().put("budget",0));assertEquals(0L,call(s,"usage"));assertTrue(file.isFile)
            val disabled=call(s,"cover",JSONObject().put("ns",ns).put("id",id(1)).put("generation",0).put("token","test")) as JSONObject
            assertTrue(disabled.getString("data").startsWith("data:image/jpeg;base64,"));assertEquals(0L,call(s,"usage"))
            call(s,"cacheSettings",JSONObject().put("budget",100_000_000));call(s,"cover",JSONObject().put("ns",ns).put("id",id(1)).put("generation",0).put("token","test2"))
            assertTrue((call(s,"usage") as Long)>0);call(s,"cacheSettings",JSONObject().put("clear",true));assertTrue(file.isFile)
            assertEquals(1,db(s).compileStatement("SELECT count(*) FROM user_fixture").simpleQueryForLong())
            // Interrupted fixed-name writes recover without touching durable data.
            File(root,"covers/interrupted.part").writeBytes(ByteArray(2048));File(root,"books/transfer.part").writeBytes(ByteArray(2048))
            // Crash after atomic rename but before the download-record commit.
            db(s).execSQL("DELETE FROM downloads WHERE id=?",arrayOf(id(1)))
            db(s).execSQL("INSERT INTO pending VALUES(?,?,?,?,?,NULL)",arrayOf<Any>(ns,id(1),version,copy.getString("file"),epub.size))
            close(s);s=MobileStorage(activity(),root) {_,_,_->throw java.io.IOException("offline")}
            assertFalse(File(root,"covers/interrupted.part").exists());assertFalse(File(root,"books/transfer.part").exists());assertArrayEquals(epub,file.readBytes())
            assertNotNull(call(s,"downloadRow",ns,id(1)))
            call(s,"removeCopy",JSONObject().put("ns",ns).put("id",id(1)));assertFalse(file.exists());assertEquals(10000,db(s).compileStatement("SELECT count(*) FROM catalog").simpleQueryForLong());assertEquals(1,db(s).compileStatement("SELECT count(*) FROM user_fixture").simpleQueryForLong())
            println("PHASE4 rows=10000 syncMs=$syncMs browseMs=$browseMs embeddedJpegBytes=${image.size} embeddedDecodeMs=$decodeMs requests=$requests")
        } finally {close(s);require(root.canonicalPath.startsWith(context.cacheDir.canonicalPath+File.separator));root.deleteRecursively()}
    }
}
