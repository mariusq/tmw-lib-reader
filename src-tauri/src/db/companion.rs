use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogCursor {
    pub expires: Option<i64>,
    pub epoch: Option<String>,
    pub revision: Option<i64>,
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub after: String,
    #[serde(default)]
    pub delta: bool,
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    fn cursor(
        epoch: Option<String>,
        revision: Option<i64>,
        since: i64,
        after: String,
        delta: bool,
    ) -> CatalogCursor {
        CatalogCursor {
            expires: Some(unix_timestamp() + 899),
            epoch,
            revision,
            since,
            after,
            delta,
        }
    }
    #[test]
    fn revisions_pages_tombstones_and_rollback_are_consistent() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("catalog.sqlite3")).unwrap();
        let root = db
            .add_library_root(crate::models::library_root::NewLibraryRoot {
                path: "generated-only",
                display_name: "fixture",
            })
            .unwrap();
        for n in 0..64 {
            db.upsert_scanned_book(crate::models::book::NewBook {
                library_root_id: root.id,
                file_path: &format!("generated-only/{n}.epub"),
                parent_folder_path: "generated-only",
                file_name: &format!("{n}.epub"),
                file_size: 123,
                modified_time: 1,
            })
            .unwrap();
        }
        let first = db
            .catalog_page(cursor(None, None, 0, "".into(), false))
            .unwrap();
        assert_eq!(first["items"].as_array().unwrap().len(), 50);
        let epoch = first["epoch"].as_str().unwrap().to_string();
        let rev = first["revision"].as_i64().unwrap();
        let second = db
            .catalog_page(cursor(
                Some(epoch.clone()),
                Some(rev),
                0,
                first["next"].as_str().unwrap().into(),
                false,
            ))
            .unwrap();
        assert_eq!(second["items"].as_array().unwrap().len(), 14);
        assert!(second["next"].is_null());
        {
            let mut conn = db.connection.lock().unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute("UPDATE books SET discovered_title='Rollback'", [])
                .unwrap();
            // Drop rolls back both metadata and the revision journal.
        }
        assert!(db
            .catalog_page(cursor(Some(epoch.clone()), Some(rev), 0, "".into(), false))
            .is_ok());
        let backup = temp.path().join("backup.sqlite3");
        db.backup_to(&backup).unwrap();
        db.remove_library_root(root.id).unwrap();
        assert_eq!(
            db.catalog_page(cursor(Some(epoch.clone()), Some(rev), 0, "".into(), false))
                .unwrap_err(),
            "restart_snapshot"
        );
        let delta = db
            .catalog_page(cursor(Some(epoch.clone()), None, rev, "".into(), true))
            .unwrap();
        assert_eq!(delta["items"].as_array().unwrap().len(), 50);
        assert!(delta["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["deleted"] == true));
        db.restore_from(&backup).unwrap();
        assert_eq!(
            db.catalog_page(cursor(Some(epoch), Some(rev), rev, "".into(), true))
                .unwrap_err(),
            "restart_snapshot"
        );
        let restored = db
            .catalog_page(cursor(None, None, 0, "".into(), false))
            .unwrap();
        assert_eq!(restored["catalogId"], first["catalogId"]);
    }
}
pub fn cover_key(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    let m = std::fs::metadata(path).ok()?;
    let stamp = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!(
        "mobile-jpeg75-v1-{:x}",
        Sha256::digest(format!("{path}:{}:{stamp}", m.len()).as_bytes())
    ))
}
impl Database {
    /// A short read transaction freezes each page. Subsequent pages must observe
    /// the same revision; never silently page across concurrent catalog writes.
    pub fn catalog_page(&self, q: CatalogCursor) -> Result<Value, &'static str> {
        static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        if q.since < 0
            || !(q.after.is_empty() || q.after.len() == 32)
            || !q.after.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("invalid_cursor");
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "catalog_busy")?
            .as_secs() as i64;
        if (!q.after.is_empty()
            && (q.epoch.is_none() || q.revision.is_none() || q.expires.is_none()))
            || q.expires.is_some_and(|e| e <= now || e > now + 900)
        {
            return Err("restart_snapshot");
        }
        let mut c = self.connection.lock().unwrap();
        let tx = c.transaction().map_err(|_| "catalog_busy")?;
        let (rev, floor, epoch): (i64, i64, String) = tx
            .query_row(
                "SELECT revision,floor,epoch FROM companion_revision",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|_| "catalog_busy")?;
        let epoch = format!(
            "{}-{}",
            epoch,
            SESSION.get_or_init(|| uuid::Uuid::new_v4().to_string())
        );
        if q.epoch.as_ref().is_some_and(|e| e != &epoch)
            || q.revision.is_some_and(|r| r != rev)
            || q.since > rev
            || (q.delta && q.since < floor)
        {
            return Err("restart_snapshot");
        }
        let mut items = Vec::new();
        let sql = if q.delta {
            "SELECT public_id FROM companion_changes WHERE revision>?1 AND public_id>?2 ORDER BY public_id LIMIT 50"
        } else {
            "SELECT public_id FROM companion_books WHERE ?1>=0 AND public_id>?2 ORDER BY public_id LIMIT 50"
        };
        let ids = tx
            .prepare(sql)
            .map_err(|_| "catalog_busy")?
            .query_map(params![q.since, q.after], |r| r.get::<_, String>(0))
            .map_err(|_| "catalog_busy")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| "catalog_busy")?;
        for public in &ids {
            let id = tx
                .query_row(
                    "SELECT book_id FROM companion_books WHERE public_id=?1",
                    [public],
                    |r| r.get::<_, i64>(0),
                )
                .optional()
                .map_err(|_| "catalog_busy")?;
            if let Some(id) = id {
                let mut stmt=tx.prepare("SELECT
                 COALESCE(NULLIF(o.title,''),NULLIF(b.discovered_title,''),CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name,1,length(b.file_name)-5) ELSE b.file_name END) AS title,
                 COALESCE(NULLIF(o.creator,''),b.discovered_creator,'') AS creator,
                 COALESCE(NULLIF(o.series_name,''),b.discovered_series,'') AS series,
                 COALESCE(NULLIF(o.volume_label,''),b.discovered_series_index,'') AS volume,
                 b.discovered_language AS language,b.discovered_identifier AS identifier,
                 b.file_name AS fileName,b.file_size AS bytes,b.modified_time AS modified,
                 b.created_at AS dateAdded,b.reading_status AS readingStatus,
                 b.extraction_status AS extractionStatus,o.notes AS notes,
                 d.title_reading AS titleReading,d.creator_reading AS creatorReading,d.series_reading AS seriesReading,d.aliases_normalized AS aliases,
                 ro.reading AS readingOverride,ro.aliases AS aliasesOverride,
                 b.discovered_title AS discoveredTitle,b.discovered_creator AS discoveredCreator,b.discovered_series AS discoveredSeries,b.discovered_series_index AS discoveredVolume,
                 o.title AS titleOverride,o.creator AS creatorOverride,o.series_name AS seriesOverride,o.volume_label AS volumeOverride,
                 COALESCE(NULLIF(o.cover_path,''),b.discovered_cover_path) AS coverPath,
                 COALESCE(d.title_normalized,'')||' '||COALESCE(d.creator_normalized,'')||' '||COALESCE(d.series_normalized,'')||' '||COALESCE(d.file_name_normalized,'')||' '||COALESCE(d.parent_folder_normalized,'')||' '||COALESCE(d.tags_normalized,'')||' '||COALESCE(d.title_reading,'')||' '||COALESCE(d.creator_reading,'')||' '||COALESCE(d.series_reading,'')||' '||COALESCE(d.file_name_reading,'')||' '||COALESCE(d.aliases_normalized,'')||' '||COALESCE(d.title_romaji,'')||' '||COALESCE(d.creator_romaji,'')||' '||COALESCE(d.series_romaji,'')||' '||COALESCE(d.file_name_romaji,'')||' '||COALESCE(d.aliases_romaji,'') AS search,
                 b.library_root_id AS rootId
                 FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id LEFT JOIN book_search_documents d ON d.book_id=b.id LEFT JOIN book_reading_overrides ro ON ro.book_id=b.id WHERE b.id=?1").map_err(|_|"catalog_busy")?;
                let names = stmt
                    .column_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>();
                let mut value = stmt
                    .query_row([id], |r| {
                        let mut obj = serde_json::Map::new();
                        for (i, name) in names.iter().enumerate() {
                            use rusqlite::types::ValueRef;
                            let v = match r.get_ref(i)? {
                                ValueRef::Text(s) if s.len() <= 65536 => {
                                    json!(String::from_utf8_lossy(s))
                                }
                                ValueRef::Text(_) => return Err(rusqlite::Error::InvalidQuery),
                                ValueRef::Integer(n) => json!(n),
                                _ => Value::Null,
                            };
                            obj.insert(name.clone(), v);
                        }
                        Ok(Value::Object(obj))
                    })
                    .map_err(|_| "catalog_busy")?;
                value["id"] = json!(public);
                value["contentVersion"] = Value::Null;
                value["contentVersionEndpoint"] = json!(format!("/v1/books/{public}/content"));
                value["available"] = json!(value["extractionStatus"] != "unavailable");
                value["coverVersion"] = json!(value["coverPath"].as_str().and_then(cover_key));
                value.as_object_mut().unwrap().remove("coverPath");
                for (key,sql) in [("tags","SELECT t.id,t.name FROM tags t JOIN book_tags m ON m.tag_id=t.id WHERE m.book_id=?1 ORDER BY t.id"),("collections","SELECT t.id,t.name FROM collections t JOIN collection_books m ON m.collection_id=t.id WHERE m.book_id=?1 ORDER BY t.id")] {
                    let entries=tx.prepare(sql).map_err(|_|"catalog_busy")?.query_map([id],|r|Ok(json!({"id":r.get::<_,i64>(0)?,"name":r.get::<_,String>(1)?}))).map_err(|_|"catalog_busy")?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"catalog_busy")?;
                    value[key]=json!(entries);
                }
                items.push(value);
                if serde_json::to_vec(&items)
                    .map_err(|_| "metadata_bounds")?
                    .len()
                    > 990_000
                {
                    return Err("metadata_bounds");
                }
            } else {
                items.push(json!({"id":public,"deleted":true}));
            }
        }
        let namespace: String = tx
            .query_row(
                "SELECT value FROM app_settings WHERE key='companion_catalog_id'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "catalog_busy")?;
        let next = if ids.len() == 50 {
            ids.last().cloned()
        } else {
            None
        };
        let response = json!({"protocolVersion":2,"catalogId":namespace,"epoch":epoch,"revision":rev,"expires":q.expires.unwrap_or(now+900),"items":items,"next":next});
        if response.to_string().len() > 1_000_000 {
            return Err("metadata_bounds");
        }
        tx.commit().map_err(|_| "catalog_busy")?;
        Ok(response)
    }
    /// Independent read-only connection: no startup migrations/index checks or shared UI mutex.
    pub fn open_api_reader(path: &Path) -> rusqlite::Result<Self> {
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(std::time::Duration::from_millis(750))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
    pub fn public_id(&self, id: i64) -> rusqlite::Result<String> {
        self.connection.lock().unwrap().query_row(
            "SELECT public_id FROM companion_books WHERE book_id=?1",
            [id],
            |r| r.get(0),
        )
    }
    pub fn private_id(&self, id: &str) -> rusqlite::Result<Option<i64>> {
        self.connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT book_id FROM companion_books WHERE public_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
    }
}
