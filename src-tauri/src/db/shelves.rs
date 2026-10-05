use super::{unix_timestamp, Database};
use crate::models::book::BrowseBooksRequest;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SmartShelf {
    pub id: Option<i64>,
    pub name: String,
    pub version: i64,
    pub filter: BrowseBooksRequest,
    pub sort_order: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{book::NewBook, library_root::NewLibraryRoot};

    fn filter() -> BrowseBooksRequest {
        BrowseBooksRequest {
            reading_status: Some("want".into()),
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: false, duplicate_filtering: None,
            query: String::new(),
            sort: "title".into(),
            offset: 0,
            limit: 80,
        }
    }

    #[test]
    fn status_transitions_shelves_rescan_and_backup_preserve_user_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let db = Database::open(&path).unwrap();
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "absent",
                display_name: "Books",
            })
            .unwrap();
        let source = NewBook {
            library_root_id: root.id,
            file_path: "absent/book.epub",
            parent_folder_path: "absent",
            file_name: "物語.epub",
            file_size: 1,
            modified_time: 1,
        };
        let id = db.add_book(source.clone()).unwrap().id;
        db.save_reading_location(id, "epubcfi(/6/2)").unwrap();
        db.set_reading_status(id, "want").unwrap();
        let mut shelf = SmartShelf {
            id: None,
            name: "読みたい".into(),
            version: 1,
            filter: filter(),
            sort_order: 2,
        };
        shelf.id = Some(db.save_smart_shelf(&shelf).unwrap());
        assert_eq!(db.browse_books(&shelf.filter).unwrap().len(), 1);
        assert!(!db.record_reader_open(id).unwrap());
        assert_eq!(db.reading_state(id).unwrap().status, "reading");
        assert!(db.browse_books(&shelf.filter).unwrap().is_empty());
        db.set_reading_status(id, "paused").unwrap();
        db.record_reader_open(id).unwrap();
        assert_eq!(db.reading_state(id).unwrap().status, "paused");
        assert!(db.resume_books(false).unwrap().is_empty());
        db.set_reading_status(id, "finished").unwrap();
        let date = db.reading_state(id).unwrap().completed_at;
        assert!(date.is_some());
        db.set_reading_status(id, "finished").unwrap();
        assert!(db.record_reader_open(id).unwrap());
        db.upsert_scanned_book(source).unwrap();
        assert_eq!(db.reading_state(id).unwrap().completed_at, date);
        let backup = dir.path().join("backup.sqlite3");
        db.backup_to(&backup).unwrap();
        db.set_reading_status(id, "reading").unwrap();
        assert!(db.reading_state(id).unwrap().completed_at.is_none());
        db.delete_smart_shelf(shelf.id.unwrap()).unwrap();
        assert!(db.reading_location(id).unwrap().is_some());
        db.restore_from(&backup).unwrap();
        assert_eq!(db.reading_state(id).unwrap().completed_at, date);
        assert_eq!(db.smart_shelves().unwrap()[0].name, "読みたい");
        shelf.name = "Finished".into();
        shelf.filter.reading_status = Some("finished".into());
        shelf.sort_order = -1;
        db.save_smart_shelf(&shelf).unwrap();
        db.connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE books SET extraction_status='unavailable' WHERE id=?1",
                [id],
            )
            .unwrap();
        assert!(!db.browse_books(&shelf.filter).unwrap()[0].is_available);
        drop(db);
        let db = Database::open(&path).unwrap();
        let stored = db.smart_shelves().unwrap().remove(0);
        assert_eq!(stored.sort_order, -1);
        assert_eq!(db.browse_books(&stored.filter).unwrap().len(), 1);
        assert!(db.set_reading_status(id, "invalid").is_err());
        shelf.version = 99;
        assert!(db.save_smart_shelf(&shelf).is_err());
        shelf.version = 1;
        shelf.filter.sort = "DROP TABLE books".into();
        assert!(db.save_smart_shelf(&shelf).is_err());
        assert!(!dir.path().join("absent").exists());
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingState {
    pub status: String,
    pub completed_at: Option<i64>,
}

