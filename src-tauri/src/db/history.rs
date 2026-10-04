use super::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupRecord {
    pub surface: String,
    pub entry_id: Option<i64>,
    pub book_id: Option<i64>,
    pub location_cfi: String,
    pub sentence: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    #[ignore = "generated 10,000-lookup benchmark"]
    fn history_retention_benchmark() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        {
            let connection = database.connection.lock().unwrap();
            connection.execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO lookup_history(identity,surface,search_text,location_cfi,sentence,looked_up_at) SELECT 'fixture','本','本','','',1 FROM n;").unwrap();
            let mut statement = connection.prepare("EXPLAIN QUERY PLAN SELECT COUNT(*) FROM lookup_history WHERE identity='fixture'").unwrap();
            let plan: Vec<String> = statement
                .query_map([], |r| r.get(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert!(plan
                .iter()
                .any(|line| line.contains("idx_lookup_history_identity")));
        }
        let started = std::time::Instant::now();
        assert_eq!(database.lookup_history("", 0).unwrap().len(), 50);
        let recent = started.elapsed();
        let started = std::time::Instant::now();
        assert_eq!(database.lookup_history("本", 9950).unwrap().len(), 50);
        let search = started.elapsed();
        let started = std::time::Instant::now();
        database.record_lookup(&request("本", None)).unwrap();
        eprintln!("lookup_history rows=10000 recent_ms={:.3} search_last_page_ms={:.3} record_with_retention_ms={:.3}", recent.as_secs_f64()*1000.0, search.as_secs_f64()*1000.0, started.elapsed().as_secs_f64()*1000.0);
    }

    #[test]
    fn previous_catalog_migrates_without_losing_progress() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        database.set_setting("existing_preference", "kept").unwrap();
        database
            .connection
            .lock()
            .unwrap()
            .execute_batch("DROP TABLE lookup_history; DROP TABLE smart_shelves; DROP INDEX idx_books_reading_status; ALTER TABLE books DROP COLUMN reading_status; ALTER TABLE books DROP COLUMN completed_at; PRAGMA user_version=12;")
            .unwrap();
        let backup = directory.path().join("previous.sqlite3");
        database.backup_to(&backup).unwrap();
        database.restore_from(&backup).unwrap();
        assert!(database.lookup_history("", 0).unwrap().is_empty());
        assert_eq!(
            database.setting("existing_preference").unwrap().as_deref(),
            Some("kept")
        );
        database.record_lookup(&request("本", None)).unwrap();
    }

    fn request(surface: &str, entry_id: Option<i64>) -> LookupRecord {
        LookupRecord {
            surface: surface.into(),
            entry_id,
            book_id: None,
            location_cfi: String::new(),
            sentence: String::new(),
        }
    }

    #[test]
    fn identities_preferences_retention_and_backup_are_independent() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("catalog.sqlite3");
        let database = Database::open(&path).unwrap();
        database
            .replace_jmdict(
                "local",
                &[
                    crate::services::dictionary::ImportedEntry {
                        term: "読む".into(),
                        reading: Some("よむ".into()),
                        definitions: vec!["read".into()],
                        part_of_speech: vec![],
                    },
                    crate::services::dictionary::ImportedEntry {
                        term: "読む".into(),
                        reading: Some("別の読み".into()),
                        definitions: vec!["ambiguous".into()],
                        part_of_speech: vec![],
                    },
                ],
            )
            .unwrap();
        let entries = database.dictionary_lookup("読む").unwrap();
        assert_eq!(
            database
                .record_lookup(&request("読んだ", Some(entries[0].id)))
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            database
                .record_lookup(&request("読みます", Some(entries[0].id)))
                .unwrap(),
            Some(2)
        );
        assert_eq!(
            database
                .record_lookup(&request("読む", Some(entries[1].id)))
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            database.record_lookup(&request("読む", None)).unwrap(),
            Some(1)
        );
        assert_eq!(database.lookup_history("読みます", 0).unwrap()[0].count, 2);
        database
            .set_setting("lookup_history_enabled", "false")
            .unwrap();
        assert_eq!(
            database
                .record_lookup(&request("読んだ", Some(entries[0].id)))
                .unwrap(),
            None
        );
        assert_eq!(database.lookup_history("", 0).unwrap().len(), 4);
        let backup = directory.path().join("backup.sqlite3");
        database.backup_to(&backup).unwrap();
        database.clear_lookup_history().unwrap();
        assert!(database.lookup_history("", 0).unwrap().is_empty());
        database.restore_from(&backup).unwrap();
        drop(database);
        let database = Database::open(&path).unwrap();
        assert_eq!(database.lookup_history("", 0).unwrap().len(), 4);
        assert_eq!(
            database
                .setting("lookup_history_enabled")
                .unwrap()
                .as_deref(),
            Some("false")
        );
        database.replace_jmdict("local", &[]).unwrap();
        assert_eq!(database.lookup_history("", 0).unwrap().len(), 4);
        database
            .set_setting("lookup_history_enabled", "true")
            .unwrap();
        {
            let connection = database.connection.lock().unwrap();
            connection.execute_batch("WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO lookup_history(identity,surface,search_text,location_cfi,sentence,looked_up_at) SELECT 'fixture','fixture','fixture','','',1 FROM n;").unwrap();
        }
        database.record_lookup(&request("new", None)).unwrap();
        let connection = database.connection.lock().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM lookup_history", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            10000
        );
        drop(connection);
        assert!(database.lookup_history_location(-1).is_err());
    }

    #[test]
    fn anchors_rescans_clear_and_source_safety() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("source.epub");
        fs::write(&source, b"source fixture").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: directory.path().to_str().unwrap(),
                display_name: "Test",
            })
            .unwrap();
        let source_book = NewBook {
            library_root_id: root.id,
            file_path: source.to_str().unwrap(),
            parent_folder_path: directory.path().to_str().unwrap(),
            file_name: "source.epub",
            file_size: metadata.len() as i64,
            modified_time: metadata
                .modified()
                .unwrap()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        };
        let book = database.add_book(source_book.clone()).unwrap();
        let mut lookup = request("本", None);
        lookup.book_id = Some(book.id);
        lookup.location_cfi = "epubcfi(/6/2!/4/2/1:0)".into();
        lookup.sentence = "本。".into();
        database.record_lookup(&lookup).unwrap();
        let id = database.lookup_history("", 0).unwrap()[0].id;
        assert_eq!(
            database.lookup_history_location(id).unwrap(),
            lookup.location_cfi
        );
        database
            .save_reading_location(book.id, &lookup.location_cfi)
            .unwrap();
        let saved = crate::models::book::SavePassageRequest {
            book_id: book.id,
            surface: "本".into(),
            headword: None,
            reading: None,
            sentence: "本。".into(),
            note: "note".into(),
            location_cfi: lookup.location_cfi.clone(),
        };
        database.save_passage(&saved).unwrap();
        database.mark_missing_books(root.id, &[]).unwrap();
        assert!(!database.lookup_history("", 0).unwrap()[0].is_available);
        database.upsert_scanned_book(source_book).unwrap();
        assert!(database.lookup_history("", 0).unwrap()[0].is_available);
        database
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE lookup_history SET source_size=0 WHERE id=?1", [id])
            .unwrap();
        assert!(database
            .lookup_history_location(id)
            .unwrap_err()
            .contains("changed"));
        database.clear_lookup_history().unwrap();
        assert_eq!(database.saved_passages(None, 0).unwrap().len(), 1);
        assert_eq!(
            database.reading_location(book.id).unwrap().as_deref(),
            Some(lookup.location_cfi.as_str())
        );
        assert_eq!(fs::read(source).unwrap(), b"source fixture");
        lookup.surface.clear();
        assert!(database.record_lookup(&lookup).is_err());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupHistory {
    pub id: i64,
    pub surface: String,
    pub headword: Option<String>,
    pub reading: Option<String>,
    pub book_id: Option<i64>,
    pub book_title: Option<String>,
    pub location_cfi: String,
    pub sentence: String,
    pub looked_up_at: i64,
    pub count: i64,
    pub is_available: bool,
}

