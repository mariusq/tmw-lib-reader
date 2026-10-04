use super::*;
use crate::models::book::{SavePassageRequest, SavedPassage};

impl Database {
    pub fn save_passage(&self, request: &SavePassageRequest) -> rusqlite::Result<i64> {
        if request.surface.trim().is_empty()
            || request.surface.chars().count() > 256
            || request.sentence.chars().count() > 4000
            || request.note.chars().count() > 2000
            || request.location_cfi.len() > 4096
            || (!request.location_cfi.is_empty() && !request.location_cfi.starts_with("epubcfi("))
        {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let connection = self.connection.lock().expect("database mutex poisoned");
        // A duplicate save reuses the bookmark without discarding an edited note/context.
        connection.query_row(
            "INSERT INTO saved_passages(book_id,surface,headword,reading,sentence,note,location_cfi,source_size,source_modified,created_at)
             SELECT id,?2,?3,?4,?5,?6,?7,file_size,modified_time,?8 FROM books WHERE id=?1
             ON CONFLICT(book_id,location_cfi,surface) DO UPDATE SET surface=excluded.surface RETURNING id",
            params![request.book_id, request.surface, request.headword, request.reading, request.sentence, request.note, request.location_cfi, unix_timestamp()],
            |row| row.get(0),
        )
    }

    pub fn saved_passages(
        &self,
        book_id: Option<i64>,
        offset: i64,
    ) -> rusqlite::Result<Vec<SavedPassage>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached(
            "SELECT p.id,p.book_id,COALESCE(o.title,b.discovered_title,b.file_name),p.surface,p.headword,p.reading,p.sentence,p.note,p.location_cfi,
                    p.source_size,p.source_modified,b.extraction_status <> 'unavailable' FROM saved_passages p
             JOIN books b ON b.id=p.book_id LEFT JOIN book_overrides o ON o.book_id=b.id
             WHERE (?1 IS NULL OR p.book_id=?1) ORDER BY p.id DESC LIMIT 50 OFFSET ?2")?;
        let rows = statement
            .query_map(params![book_id, offset.max(0)], |row| {
                Ok(SavedPassage {
                    id: row.get(0)?,
                    book_id: row.get(1)?,
                    book_title: row.get(2)?,
                    surface: row.get(3)?,
                    headword: row.get(4)?,
                    reading: row.get(5)?,
                    sentence: row.get(6)?,
                    note: row.get(7)?,
                    location_cfi: row.get(8)?,
                    source_size: row.get(9)?,
                    source_modified: row.get(10)?,
                    is_available: row.get(11)?,
                })
            })?
            .collect();
        rows
    }

    pub fn edit_passage(&self, id: i64, sentence: &str, note: &str) -> rusqlite::Result<()> {
        if sentence.chars().count() > 4000 || note.chars().count() > 2000 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        self.connection
            .lock()
            .expect("database mutex poisoned")
            .execute(
                "UPDATE saved_passages SET sentence=?2,note=?3 WHERE id=?1",
                params![id, sentence, note],
            )?;
        Ok(())
    }

    pub fn delete_passage(&self, id: i64) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .expect("database mutex poisoned")
            .execute("DELETE FROM saved_passages WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn passage_location(&self, id: i64) -> Result<String, String> {
        let (path,cfi,size,modified): (String,String,i64,i64) = self.connection.lock().expect("database mutex poisoned")
            .query_row("SELECT b.file_path,p.location_cfi,p.source_size,p.source_modified FROM saved_passages p JOIN books b ON b.id=p.book_id WHERE p.id=?1", [id],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).map_err(|e| e.to_string())?;
        if cfi.is_empty() {
            return Err(
                "No source anchor was captured. Use the saved excerpt as a reference.".into(),
            );
        }
        let metadata = fs::metadata(path).map_err(|_| {
            "The source EPUB is unavailable. Your saved excerpt is still readable.".to_string()
        })?;
        let actual_modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|time| time.as_secs() as i64);
        if metadata.len() as i64 != size || actual_modified != Some(modified) {
            return Err("The source EPUB has changed. The old anchor may be invalid; use the saved excerpt to find the passage.".into());
        }
        Ok(cfi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bookmarks_survive_duplicate_saves_rescans_restarts_and_backup() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("catalog.sqlite3");
        let database = Database::open(&path).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: "missing",
                display_name: "Books",
            })
            .unwrap();
        let source = NewBook {
            library_root_id: root.id,
            file_path: "missing/book.epub",
            parent_folder_path: "missing",
            file_name: "本.epub",
            file_size: 4,
            modified_time: 1,
        };
        let book = database.add_book(source.clone()).unwrap();
        let mut request = SavePassageRequest {
            book_id: book.id,
            surface: "読んだ".into(),
            headword: Some("読む".into()),
            reading: Some("よむ".into()),
            sentence: "本を読んだ。".into(),
            note: "覚えておきたい".into(),
            location_cfi: "epubcfi(/6/2!/4/2/1:0)".into(),
        };
        let id = database.save_passage(&request).unwrap();
        database
            .edit_passage(id, "編集した文章。", "私のメモ")
            .unwrap();
        request.note.clear();
        assert_eq!(database.save_passage(&request).unwrap(), id);
        database.upsert_scanned_book(source).unwrap();
        database
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE books SET extraction_status='unavailable'", [])
            .unwrap();
        let rows = database.saved_passages(None, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sentence, "編集した文章。");
        assert!(!rows[0].is_available);
        assert!(database
            .passage_location(id)
            .unwrap_err()
            .contains("unavailable"));
        let backup = directory.path().join("backup.sqlite3");
        database.backup_to(&backup).unwrap();
        database.delete_passage(id).unwrap();
        database.restore_from(&backup).unwrap();
        drop(database);
        let database = Database::open(&path).unwrap();
        let rows = database.saved_passages(Some(book.id), 0).unwrap();
        assert_eq!(rows[0].note, "私のメモ");
        assert_eq!(rows[0].headword.as_deref(), Some("読む"));
        assert_eq!(rows[0].location_cfi, request.location_cfi);
        let recovered = NewBook {
            library_root_id: root.id,
            file_path: "missing/book.epub",
            parent_folder_path: "missing",
            file_name: "本.epub",
            file_size: 4,
            modified_time: 1,
        };
        assert!(database.upsert_scanned_book(recovered).unwrap().changed);
        assert!(database.saved_passages(Some(book.id), 0).unwrap()[0].is_available);
        database.delete_passage(id).unwrap();
        assert!(database.saved_passages(None, 0).unwrap().is_empty());
        assert!(database.book(book.id).unwrap().is_some());
    }

    #[test]
    fn anchor_checks_source_changes_and_never_writes_source() {
        let directory = tempdir().unwrap();
        let source_path = directory.path().join("fixture.epub");
        fs::write(&source_path, b"read only fixture").unwrap();
        let metadata = fs::metadata(&source_path).unwrap();
        let modified = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: directory.path().to_str().unwrap(),
                display_name: "Test",
            })
            .unwrap();
        let book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: source_path.to_str().unwrap(),
                parent_folder_path: directory.path().to_str().unwrap(),
                file_name: "fixture.epub",
                file_size: metadata.len() as i64,
                modified_time: modified,
            })
            .unwrap();
        let mut request = SavePassageRequest {
            book_id: book.id,
            surface: "本".into(),
            headword: None,
            reading: None,
            sentence: "".into(),
            note: "".into(),
            location_cfi: "epubcfi(/6/2!/4/2/1:0)".into(),
        };
        let id = database.save_passage(&request).unwrap();
        assert_eq!(database.passage_location(id).unwrap(), request.location_cfi);
        database.edit_passage(id, "本。", "note").unwrap();
        assert_eq!(fs::read(&source_path).unwrap(), b"read only fixture");
        // Simulate changed extraction inputs without modifying the source fixture.
        database
            .connection
            .lock()
            .unwrap()
            .execute("UPDATE saved_passages SET source_size=0 WHERE id=?1", [id])
            .unwrap();
        assert!(database
            .passage_location(id)
            .unwrap_err()
            .contains("changed"));
        request.location_cfi.clear();
        let excerpt_id = database.save_passage(&request).unwrap();
        assert!(database
            .passage_location(excerpt_id)
            .unwrap_err()
            .contains("No source anchor"));
        request.surface.clear();
        assert!(database.save_passage(&request).is_err());
        request.surface = "本".into();
        request.book_id = -1;
        assert!(database.save_passage(&request).is_err());
        for index in 0..55 {
            request.book_id = book.id;
            request.surface = format!("本{index}");
            database.save_passage(&request).unwrap();
        }
        assert_eq!(database.saved_passages(None, 0).unwrap().len(), 50);
        assert_eq!(database.saved_passages(Some(book.id), 50).unwrap().len(), 7);
    }

    #[test]
    fn restoring_previous_schema_adds_passages_without_changing_progress() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: "source",
                display_name: "Books",
            })
            .unwrap();
        let book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: "source/book.epub",
                parent_folder_path: "source",
                file_name: "本.epub",
                file_size: 4,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_reading_location(book.id, "epubcfi(/6/2!/4/2/1:0)")
            .unwrap();
        database.record_reader_open(book.id).unwrap();
        database.set_reader_finished(book.id, true).unwrap();
        database
            .save_book_overrides(
                book.id,
                &BookOverride {
                    title: Some("修正した題名".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        database
            .connection
            .lock()
            .unwrap()
            .execute_batch("DROP TABLE saved_passages; DROP TABLE lookup_history; DROP TABLE smart_shelves; DROP INDEX idx_books_reading_status; ALTER TABLE books DROP COLUMN reading_status; ALTER TABLE books DROP COLUMN completed_at; PRAGMA user_version=11;")
            .unwrap();
        let backup = directory.path().join("previous.sqlite3");
        database.backup_to(&backup).unwrap();
        database.restore_from(&backup).unwrap();
        assert!(database.saved_passages(None, 0).unwrap().is_empty());
        assert_eq!(
            database.reading_location(book.id).unwrap().as_deref(),
            Some("epubcfi(/6/2!/4/2/1:0)")
        );
        assert!(database.record_reader_open(book.id).unwrap());
        assert_eq!(
            database
                .book_details(book.id)
                .unwrap()
                .unwrap()
                .effective_title,
            "修正した題名"
        );
        let version: i64 = database
            .connection
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 14);
    }
}
