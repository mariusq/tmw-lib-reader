package com.tmw.companion

import android.content.ContentValues
import android.database.sqlite.SQLiteDatabase
import org.json.JSONObject
import java.security.MessageDigest
import java.text.Normalizer
import java.util.Locale

/** Versioned PC metadata only; no Android inference or book-file access. */
internal object SeriesCatalog {
    fun ensureSchema(db:SQLiteDatabase) {
        val existing=mutableSetOf<String>()
        db.rawQuery("PRAGMA table_info(catalog)",null).use {while(it.moveToNext())existing.add(it.getString(1))}
        for((name,type) in listOf("seriesKey" to "TEXT NOT NULL DEFAULT ''","seriesTitle" to "TEXT NOT NULL DEFAULT ''","seriesConfidence" to "INTEGER NOT NULL DEFAULT 0","seriesOrder" to "REAL NOT NULL DEFAULT 0","copyKey" to "TEXT NOT NULL DEFAULT ''")) {
            if(!existing.contains(name))db.execSQL("ALTER TABLE catalog ADD COLUMN $name $type")
        }
        db.execSQL("CREATE INDEX IF NOT EXISTS catalog_series_members ON catalog(ns,gen,seriesKey,seriesConfidence,id)")
        db.execSQL("CREATE INDEX IF NOT EXISTS catalog_copy_members ON catalog(ns,gen,copyKey,id)")
    }
    private fun digest(value:String)=MessageDigest.getInstance("SHA-256").digest(value.toByteArray(Charsets.UTF_8)).joinToString("") {"%02x".format(it)}
    private fun normalized(value:String)=Normalizer.normalize(value,Normalizer.Form.NFKC).lowercase(Locale.ROOT).trim().replace(Regex("\\s+")," ")
    fun putValues(values:ContentValues,row:JSONObject) {
        var key="";var name="";var confidence=0;var order=0.0
        val explicit=row.optString("series","")
        val membership=row.optJSONObject("seriesMembership")
        if(explicit.isNotEmpty()) {key="metadata:"+digest(explicit);name=explicit;confidence=1;order=row.optString("volume").toDoubleOrNull()?:0.0}
        else if(membership!=null && membership.optInt("version")==1) {
            val candidateKey=membership.optString("key");val candidateName=membership.optString("name");val candidateConfidence=membership.optInt("confidence")
            if(candidateKey.length in 1..128 && candidateName.length in 1..512 && candidateConfidence in 1..3) {
                key=candidateKey;name=candidateName;confidence=candidateConfidence;order=membership.optDouble("order",0.0).takeIf {it.isFinite() && it in -1_000_000.0..1_000_000.0}?:0.0
            }
        }
        order=order.takeIf {it.isFinite() && it in -1_000_000.0..1_000_000.0}?:0.0
        values.put("seriesKey",key);values.put("seriesTitle",name);values.put("seriesConfidence",confidence);values.put("seriesOrder",order)
        val title=normalized(row.optString("title"))
        val copy=if(title.isEmpty()) "book:"+row.getString("id") else digest(org.json.JSONArray().put(title).put(normalized(row.optString("creator"))).put(normalized(row.optString("volume"))).toString())
        values.put("copyKey",copy)
    }
    // Aggregate CTEs work on API 24's SQLite; no window functions.
    fun groupedSql(filtered:String,sort:String,collapse:Boolean):String {
        if(!collapse) return "WITH filtered AS ($filtered) SELECT f.json,f.deleted,f.available,f.downloadVersion,f.downloadFile,f.downloadBytes,f.groupKey,f.groupTitle,1 FROM filtered f ORDER BY $sort,f.id LIMIT 25 OFFSET ?"
        // Aggregate once per group, retaining exact numeric order and deterministic ties.
        // Correlated scans over a materialized CTE made full-library browsing quadratic.
        return """WITH filtered AS ($filtered),
            ranked AS (SELECT *, (CASE WHEN downloadVersion IS NULL THEN 100 ELSE 0 END)+(CASE WHEN available=1 THEN 0 ELSE 10 END)+seriesConfidence AS priority FROM filtered),
            groups AS (SELECT groupKey,count(*) AS groupCount,min(priority) AS priority FROM ranked GROUP BY groupKey),
            ordered AS (SELECT r.groupKey,min(r.seriesOrder) AS seriesOrder FROM ranked r JOIN groups g ON g.groupKey=r.groupKey AND g.priority=r.priority GROUP BY r.groupKey),
            representatives AS (SELECT r.groupKey,min(r.id) AS id FROM ranked r JOIN groups g ON g.groupKey=r.groupKey AND g.priority=r.priority JOIN ordered o ON o.groupKey=r.groupKey AND o.seriesOrder=r.seriesOrder GROUP BY r.groupKey)
            SELECT f.json,f.deleted,f.available,f.downloadVersion,f.downloadFile,f.downloadBytes,f.groupKey,f.groupTitle,g.groupCount
            FROM ranked f JOIN representatives r ON r.groupKey=f.groupKey AND r.id=f.id JOIN groups g ON g.groupKey=f.groupKey
            ORDER BY $sort,f.id LIMIT 25 OFFSET ?"""
    }
}