impl Database {
    pub fn record_lookup(&self, request: &LookupRecord) -> rusqlite::Result<Option<i64>> {
        if request.surface.trim().is_empty()
            || request.surface.chars().count() > 256
            || request.sentence.chars().count() > 4000
            || request.location_cfi.len() > 4096
            || (!request.location_cfi.is_empty() && !request.location_cfi.starts_with("epubcfi("))
            || (request.book_id.is_none() && !request.location_cfi.is_empty())
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        let enabled: Option<String> = transaction
            .query_row(
                "SELECT value FROM app_settings WHERE key='lookup_history_enabled'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        if enabled.as_deref() == Some("false") {
            return Ok(None);
        }
        let (headword, reading): (Option<String>, Option<String>) = match request.entry_id {
            Some(id) => transaction.query_row(
                "SELECT term,reading FROM dictionary_entries WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
            None => (None, None),
        };
        // Length-safe JSON tuple, with distinct namespaces for unresolved queries.
        let identity = serde_json::to_string(&(
            if headword.is_some() { "entry" } else { "query" },
            normalize_for_search(headword.as_deref().unwrap_or(&request.surface)),
            reading.as_deref().map(normalize_for_search),
        ))
        .map_err(|_| rusqlite::Error::InvalidQuery)?;
        let source: (Option<i64>, Option<i64>) = match request.book_id {
            Some(id) => transaction.query_row(
                "SELECT file_size,modified_time FROM books WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?,
            None => (None, None),
        };
        let search = normalize_for_search(&format!(
            "{} {} {}",
            request.surface,
            headword.as_deref().unwrap_or(""),
            reading.as_deref().unwrap_or("")
        ));
        transaction.execute("INSERT INTO lookup_history(identity,surface,headword,reading,search_text,book_id,location_cfi,sentence,source_size,source_modified,looked_up_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)", params![identity, request.surface, headword, reading, search, request.book_id, request.location_cfi, request.sentence, source.0, source.1, unix_timestamp()])?;
        // Counts describe the most recent 10,000 intentional lookups, never occurrences.
        transaction.execute("DELETE FROM lookup_history WHERE id IN (SELECT id FROM lookup_history ORDER BY id DESC LIMIT -1 OFFSET 10000)", [])?;
        let count = transaction.query_row(
            "SELECT COUNT(*) FROM lookup_history WHERE identity=?1",
            [identity],
            |r| r.get(0),
        )?;
        transaction.commit()?;
        Ok(Some(count))
    }

    pub fn lookup_history(&self, query: &str, offset: i64) -> rusqlite::Result<Vec<LookupHistory>> {
        if query.chars().count() > 256 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached("SELECT h.id,h.surface,h.headword,h.reading,h.book_id,COALESCE(o.title,b.discovered_title,b.file_name),h.location_cfi,h.sentence,h.looked_up_at,(SELECT COUNT(*) FROM lookup_history c WHERE c.identity=h.identity),COALESCE(b.extraction_status <> 'unavailable',0) FROM lookup_history h LEFT JOIN books b ON b.id=h.book_id LEFT JOIN book_overrides o ON o.book_id=b.id WHERE instr(h.search_text,?1)>0 ORDER BY h.id DESC LIMIT 50 OFFSET ?2")?;
        let rows = statement
            .query_map(
                params![normalize_for_search(query), offset.clamp(0, 10000)],
                |r| {
                    Ok(LookupHistory {
                        id: r.get(0)?,
                        surface: r.get(1)?,
                        headword: r.get(2)?,
                        reading: r.get(3)?,
                        book_id: r.get(4)?,
                        book_title: r.get(5)?,
                        location_cfi: r.get(6)?,
                        sentence: r.get(7)?,
                        looked_up_at: r.get(8)?,
                        count: r.get(9)?,
                        is_available: r.get(10)?,
                    })
                },
            )?
            .collect();
        rows
    }

    pub fn clear_lookup_history(&self) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .expect("database mutex poisoned")
            .execute("DELETE FROM lookup_history", [])?;
        Ok(())
    }

    pub fn lookup_history_location(&self, id: i64) -> Result<String, String> {
        let (path,cfi,size,modified): (String,String,i64,i64) = self.connection.lock().expect("database mutex poisoned").query_row("SELECT b.file_path,h.location_cfi,h.source_size,h.source_modified FROM lookup_history h JOIN books b ON b.id=h.book_id WHERE h.id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).map_err(|e| e.to_string())?;
        if cfi.is_empty() {
            return Err("No source anchor was captured.".into());
        }
        let metadata = fs::metadata(path).map_err(|_| {
            "The source EPUB is unavailable. Your lookup history is retained.".to_string()
        })?;
        let actual_modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|t| t.as_secs() as i64);
        if metadata.len() as i64 != size || actual_modified != Some(modified) {
            return Err("The source EPUB has changed; the old anchor may be invalid. Use the excerpt as a reference.".into());
        }
        Ok(cfi)
    }
}
