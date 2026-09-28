use std::{
    fs,
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{
    book::{BatchTagRequest, Book, BookDetails, BookOverride, BrowseBooksRequest, BrowserBook, ExtractedBookMetadata, NewBook},
    library_root::{LibraryRoot, LibraryRootSummary, NewLibraryRoot},
};
use crate::services::metadata::normalize_for_search;
use crate::services::readings::{derive_reading, kana_to_romaji, normalize_romaji};

const INITIAL_SCHEMA: &str = include_str!("migrations/001_initial.sql");
const METADATA_SCHEMA: &str = include_str!("migrations/002_metadata.sql");
const SEARCH_SCHEMA: &str = include_str!("migrations/003_search.sql");
const READING_SEARCH_SCHEMA: &str = include_str!("migrations/004_reading_search.sql");

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        let mut connection = Connection::open(path)?;
        configure_connection(&connection)?;
        run_migrations(&mut connection)?;
        rebuild_search_index(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn add_library_root(&self, root: NewLibraryRoot<'_>) -> rusqlite::Result<LibraryRoot> {
        let now = unix_timestamp();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO library_roots (path, display_name, added_at) VALUES (?1, ?2, ?3)",
            params![root.path, root.display_name, now],
        )?;
        let id = connection.last_insert_rowid();
        get_library_root(&connection, id)
    }

    pub fn library_root(&self, id: i64) -> rusqlite::Result<Option<LibraryRoot>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        get_library_root(&connection, id).optional()
    }

    pub fn library_roots(&self) -> rusqlite::Result<Vec<LibraryRootSummary>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT r.id, r.path, r.display_name, r.added_at, r.last_scanned_at, COUNT(b.id) \
             FROM library_roots r LEFT JOIN books b ON b.library_root_id = r.id \
             GROUP BY r.id ORDER BY r.added_at DESC",
        )?;
        let roots = statement
            .query_map([], |row| {
                Ok(LibraryRootSummary {
                    root: LibraryRoot {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        display_name: row.get(2)?,
                        added_at: row.get(3)?,
                        last_scanned_at: row.get(4)?,
                    },
                    book_count: row.get(5)?,
                })
            })?
            .collect();
        roots
    }

    pub fn remove_library_root(&self, id: i64) -> rusqlite::Result<bool> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        Ok(connection.execute("DELETE FROM library_roots WHERE id = ?1", [id])? > 0)
    }

    pub fn upsert_scanned_book(&self, book: NewBook<'_>) -> rusqlite::Result<bool> {
        let now = unix_timestamp();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let existing: Option<(i64, i64)> = connection
            .query_row(
                "SELECT file_size, modified_time FROM books WHERE file_path = ?1",
                [book.file_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if existing == Some((book.file_size, book.modified_time)) {
            return Ok(false);
        }
        connection.execute(
            "INSERT INTO books (library_root_id, file_path, parent_folder_path, file_name, file_size, modified_time, extraction_status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7) \
             ON CONFLICT(file_path) DO UPDATE SET library_root_id=excluded.library_root_id, parent_folder_path=excluded.parent_folder_path, file_name=excluded.file_name, file_size=excluded.file_size, modified_time=excluded.modified_time, extraction_status='pending', extraction_error=NULL, updated_at=excluded.updated_at",
            params![book.library_root_id, book.file_path, book.parent_folder_path, book.file_name, book.file_size, book.modified_time, now],
        )?;
        let id: i64 = connection.query_row("SELECT id FROM books WHERE file_path=?1", [book.file_path], |row| row.get(0))?;
        refresh_search_document(&connection, id)?;
        Ok(true)
    }

    pub fn mark_root_scanned(&self, id: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "UPDATE library_roots SET last_scanned_at = ?1 WHERE id = ?2",
            params![unix_timestamp(), id],
        )?;
        Ok(())
    }

    pub fn add_book(&self, book: NewBook<'_>) -> rusqlite::Result<Book> {
        let now = unix_timestamp();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO books (library_root_id, file_path, parent_folder_path, file_name, file_size, modified_time, extraction_status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7)",
            params![book.library_root_id, book.file_path, book.parent_folder_path, book.file_name, book.file_size, book.modified_time, now],
        )?;
        refresh_search_document(&connection, connection.last_insert_rowid())?;
        get_book(&connection, connection.last_insert_rowid())
    }

    pub fn book(&self, id: i64) -> rusqlite::Result<Option<Book>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        get_book(&connection, id).optional()
    }

    pub fn book_by_path(&self, path: &str) -> rusqlite::Result<Option<Book>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        get_book_by_path(&connection, path).optional()
    }

    pub fn book_details(&self, id: i64) -> rusqlite::Result<Option<BookDetails>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let book = match get_book(&connection, id).optional()? { Some(book) => book, None => return Ok(None) };
        let override_values = connection.query_row(
            "SELECT title, creator, series_name, volume_label, cover_path, notes FROM book_overrides WHERE book_id=?1",
            [id], |row| Ok(BookOverride { title: row.get(0)?, creator: row.get(1)?, series_name: row.get(2)?, volume_label: row.get(3)?, cover_path: row.get(4)?, notes: row.get(5)? }),
        ).optional()?.unwrap_or_default();
        let tags = connection.prepare("SELECT t.id, t.name FROM tags t JOIN book_tags bt ON bt.tag_id=t.id WHERE bt.book_id=?1 ORDER BY t.name COLLATE NOCASE")?
            .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let fallback = filename_fallback(&book.file_name);
        Ok(Some(BookDetails {
            effective_title: nonempty(&override_values.title).or(nonempty(&book.discovered_title)).unwrap_or(fallback),
            effective_creator: nonempty(&override_values.creator).or(nonempty(&book.discovered_creator)).unwrap_or_default(),
            effective_series: nonempty(&override_values.series_name).or(nonempty(&book.discovered_series)).unwrap_or_default(),
            effective_volume: nonempty(&override_values.volume_label).or(nonempty(&book.discovered_series_index)).unwrap_or_default(),
            effective_cover_path: nonempty(&override_values.cover_path).or(book.discovered_cover_path.clone()),
            book, override_values, tags,
        }))
    }

    pub fn save_book_overrides(&self, id: i64, values: &BookOverride) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("INSERT INTO book_overrides(book_id,title,creator,series_name,volume_label,cover_path,notes,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(book_id) DO UPDATE SET title=excluded.title,creator=excluded.creator,series_name=excluded.series_name,volume_label=excluded.volume_label,cover_path=excluded.cover_path,notes=excluded.notes,updated_at=excluded.updated_at", params![id, values.title, values.creator, values.series_name, values.volume_label, values.cover_path, values.notes, unix_timestamp()])?;
        refresh_search_document(&connection, id)
    }

    pub fn reset_book_override(&self, id: i64, field: &str) -> rusqlite::Result<()> {
        let column = match field { "title" | "creator" | "series_name" | "volume_label" | "cover_path" | "notes" => field, _ => return Err(rusqlite::Error::InvalidParameterName("Unknown override field".into())) };
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(&format!("UPDATE book_overrides SET {column}=NULL, updated_at=?1 WHERE book_id=?2"), params![unix_timestamp(), id])?;
        refresh_search_document(&connection, id)
    }

    pub fn reset_all_book_overrides(&self, id: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("DELETE FROM book_overrides WHERE book_id=?1", [id])?;
        refresh_search_document(&connection, id)
    }

    pub fn replace_book_tags(&self, id: i64, names: &[String]) -> rusqlite::Result<()> {
        self.apply_tags(&[id], names)
    }

    pub fn batch_replace_tags(&self, request: &BatchTagRequest) -> rusqlite::Result<()> {
        self.apply_tags(&request.book_ids, &request.tag_names)
    }

    fn apply_tags(&self, book_ids: &[i64], names: &[String]) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        let clean: Vec<String> = names.iter().map(|name| name.trim()).filter(|name| !name.is_empty()).map(ToOwned::to_owned).collect();
        for name in &clean { transaction.execute("INSERT INTO tags(name) VALUES(?1) ON CONFLICT(name) DO NOTHING", [name])?; }
        for id in book_ids {
            transaction.execute("DELETE FROM book_tags WHERE book_id=?1", [id])?;
            for name in &clean { transaction.execute("INSERT INTO book_tags(book_id,tag_id) SELECT ?1,id FROM tags WHERE name=?2", params![id, name])?; }
        }
        transaction.commit()?;
        for id in book_ids { refresh_search_document(&connection, *id)?; }
        Ok(())
    }

    pub fn save_extracted_metadata(
        &self,
        book_id: i64,
        metadata: &ExtractedBookMetadata,
    ) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let status = if metadata.extraction_error.is_some() {
            "error"
        } else {
            "complete"
        };
        connection.execute(
            "UPDATE books SET discovered_title=?1, discovered_creator=?2, discovered_language=?3, \
             discovered_identifier=?4, discovered_series=?5, discovered_series_index=?6, discovered_cover_path=?7, \
             extraction_status=?8, extraction_error=?9, updated_at=?10 WHERE id=?11",
            params![metadata.title, metadata.creator, metadata.language, metadata.identifier,
                metadata.series, metadata.series_index, metadata.cover_path, status,
                metadata.extraction_error, unix_timestamp(), book_id],
        )?;
        refresh_search_document(&connection, book_id)?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT value FROM app_settings WHERE key=?1",
                [key],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("INSERT INTO app_settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        Ok(())
    }

    pub fn library_root_paths(&self) -> rusqlite::Result<Vec<String>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let paths = connection
            .prepare("SELECT path FROM library_roots")?
            .query_map([], |row| row.get(0))?
            .collect();
        paths
    }

    pub fn browse_books(&self, request: &BrowseBooksRequest) -> rusqlite::Result<Vec<BrowserBook>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let query = normalize_for_search(&request.query);
        let romaji_query = normalize_romaji(&request.query);
        let sort = match request.sort.as_str() {
            "author" => "effective_creator COLLATE NOCASE, effective_title COLLATE NOCASE",
            "series" => "effective_series COLLATE NOCASE, effective_volume COLLATE NOCASE, effective_title COLLATE NOCASE",
            "dateAdded" => "b.created_at DESC, b.id DESC",
            "modified" => "b.modified_time DESC, b.id DESC",
            "folder" => "b.parent_folder_path COLLATE NOCASE, b.file_name COLLATE NOCASE",
            _ => "effective_title COLLATE NOCASE, b.file_name COLLATE NOCASE",
        };
        let sql = format!(
            "SELECT b.id, b.library_root_id, b.file_name, b.parent_folder_path, \
             COALESCE(NULLIF(o.title, ''), NULLIF(b.discovered_title, ''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name, 1, length(b.file_name)-5) ELSE b.file_name END) AS effective_title, \
             COALESCE(NULLIF(o.creator, ''), NULLIF(b.discovered_creator, ''), '') AS effective_creator, \
             COALESCE(NULLIF(o.series_name, ''), NULLIF(b.discovered_series, ''), '') AS effective_series, \
             COALESCE(NULLIF(o.volume_label, ''), NULLIF(b.discovered_series_index, ''), '') AS effective_volume, \
             COALESCE(NULLIF(o.cover_path, ''), b.discovered_cover_path), b.created_at, b.modified_time, \
             (b.discovered_title IS NULL OR trim(b.discovered_title) = '' OR b.extraction_status = 'error') AS needs_metadata \
             FROM books b JOIN library_roots r ON r.id=b.library_root_id \
             LEFT JOIN book_overrides o ON o.book_id=b.id \
             LEFT JOIN book_search_documents d ON d.book_id=b.id \
             WHERE (?1 IS NULL OR b.library_root_id=?1) \
             AND (?2 IS NULL OR EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id=b.id AND bt.tag_id=?2)) \
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM collection_books cb WHERE cb.book_id=b.id AND cb.collection_id=?3)) \
             AND (?4=0 OR b.discovered_title IS NULL OR trim(b.discovered_title)='' OR b.extraction_status='error') \
             AND (?5='' OR instr(d.title_normalized, ?5)>0 OR instr(d.creator_normalized, ?5)>0 OR instr(d.series_normalized, ?5)>0 OR instr(d.file_name_normalized, ?5)>0 OR instr(d.parent_folder_normalized, ?5)>0 OR instr(d.tags_normalized, ?5)>0 OR instr(d.title_reading, ?5)>0 OR instr(d.creator_reading, ?5)>0 OR instr(d.series_reading, ?5)>0 OR instr(d.file_name_reading, ?5)>0 OR instr(d.title_romaji, ?6)>0 OR instr(d.creator_romaji, ?6)>0 OR instr(d.series_romaji, ?6)>0 OR instr(d.file_name_romaji, ?6)>0) \
             ORDER BY CASE WHEN ?5='' THEN 0 WHEN (NULLIF(o.title,'') IS NOT NULL OR NULLIF(b.discovered_title,'') IS NOT NULL) AND d.title_normalized=?5 THEN 0 WHEN (NULLIF(o.title,'') IS NOT NULL OR NULLIF(b.discovered_title,'') IS NOT NULL) AND d.title_normalized LIKE ?5 || '%' THEN 1 WHEN d.title_reading=?5 OR d.title_romaji=?6 THEN 2 WHEN d.title_reading LIKE ?5 || '%' OR d.title_romaji LIKE ?6 || '%' THEN 3 WHEN d.title_romaji LIKE '%' || ?6 || '%' THEN 4 ELSE 5 END, {sort} LIMIT ?7 OFFSET ?8"
        );
        let mut statement = connection.prepare(&sql)?;
        let books = statement.query_map(params![request.library_root_id, request.tag_id, request.collection_id, request.needs_metadata as i64, query, romaji_query, request.limit.clamp(1, 200), request.offset.max(0)], |row| {
            Ok(BrowserBook { id: row.get(0)?, library_root_id: row.get(1)?, file_name: row.get(2)?, parent_folder_path: row.get(3)?, effective_title: row.get(4)?, effective_creator: row.get(5)?, effective_series: row.get(6)?, effective_volume: row.get(7)?, effective_cover_path: row.get(8)?, created_at: row.get(9)?, modified_time: row.get(10)?, needs_metadata: row.get(11)? })
        })?.collect();
        books
    }

    pub fn tags(&self) -> rusqlite::Result<Vec<(i64, String)>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id, name FROM tags ORDER BY name COLLATE NOCASE")?;
        let tags = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
        tags
    }

    pub fn collections(&self) -> rusqlite::Result<Vec<(i64, String)>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id, name FROM collections ORDER BY name COLLATE NOCASE")?;
        let collections = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect();
        collections
    }

    pub fn books_for_cover_regeneration(&self) -> rusqlite::Result<Vec<Book>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id FROM books ORDER BY id")?;
        let ids: Vec<i64> = statement.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
        ids.into_iter().map(|id| get_book(&connection, id)).collect()
    }
}

fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(())
}

fn run_migrations(connection: &mut Connection) -> rusqlite::Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version < 1 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(INITIAL_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 1)?;
        transaction.commit()?;
    }
    if version < 2 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(METADATA_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 2)?;
        transaction.commit()?;
    }
    if version < 3 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(SEARCH_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 3)?;
        transaction.commit()?;
    }
    if version < 4 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(READING_SEARCH_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 4)?;
        transaction.commit()?;
    }
    Ok(())
}

fn rebuild_search_index(connection: &Connection) -> rusqlite::Result<()> {
    let ids: Vec<i64> = connection.prepare("SELECT id FROM books")?.query_map([], |row| row.get(0))?.collect::<rusqlite::Result<_>>()?;
    connection.execute("DELETE FROM book_search_documents", [])?;
    connection.execute("DELETE FROM book_search_fts", [])?;
    for id in ids { refresh_search_document(connection, id)?; }
    Ok(())
}

fn refresh_search_document(connection: &Connection, book_id: i64) -> rusqlite::Result<()> {
    let row: (String, String, String, String, String, String) = connection.query_row(
        "SELECT COALESCE(NULLIF(o.title,''), NULLIF(b.discovered_title,''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name,1,length(b.file_name)-5) ELSE b.file_name END), COALESCE(NULLIF(o.creator,''), NULLIF(b.discovered_creator,''), ''), COALESCE(NULLIF(o.series_name,''), NULLIF(b.discovered_series,''), ''), b.file_name, b.parent_folder_path, COALESCE((SELECT group_concat(t.name, ' ') FROM book_tags bt JOIN tags t ON t.id=bt.tag_id WHERE bt.book_id=b.id), '') FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id WHERE b.id=?1",
        [book_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?;
    let n = [normalize_for_search(&row.0), normalize_for_search(&row.1), normalize_for_search(&row.2), normalize_for_search(&row.3), normalize_for_search(&row.4), normalize_for_search(&row.5)];
    let generated = |value: &str| derive_reading(value).unwrap_or_default();
    let title_reading = generated(&row.0);
    let creator_reading = generated(&row.1); let series_reading = generated(&row.2); let file_name_reading = generated(&row.3);
    let title_romaji=kana_to_romaji(&title_reading); let creator_romaji=kana_to_romaji(&creator_reading); let series_romaji=kana_to_romaji(&series_reading); let file_name_romaji=kana_to_romaji(&file_name_reading);
    connection.execute("INSERT INTO book_search_documents(book_id,title_normalized,creator_normalized,series_normalized,file_name_normalized,parent_folder_normalized,tags_normalized,title_reading,creator_reading,series_reading,file_name_reading,aliases_normalized,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'',?12,?13,?14,?15,'') ON CONFLICT(book_id) DO UPDATE SET title_normalized=excluded.title_normalized,creator_normalized=excluded.creator_normalized,series_normalized=excluded.series_normalized,file_name_normalized=excluded.file_name_normalized, parent_folder_normalized=excluded.parent_folder_normalized,tags_normalized=excluded.tags_normalized,title_reading=excluded.title_reading,creator_reading=excluded.creator_reading,series_reading=excluded.series_reading,file_name_reading=excluded.file_name_reading,aliases_normalized='',title_romaji=excluded.title_romaji,creator_romaji=excluded.creator_romaji,series_romaji=excluded.series_romaji,file_name_romaji=excluded.file_name_romaji,aliases_romaji=''", params![book_id,n[0],n[1],n[2],n[3],n[4],n[5],title_reading,creator_reading,series_reading,file_name_reading,title_romaji,creator_romaji,series_romaji,file_name_romaji])?;
    connection.execute("DELETE FROM book_search_fts WHERE book_id=?1", [book_id])?;
    connection.execute("INSERT INTO book_search_fts(book_id,title,creator,series,file_name,parent_folder,tags,title_reading,creator_reading,series_reading,file_name_reading,aliases,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'',?12,?13,?14,?15,'')", params![book_id,n[0],n[1],n[2],n[3],n[4],n[5],title_reading,creator_reading,series_reading,file_name_reading,title_romaji,creator_romaji,series_romaji,file_name_romaji])?;
    Ok(())
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(ToOwned::to_owned)
}

fn filename_fallback(file_name: &str) -> String {
    file_name.strip_suffix(".epub").or_else(|| file_name.strip_suffix(".EPUB")).unwrap_or(file_name).to_owned()
}


fn get_book_by_path(connection: &Connection, path: &str) -> rusqlite::Result<Book> {
    connection.query_row("SELECT id, library_root_id, file_path, parent_folder_path, file_name, file_size, modified_time, content_hash, discovered_title, discovered_creator, discovered_language, discovered_identifier, discovered_series, discovered_series_index, discovered_cover_path, extraction_status, extraction_error, created_at, updated_at FROM books WHERE file_path = ?1", [path], |row| Ok(Book {
        id: row.get(0)?, library_root_id: row.get(1)?, file_path: row.get(2)?, parent_folder_path: row.get(3)?, file_name: row.get(4)?, file_size: row.get(5)?, modified_time: row.get(6)?, content_hash: row.get(7)?, discovered_title: row.get(8)?, discovered_creator: row.get(9)?, discovered_language: row.get(10)?, discovered_identifier: row.get(11)?, discovered_series: row.get(12)?, discovered_series_index: row.get(13)?, discovered_cover_path: row.get(14)?, extraction_status: row.get(15)?, extraction_error: row.get(16)?, created_at: row.get(17)?, updated_at: row.get(18)?,
    }))
}

fn get_library_root(connection: &Connection, id: i64) -> rusqlite::Result<LibraryRoot> {
    connection.query_row(
        "SELECT id, path, display_name, added_at, last_scanned_at FROM library_roots WHERE id = ?1",
        [id],
        |row| {
            Ok(LibraryRoot {
                id: row.get(0)?,
                path: row.get(1)?,
                display_name: row.get(2)?,
                added_at: row.get(3)?,
                last_scanned_at: row.get(4)?,
            })
        },
    )
}

fn get_book(connection: &Connection, id: i64) -> rusqlite::Result<Book> {
    connection.query_row("SELECT id, library_root_id, file_path, parent_folder_path, file_name, file_size, modified_time, content_hash, discovered_title, discovered_creator, discovered_language, discovered_identifier, discovered_series, discovered_series_index, discovered_cover_path, extraction_status, extraction_error, created_at, updated_at FROM books WHERE id = ?1", [id], |row| Ok(Book {
        id: row.get(0)?, library_root_id: row.get(1)?, file_path: row.get(2)?, parent_folder_path: row.get(3)?, file_name: row.get(4)?, file_size: row.get(5)?, modified_time: row.get(6)?, content_hash: row.get(7)?, discovered_title: row.get(8)?, discovered_creator: row.get(9)?, discovered_language: row.get(10)?, discovered_identifier: row.get(11)?, discovered_series: row.get(12)?, discovered_series_index: row.get(13)?, discovered_cover_path: row.get(14)?, extraction_status: row.get(15)?, extraction_error: row.get(16)?, created_at: row.get(17)?, updated_at: row.get(18)?,
    }))
}

fn unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrations_create_schema_and_enable_required_pragmas() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let connection = database.connection.lock().unwrap();
        let version: i64 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let foreign_keys: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
        assert_eq!(foreign_keys, 1);
        assert!(connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'books'",
                [],
                |_| Ok(())
            )
            .is_ok());
    }

    #[test]
    fn inserts_and_retrieves_library_roots_and_books() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\日本語.epub",
                parent_folder_path: r"F:\\books",
                file_name: "日本語.epub",
                file_size: 42,
                modified_time: 123,
            })
            .unwrap();
        assert_eq!(database.library_root(root.id).unwrap(), Some(root));
        assert_eq!(database.book(book.id).unwrap(), Some(book));
    }

    #[test]
    fn rescans_update_changed_books_without_duplicates_and_root_removal_is_catalog_only() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let book = NewBook {
            library_root_id: root.id,
            file_path: r"F:\\books\\nested\\本.EPUB",
            parent_folder_path: r"F:\\books\\nested",
            file_name: "本.EPUB",
            file_size: 42,
            modified_time: 123,
        };
        assert!(database.upsert_scanned_book(book.clone()).unwrap());
        assert!(!database.upsert_scanned_book(book.clone()).unwrap());
        assert!(database
            .upsert_scanned_book(NewBook {
                file_size: 43,
                modified_time: 124,
                ..book
            })
            .unwrap());
        assert_eq!(database.library_roots().unwrap()[0].book_count, 1);
        assert!(database.remove_library_root(root.id).unwrap());
        assert!(database.book(1).unwrap().is_none());
    }

    #[test]
    fn search_matches_normalized_japanese_substrings_and_ranks_titles() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database.add_library_root(NewLibraryRoot { path: r"F:\\books", display_name: "Books" }).unwrap();
        let title_book = database.add_book(NewBook { library_root_id: root.id, file_path: r"F:\\books\\a.epub", parent_folder_path: r"F:\\books", file_name: "a.epub", file_size: 1, modified_time: 1 }).unwrap();
        database.save_extracted_metadata(title_book.id, &ExtractedBookMetadata { title: Some("巨人１巻".into()), creator: Some("著者".into()), ..Default::default() }).unwrap();
        let file_book = database.add_book(NewBook { library_root_id: root.id, file_path: r"F:\\books\\巨人のメモ.epub", parent_folder_path: r"F:\\books", file_name: "巨人のメモ.epub", file_size: 1, modified_time: 1 }).unwrap();
        let request = |query: &str| BrowseBooksRequest { library_root_id: None, tag_id: None, collection_id: None, needs_metadata: false, query: query.into(), sort: "title".into(), offset: 0, limit: 20 };
        let results = database.browse_books(&request("巨人1巻")).unwrap();
        assert_eq!(results[0].id, title_book.id);
        assert_eq!(results[0].effective_title, "巨人１巻");
        assert!(database.browse_books(&request("巨人")).unwrap().iter().any(|book| book.id == file_book.id));
    }

    #[test]
    fn romaji_readings_rank_before_filenames() {
        let directory = tempdir().unwrap(); let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database.add_library_root(NewLibraryRoot { path: r"F:\\books", display_name: "Books" }).unwrap();
        let title = database.add_book(NewBook { library_root_id: root.id, file_path: r"F:\\books\\a.epub", parent_folder_path: r"F:\\books", file_name: "a.epub", file_size: 1, modified_time: 1 }).unwrap();
        database.save_extracted_metadata(title.id, &ExtractedBookMetadata { title: Some("進撃の巨人".into()), ..Default::default() }).unwrap();
        let other = database.add_book(NewBook { library_root_id: root.id, file_path: r"F:\\books\\shingeki no kyojin.epub", parent_folder_path: r"F:\\books", file_name: "shingeki no kyojin.epub", file_size: 1, modified_time: 1 }).unwrap();
        let request = |query: &str| BrowseBooksRequest { library_root_id: None, tag_id: None, collection_id: None, needs_metadata: false, query: query.into(), sort: "title".into(), offset: 0, limit: 20 };
        assert_eq!(database.browse_books(&request("shingeki no kyojin")).unwrap()[0].id, title.id);
        assert_ne!(other.id, title.id);
    }

    #[test]
    fn overrides_and_tags_persist_and_refresh_effective_metadata() {
        let directory = tempdir().unwrap(); let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database.add_library_root(NewLibraryRoot { path: r"F:\\books", display_name: "Books" }).unwrap();
        let book = database.add_book(NewBook { library_root_id: root.id, file_path: r"F:\\books\\old.epub", parent_folder_path: r"F:\\books", file_name: "old.epub", file_size: 1, modified_time: 1 }).unwrap();
        database.save_extracted_metadata(book.id, &ExtractedBookMetadata { title: Some("Discovered".into()), ..Default::default() }).unwrap();
        database.save_book_overrides(book.id, &BookOverride { title: Some("Corrected".into()), notes: Some("Personal note".into()), ..Default::default() }).unwrap();
        database.replace_book_tags(book.id, &["読書中".into(), "漫画".into()]).unwrap();
        let details = database.book_details(book.id).unwrap().unwrap();
        assert_eq!(details.effective_title, "Corrected");
        assert_eq!(details.tags.len(), 2);
        database.reset_book_override(book.id, "title").unwrap();
        assert_eq!(database.book_details(book.id).unwrap().unwrap().effective_title, "Discovered");
    }
}