fn invalid() -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}
fn valid_status(status: &str) -> bool {
    matches!(status, "unset" | "want" | "reading" | "paused" | "finished")
}
pub(super) fn validate_filter(filter: &BrowseBooksRequest) -> rusqlite::Result<()> {
    if filter
        .reading_status
        .as_deref()
        .is_some_and(|s| !valid_status(s))
        || !matches!(
            filter.sort.as_str(),
            "title" | "author" | "series" | "dateAdded" | "modified" | "folder"
        )
        || filter.query.chars().count() > 2000
        || [filter.library_root_id, filter.tag_id, filter.collection_id]
            .into_iter()
            .flatten()
            .any(|id| id <= 0)
    {
        return Err(invalid());
    }
    Ok(())
}

impl Database {
    pub fn reading_state(&self, id: i64) -> rusqlite::Result<ReadingState> {
        self.connection
            .lock()
            .expect("database mutex poisoned")
            .query_row(
                "SELECT reading_status,completed_at FROM books WHERE id=?1",
                [id],
                |r| {
                    Ok(ReadingState {
                        status: r.get(0)?,
                        completed_at: r.get(1)?,
                    })
                },
            )
    }
    pub fn set_reading_status(&self, id: i64, status: &str) -> rusqlite::Result<()> {
        if !valid_status(status) {
            return Err(invalid());
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        // Repeated Finished preserves its date; leaving Finished clears it.
        if transaction.execute("UPDATE books SET completed_at=CASE WHEN ?2='finished' THEN CASE WHEN reading_status='finished' THEN completed_at ELSE ?3 END ELSE NULL END, reading_status=?2 WHERE id=?1", params![id,status,unix_timestamp()])? == 0 { return Err(rusqlite::Error::QueryReturnedNoRows); }
        transaction.execute(
            "UPDATE reader_resume SET finished=(?2='finished') WHERE book_id=?1",
            params![id, status],
        )?;
        transaction.commit()
    }
    pub fn smart_shelves(&self) -> rusqlite::Result<Vec<SmartShelf>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id,name,version,filter_json,sort_order FROM smart_shelves ORDER BY sort_order,name COLLATE NOCASE,id")?;
        let rows = statement
            .query_map([], |r| {
                let json: String = r.get(3)?;
                let filter = serde_json::from_str(&json).map_err(|_| invalid())?;
                validate_filter(&filter)?;
                Ok(SmartShelf {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    version: r.get(2)?,
                    filter,
                    sort_order: r.get(4)?,
                })
            })?
            .collect();
        rows
    }
    pub fn save_smart_shelf(&self, shelf: &SmartShelf) -> rusqlite::Result<i64> {
        validate_filter(&shelf.filter)?;
        let name = shelf.name.trim();
        if name.is_empty() || name.chars().count() > 120 || shelf.version != 1 {
            return Err(invalid());
        }
        let mut filter = shelf.filter.clone();
        filter.offset = 0;
        filter.limit = 80;
        let json = serde_json::to_string(&filter).map_err(|_| invalid())?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        if let Some(id) = shelf.id {
            if connection.execute(
                "UPDATE smart_shelves SET name=?2,filter_json=?3,sort_order=?4 WHERE id=?1",
                params![id, name, json, shelf.sort_order],
            )? == 0
            {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            Ok(id)
        } else {
            connection.execute(
                "INSERT INTO smart_shelves(name,filter_json,sort_order) VALUES(?1,?2,?3)",
                params![name, json, shelf.sort_order],
            )?;
            Ok(connection.last_insert_rowid())
        }
    }
    pub fn delete_smart_shelf(&self, id: i64) -> rusqlite::Result<()> {
        self.connection
            .lock()
            .expect("database mutex poisoned")
            .execute("DELETE FROM smart_shelves WHERE id=?1", [id])?;
        Ok(())
    }
}
