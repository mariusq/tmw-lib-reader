use super::*;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncRequest {
    pub catalog_id: String,
    #[serde(default)]
    pub history_version: Option<i64>,
    #[serde(default)]
    pub epoch: Option<String>,
    #[serde(default)]
    pub cursor: i64,
    #[serde(default)]
    pub operations: Vec<Operation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub id: String,
    pub sequence: i64,
    pub book_id: String,
    pub kind: String,
    #[serde(default)]
    pub entity_id: String,
    pub content_version: Option<String>,
    #[serde(default)]
    pub fields: serde_json::Map<String, Value>,
    #[serde(default)]
    pub deleted: bool,
}
fn identifier(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
#[cfg(test)]
pub fn remove_test_schema(c: &Connection) {
    c.execute_batch("DROP TRIGGER IF EXISTS companion_history_insert; DROP TRIGGER IF EXISTS companion_history_update; DROP TRIGGER IF EXISTS companion_history_synced_insert; DROP TRIGGER IF EXISTS companion_history_delete; DROP TABLE IF EXISTS companion_history_tombstones; DROP INDEX IF EXISTS lookup_history_sync_id; ALTER TABLE lookup_history DROP COLUMN sync_id; ALTER TABLE lookup_history DROP COLUMN public_book_id; ALTER TABLE lookup_history DROP COLUMN content_version; ALTER TABLE lookup_history DROP COLUMN dictionary_id; ALTER TABLE lookup_history DROP COLUMN dictionary_entry_id; ALTER TABLE lookup_history DROP COLUMN dictionary_label;").unwrap();
    c.execute_batch("DROP TRIGGER companion_progress_insert; DROP TRIGGER companion_progress_update; DROP TRIGGER companion_passage_insert; DROP TRIGGER companion_passage_update; DROP TRIGGER companion_passage_delete; DROP TABLE companion_content_versions; DROP TABLE companion_user_changes; DROP TABLE companion_user_receipts; DROP TABLE companion_user_devices; DROP TABLE companion_passage_tombstones; DROP INDEX saved_passages_sync_id; ALTER TABLE saved_passages DROP COLUMN sync_id; ALTER TABLE saved_passages DROP COLUMN content_version; ALTER TABLE reading_progress DROP COLUMN content_version;").unwrap();
}
impl Database {
    pub fn forget_content_version(&self, id: i64) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute(
            "DELETE FROM companion_content_versions WHERE book_id=?1",
            [id],
        )?;
        Ok(())
    }
    pub fn open_api_writer(path: &Path) -> rusqlite::Result<Self> {
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        connection.busy_timeout(std::time::Duration::from_millis(750))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }
    pub fn remember_content_version(
        &self,
        id: i64,
        version: &str,
        size: i64,
        modified: i64,
    ) -> rusqlite::Result<()> {
        self.connection.lock().unwrap().execute("INSERT INTO companion_content_versions VALUES(?1,?2,?3,?4) ON CONFLICT(book_id) DO UPDATE SET version=excluded.version,source_size=excluded.source_size,source_modified=excluded.source_modified",params![id,version,size,modified])?;
        Ok(())
    }
    pub fn user_sync(&self, device: &str, q: SyncRequest) -> Result<Value, &'static str> {
        if q.cursor < 0 || q.operations.len() > 16 {
            return Err("sync_bounds");
        }
        for op in &q.operations {
            if op.id.is_empty()
                || op.id.len() > 80
                || op.sequence <= 0
                || (!identifier(&op.book_id) && !(op.kind == "history" && op.book_id.is_empty()))
                || !["progress", "passage", "history"].contains(&op.kind.as_str())
                || (op.kind == "history" && q.history_version != Some(1))
                || (matches!(op.kind.as_str(), "passage" | "history") && !identifier(&op.entity_id))
                || op.fields.len() > if op.kind == "history" { 9 } else { 6 }
            {
                return Err("sync_bounds");
            }
            for (k, v) in &op.fields {
                if op.kind == "progress" && (k != "locationCfi" || v.as_str().is_none()) {
                    return Err("sync_bounds");
                }
                let max = match k.as_str() {
                    "dictionaryId" | "dictionaryEntryId" if op.kind == "history" => 128,
                    "dictionaryLabel" if op.kind == "history" => 256,
                    "lookedUpAt" if op.kind == "history" => 32,
                    "surface" => 1024,
                    "headword" | "reading" => 1024,
                    "sentence" => 16000,
                    "note" => 8000,
                    "locationCfi" => 4096,
                    _ => return Err("sync_bounds"),
                };
                if !v.is_null() && v.as_str().is_none_or(|s| s.len() > max) {
                    return Err("sync_bounds");
                }
            }
        }
        let mut c = self.connection.lock().unwrap();
        let tx = c
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| "catalog_busy")?;
        let namespace: String = tx
            .query_row(
                "SELECT value FROM app_settings WHERE key='companion_catalog_id'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "catalog_busy")?;
        if namespace != q.catalog_id {
            return Err("catalog_changed");
        }
        let epoch: String = tx
            .query_row("SELECT epoch FROM companion_revision", [], |r| r.get(0))
            .map_err(|_| "catalog_busy")?;
        if q.epoch.as_ref().is_some_and(|e| e != &epoch) {
            return Err("cursor_reset");
        }
        let high: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(sequence),0) FROM companion_user_changes",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "catalog_busy")?;
        if q.cursor > high {
            return Err("cursor_reset");
        }
        let mut acknowledged = Vec::new();
        let mut rejected = Vec::new();
        for op in q.operations {
            let prior:Option<Option<String>>=tx.query_row("SELECT reason FROM companion_user_receipts WHERE device_id=?1 AND operation_id=?2",params![device,op.id],|r|r.get(0)).optional().map_err(|_|"catalog_busy")?;
            if let Some(reason) = prior {
                if let Some(reason) = reason {
                    rejected.push(json!({"id":op.id,"reason":reason}))
                } else {
                    acknowledged.push(op.id)
                };
                continue;
            }
            let last: i64 = tx
                .query_row(
                    "SELECT last_sequence FROM companion_user_devices WHERE device_id=?1",
                    [device],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| "catalog_busy")?
                .unwrap_or(0);
            if op.sequence <= last {
                return Err("operation_order");
            }
            let id: Option<i64> = tx
                .query_row(
                    "SELECT book_id FROM companion_books WHERE public_id=?1",
                    [&op.book_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|_| "catalog_busy")?;
            let reason = if op.kind == "history" {
                apply_history(&tx, id, &op).map_err(|_| "catalog_busy")?
            } else if let Some(id) = id {
                apply(&tx, id, &op).map_err(|_| "catalog_busy")?
            } else {
                Some("book_deleted")
            };
            tx.execute(
                "INSERT INTO companion_user_receipts VALUES(?1,?2,?3,?4)",
                params![device, op.id, op.sequence, reason],
            )
            .map_err(|_| "catalog_busy")?;
            tx.execute("INSERT INTO companion_user_devices VALUES(?1,?2) ON CONFLICT(device_id) DO UPDATE SET last_sequence=excluded.last_sequence",params![device,op.sequence]).map_err(|_|"catalog_busy")?;
            if let Some(reason) = reason {
                rejected.push(json!({"id":op.id,"reason":reason}))
            } else {
                acknowledged.push(op.id)
            };
        }
        let candidates=tx.prepare("SELECT sequence,book_id,kind,entity_id,deleted,content_version,fields FROM companion_user_changes WHERE sequence>?1 ORDER BY sequence LIMIT 50").map_err(|_|"catalog_busy")?.query_map([q.cursor],|r|{let fields:String=r.get(6)?;Ok(json!({"sequence":r.get::<_,i64>(0)?,"bookId":r.get::<_,String>(1)?,"kind":r.get::<_,String>(2)?,"entityId":r.get::<_,String>(3)?,"deleted":r.get::<_,bool>(4)?,"contentVersion":r.get::<_,Option<String>>(5)?,"fields":serde_json::from_str::<Value>(&fields).unwrap_or(Value::Null)}))}).map_err(|_|"catalog_busy")?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_|"catalog_busy")?;
        let mut scanned_cursor = q.cursor;
        let mut changes = Vec::new();
        let mut bytes = 0usize;
        for item in candidates {
            if item["kind"] == "history" && q.history_version != Some(1) { scanned_cursor=item["sequence"].as_i64().unwrap(); continue; }
            let size = serde_json::to_vec(&item)
                .map_err(|_| "metadata_bounds")?
                .len();
            if bytes + size > 950_000 {
                if changes.is_empty() {
                    return Err("metadata_bounds");
                }
                break;
            }
            scanned_cursor=item["sequence"].as_i64().unwrap();
            bytes += size;
            changes.push(item);
        }
        let cursor = scanned_cursor;
        let high: i64 = tx
            .query_row(
                "SELECT COALESCE(MAX(sequence),0) FROM companion_user_changes",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "catalog_busy")?;
        tx.commit().map_err(|_| "catalog_busy")?;
        Ok(
            json!({"protocolVersion":3,"catalogId":namespace,"epoch":epoch,"cursor":cursor,"highWater":high,"hasMore":cursor<high,"acknowledged":acknowledged,"rejected":rejected,"changes":changes}),
        )
    }
}
// History is an immutable archive, not a new EPUB anchor. Missing/replaced sources
// must not prevent preserving a completed offline lookup.
fn apply_history(tx: &rusqlite::Transaction<'_>, book: Option<i64>, op: &Operation) -> rusqlite::Result<Option<&'static str>> {
    if op.content_version.as_ref().is_some_and(|v| !v.strip_prefix("sha256-").is_some_and(|digest| digest.len()==64 && digest.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))) { return Ok(Some("invalid_history")); }
    let dead: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM companion_history_tombstones WHERE entity_id=?1)",[&op.entity_id],|r|r.get(0))?;
    if dead { return Ok(if op.deleted {None} else {Some("history_deleted")}); }
    let existing: Option<(i64,String)>=tx.query_row("SELECT id,public_book_id FROM lookup_history WHERE sync_id=?1",[&op.entity_id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((_,ref public))=existing { if public != &op.book_id {return Ok(Some("entity_book_mismatch"));} }
    if op.deleted {
        if let Some((id,_))=existing {tx.execute("DELETE FROM lookup_history WHERE id=?1",[id])?;} else {
            tx.execute("INSERT INTO companion_history_tombstones VALUES(?1)",[&op.entity_id])?;
            tx.execute("INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) VALUES(?1,'history',?2,1,?3,'{}')",params![op.book_id,op.entity_id,op.content_version])?;
        }
        return Ok(None);
    }
    if existing.is_some() {return Ok(Some("immutable_history"));}
    let text=|key:&str|op.fields.get(key).and_then(Value::as_str).unwrap_or("");
    let optional=|key:&str|op.fields.get(key).and_then(Value::as_str);
    let at=match text("lookedUpAt").parse::<i64>() {Ok(n) if n>=0=>n,_=>return Ok(Some("invalid_history"))};
    if text("surface").trim().is_empty() || text("surface").chars().count()>256 || text("headword").chars().count()>256 || text("reading").chars().count()>256 || text("sentence").chars().count()>4000 || text("dictionaryId").is_empty() || op.fields.contains_key("note") || (!text("locationCfi").is_empty() && !text("locationCfi").starts_with("epubcfi(")) {return Ok(Some("invalid_history"));}
    let headword=optional("headword");let reading=optional("reading");
    let identity=serde_json::to_string(&(if headword.is_some() {"entry"}else{"query"},normalize_for_search(headword.unwrap_or(text("surface"))),reading.map(normalize_for_search))).map_err(|_|rusqlite::Error::InvalidQuery)?;
    let search=normalize_for_search(&format!("{} {} {} {}",text("surface"),text("headword"),text("reading"),text("sentence")));
    tx.execute("INSERT INTO lookup_history(identity,surface,headword,reading,search_text,book_id,location_cfi,sentence,looked_up_at,sync_id,public_book_id,content_version,dictionary_id,dictionary_entry_id,dictionary_label,source_size,source_modified) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,(SELECT source_size FROM companion_content_versions WHERE book_id=?6 AND version=?12),(SELECT source_modified FROM companion_content_versions WHERE book_id=?6 AND version=?12))",params![identity,text("surface"),headword,reading,search,book,text("locationCfi"),text("sentence"),at,op.entity_id,op.book_id,op.content_version,text("dictionaryId"),optional("dictionaryEntryId"),optional("dictionaryLabel")])?;
    tx.execute("DELETE FROM lookup_history WHERE id IN (SELECT id FROM lookup_history ORDER BY id DESC LIMIT -1 OFFSET 10000)",[])?;
    Ok(None)
}

fn apply(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
    op: &Operation,
) -> rusqlite::Result<Option<&'static str>> {
    let text = |k: &str| op.fields.get(k).and_then(Value::as_str).unwrap_or("");
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM saved_passages WHERE sync_id=?1 AND book_id=?2",
            params![op.entity_id, id],
            |r| r.get(0),
        )
        .optional()?;
    if op.kind == "passage"
        && tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM saved_passages WHERE sync_id=?1 AND book_id<>?2)",
            params![op.entity_id, id],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Ok(Some("entity_book_mismatch"));
    }
    if op.kind == "passage" {
        let dead: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM companion_passage_tombstones WHERE entity_id=?1)",
            [&op.entity_id],
            |r| r.get(0),
        )?;
        if dead {
            return Ok(if op.deleted {
                None
            } else {
                Some("passage_deleted")
            });
        }
        if op.deleted {
            if let Some(pid) = existing {
                tx.execute("DELETE FROM saved_passages WHERE id=?1", [pid])?;
            } else {
                tx.execute(
                    "INSERT INTO companion_passage_tombstones VALUES(?1)",
                    [&op.entity_id],
                )?;
                tx.execute("INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) VALUES(?1,'passage',?2,1,?3,'{}')",params![op.book_id,op.entity_id,op.content_version])?;
            }
            return Ok(None);
        }
    }
    let anchor =
        op.kind == "progress" || existing.is_none() || op.fields.contains_key("locationCfi");
    if anchor {
        let current:Option<String>=tx.query_row("SELECT v.version FROM companion_content_versions v JOIN books b ON b.id=v.book_id WHERE b.id=?1 AND b.file_size=v.source_size AND b.modified_time=v.source_modified AND b.extraction_status<>'unavailable'",[id],|r|r.get(0)).optional()?;
        if current.is_none() || current != op.content_version {
            return Ok(Some("content_version_changed"));
        }
        if !text("locationCfi").is_empty() && !text("locationCfi").starts_with("epubcfi(") {
            return Ok(Some("invalid_anchor"));
        }
    }
    if op.kind == "progress" {
        if op.deleted || !op.fields.contains_key("locationCfi") {
            return Ok(Some("invalid_progress"));
        }
        tx.execute("INSERT INTO reading_progress(book_id,location_cfi,updated_at,content_version) VALUES(?1,?2,?3,?4) ON CONFLICT(book_id) DO UPDATE SET location_cfi=excluded.location_cfi,updated_at=excluded.updated_at,content_version=excluded.content_version",params![id,text("locationCfi"),unix_timestamp(),op.content_version])?;
    } else if let Some(pid) = existing {
        if op
            .fields
            .keys()
            .any(|k| !matches!(k.as_str(), "sentence" | "note"))
        {
            return Ok(Some("immutable_passage_field"));
        }
        for (field, column) in [("sentence", "sentence"), ("note", "note")] {
            if op.fields.contains_key(field) {
                tx.execute(
                    &format!("UPDATE saved_passages SET {column}=?2 WHERE id=?1"),
                    params![pid, text(field)],
                )?;
            }
        }
    } else {
        if text("surface").trim().is_empty() {
            return Ok(Some("invalid_passage"));
        }
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM saved_passages WHERE book_id=?1 AND location_cfi=?2 AND surface=?3)",params![id,text("locationCfi"),text("surface")],|r|r.get(0))?;
        if duplicate {
            return Ok(Some("duplicate_passage"));
        }
        tx.execute("INSERT INTO saved_passages(book_id,surface,headword,reading,sentence,note,location_cfi,source_size,source_modified,created_at,sync_id,content_version) SELECT id,?2,?3,?4,?5,?6,?7,file_size,modified_time,?8,?9,?10 FROM books WHERE id=?1",params![id,text("surface"),op.fields.get("headword").and_then(Value::as_str),op.fields.get("reading").and_then(Value::as_str),text("sentence"),text("note"),text("locationCfi"),unix_timestamp(),op.entity_id,op.content_version])?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Database, i64, String, String) {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("catalog.sqlite3")).unwrap();
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "missing",
                display_name: "test",
            })
            .unwrap();
        let book = db
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: "missing/book.epub",
                parent_folder_path: "missing",
                file_name: "本.epub",
                file_size: 4,
                modified_time: 1,
            })
            .unwrap();
        db.remember_content_version(book.id, "sha256-test", 4, 1)
            .unwrap();
        let public = db.public_id(book.id).unwrap();
        let namespace = db.setting("companion_catalog_id").unwrap().unwrap();
        (temp, db, book.id, public, namespace)
    }
    fn request(namespace: &str, ops: Value) -> SyncRequest {
        serde_json::from_value(json!({"catalogId":namespace,"operations":ops})).unwrap()
    }
    fn op(id: &str, sequence: i64, book: &str, kind: &str, fields: Value) -> Value {
        json!({"id":id,"sequence":sequence,"bookId":book,"kind":kind,"entityId":"12345678901234567890123456789012","contentVersion":"sha256-test","fields":fields})
    }
    #[test]
    fn history_offline_archive_retry_clear_retention_and_legacy_capability() {
        let (_temp, db, id, book, ns)=setup();
        let event=|key:&str,sequence:i64,entity:&str|json!({"id":key,"sequence":sequence,"bookId":book,"kind":"history","entityId":entity,"contentVersion":format!("sha256-{}","a".repeat(64)),"fields":{"surface":"読んだ","headword":"読む","reading":"よむ","sentence":"本を読んだ。","locationCfi":"epubcfi(/6/2)","lookedUpAt":"1","dictionaryId":"jmdict-eng:fixture","dictionaryEntryId":null}});
        let call=|ops:Value| {let mut q=request(&ns,ops);q.history_version=Some(1);db.user_sync("phone",q).unwrap()};
        db.connection.lock().unwrap().execute("UPDATE books SET extraction_status='unavailable' WHERE id=?1",[id]).unwrap();
        let mut malformed=event("bad-version",1,"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");malformed["contentVersion"]=json!("sha256-old");
        let mut bad_request=request(&ns,json!([malformed]));bad_request.history_version=Some(1);
        let bad=db.user_sync("malformed-client",bad_request).unwrap();
        assert_eq!(bad["rejected"][0]["reason"],"invalid_history");
        assert!(bad["changes"].as_array().unwrap().is_empty());
        let e=event("h1",1,"11111111111111111111111111111111");
        assert_eq!(call(json!([e.clone()]))["acknowledged"],json!(["h1"]));
        call(json!([e]));
        assert_eq!(db.lookup_history("読む",0).unwrap().len(),1);
        assert!(db.lookup_history_location(db.lookup_history("",0).unwrap()[0].id).is_err());
        call(json!([event("h2",2,"22222222222222222222222222222222")]));
        assert_eq!(db.lookup_history("",0).unwrap()[0].count,2);
        let legacy=db.user_sync("old",request(&ns,json!([]))).unwrap();
        assert!(legacy["changes"].as_array().unwrap().is_empty());
        assert_eq!(legacy["cursor"],legacy["highWater"]);
        db.clear_lookup_history().unwrap();
        assert!(db.lookup_history("",0).unwrap().is_empty());
        let revived=call(json!([event("h3",3,"11111111111111111111111111111111")]));
        assert_eq!(revived["rejected"][0]["reason"],"history_deleted");
        let mut orphan=event("h4",4,"44444444444444444444444444444444");orphan["bookId"]=json!("");
        call(json!([orphan]));
        assert_eq!(db.lookup_history("",0).unwrap().len(),1);
        {
            let c=db.connection.lock().unwrap();
            c.execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO lookup_history(identity,surface,search_text,location_cfi,sentence,looked_up_at) SELECT 'fixture','fixture','fixture','','',1 FROM n;").unwrap();
        }
        call(json!([event("h5",5,"55555555555555555555555555555555")]));
        let c=db.connection.lock().unwrap();
        assert_eq!(c.query_row("SELECT COUNT(*) FROM lookup_history",[],|r|r.get::<_,i64>(0)).unwrap(),10000);
        assert!(c.query_row("SELECT EXISTS(SELECT 1 FROM companion_history_tombstones WHERE entity_id='44444444444444444444444444444444')",[],|r|r.get::<_,bool>(0)).unwrap());
    }

    #[test]
    fn ordered_retry_field_merge_tombstone_and_desktop_journal() {
        let (_temp, db, id, book, ns) = setup();
        let create = op(
            "one",
            1,
            &book,
            "passage",
            json!({"surface":"本","sentence":"sentence","note":"original","locationCfi":"epubcfi(/6/2)"}),
        );
        let first = db
            .user_sync("phone", request(&ns, json!([create.clone()])))
            .unwrap();
        assert_eq!(first["changes"].as_array().unwrap().len(), 1);
        let retry = db
            .user_sync("phone", request(&ns, json!([create])))
            .unwrap();
        assert_eq!(retry["changes"].as_array().unwrap().len(), 1);
        db.user_sync(
            "phone",
            request(
                &ns,
                json!([op("two", 2, &book, "passage", json!({"note":"phone"}))]),
            ),
        )
        .unwrap();
        let pid = db.saved_passages(Some(id), 0).unwrap()[0].id;
        db.edit_passage(pid, "desktop", "phone").unwrap();
        db.user_sync(
            "other",
            request(
                &ns,
                json!([op("three", 1, &book, "passage", json!({"note":null}))]),
            ),
        )
        .unwrap();
        let saved = db.saved_passages(Some(id), 0).unwrap();
        assert_eq!(saved[0].sentence, "desktop");
        assert_eq!(saved[0].note, "");
        db.delete_passage(pid).unwrap();
        let rejected = db
            .user_sync(
                "phone",
                request(
                    &ns,
                    json!([op("four", 3, &book, "passage", json!({"note":"revive"}))]),
                ),
            )
            .unwrap();
        assert_eq!(rejected["rejected"][0]["reason"], "passage_deleted");
        assert!(db.saved_passages(Some(id), 0).unwrap().is_empty());
        let progress = json!([
            op(
                "p1",
                4,
                &book,
                "progress",
                json!({"locationCfi":"epubcfi(/6/4)"})
            ),
            op(
                "p2",
                5,
                &book,
                "progress",
                json!({"locationCfi":"epubcfi(/6/2)"})
            )
        ]);
        db.user_sync("phone", request(&ns, progress)).unwrap();
        assert_eq!(db.reading_location(id).unwrap().unwrap(), "epubcfi(/6/2)");
        db.save_reading_location(id, "epubcfi(/6/8)").unwrap();
        let pull = db.user_sync("phone", request(&ns, json!([]))).unwrap();
        assert_eq!(
            pull["changes"].as_array().unwrap().last().unwrap()["fields"]["locationCfi"],
            "epubcfi(/6/8)"
        );
    }
    #[test]
    fn replacement_unavailable_rejection_and_atomic_batch_rollback() {
        let (_temp, db, id, book, ns) = setup();
        let batch = json!([
            op(
                "first",
                2,
                &book,
                "progress",
                json!({"locationCfi":"epubcfi(/6/2)"})
            ),
            op(
                "older",
                1,
                &book,
                "progress",
                json!({"locationCfi":"epubcfi(/6/4)"})
            )
        ]);
        assert_eq!(
            db.user_sync("phone", request(&ns, batch)).unwrap_err(),
            "operation_order"
        );
        assert!(db.reading_location(id).unwrap().is_none());
        db.connection
            .lock()
            .unwrap()
            .execute("UPDATE books SET file_size=7 WHERE id=?1", [id])
            .unwrap();
        let result = db
            .user_sync(
                "phone",
                request(
                    &ns,
                    json!([op(
                        "new",
                        1,
                        &book,
                        "progress",
                        json!({"locationCfi":"epubcfi(/6/2)"})
                    )]),
                ),
            )
            .unwrap();
        assert_eq!(result["rejected"][0]["reason"], "content_version_changed");
        db.connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE books SET extraction_status='unavailable' WHERE id=?1",
                [id],
            )
            .unwrap();
        assert!(db.reading_location(id).unwrap().is_none());
    }
    #[test]
    fn journal_pages_and_pending_receipts_survive_restart() {
        let (temp, db, id, book, ns) = setup();
        for n in 0..65 {
            db.save_reading_location(id, &format!("epubcfi(/6/{n})"))
                .unwrap();
        }
        let first = db.user_sync("phone", request(&ns, json!([op("accepted-after-page",1,&book,"progress",json!({"locationCfi":"epubcfi(/6/99)"}))]))).unwrap();
        assert_eq!(first["changes"].as_array().unwrap().len(), 50);
        assert_eq!(first["hasMore"], true);
        assert_eq!(first["highWater"],66);
        assert_eq!(first["acknowledged"],json!(["accepted-after-page"]));
        assert!(first["cursor"].as_i64().unwrap()<first["highWater"].as_i64().unwrap());
        drop(db);
        let db = Database::open(&temp.path().join("catalog.sqlite3")).unwrap();
        let mut q = request(&ns, json!([]));
        q.cursor = first["cursor"].as_i64().unwrap();
        let second = db.user_sync("phone", q).unwrap();
        assert_eq!(second["changes"].as_array().unwrap().len(), 16);
        assert_eq!(second["hasMore"], false);
        assert_eq!(second["cursor"],second["highWater"]);
    }
}
