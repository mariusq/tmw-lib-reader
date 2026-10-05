pub(crate) mod companion;
pub(crate) mod user_sync;
pub mod history;
mod passages;
mod duplicates;
pub mod shelves;

use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    params, params_from_iter, types::Value, Connection, DatabaseName, OptionalExtension,
};
use unicode_normalization::UnicodeNormalization;

use crate::models::{
    book::{
        AssignSeriesRequest, BatchTagRequest, Book, BookDetails, BookOverride, BrowseBooksRequest,
        BrowserBook, CreateCollectionRequest, DictionaryEntry, DictionarySummary,
        ExtractedBookMetadata, FolderGroup, FolderGroupBook, NewBook, ReaderBook, ResumeBook,
        ScanUpsert,
    },
    library_root::{LibraryRoot, LibraryRootSummary, NewLibraryRoot},
};
use crate::services::metadata::normalize_for_search;
use crate::services::readings::{derive_reading, kana_to_romaji, normalize_romaji};
use crate::services::{
    performance::{self, Stage},
    volumes::suggested_volume,
};

const INITIAL_SCHEMA: &str = include_str!("migrations/001_initial.sql");
const METADATA_SCHEMA: &str = include_str!("migrations/002_metadata.sql");
const SEARCH_SCHEMA: &str = include_str!("migrations/003_search.sql");
const READING_SEARCH_SCHEMA: &str = include_str!("migrations/004_reading_search.sql");
const READER_PROGRESS_SCHEMA: &str = include_str!("migrations/005_reader_progress.sql");
const DICTIONARY_SCHEMA: &str = include_str!("migrations/006_dictionary.sql");
const DICTIONARY_LOOKUP_SCHEMA: &str = include_str!("migrations/007_dictionary_lookup.sql");
const SEARCH_INDEX_STATE_SCHEMA: &str = include_str!("migrations/008_search_index_state.sql");
const LARGE_LIBRARY_QUERY_SCHEMA: &str = include_str!("migrations/009_large_library_queries.sql");
const DUPLICATE_TITLE_LOOKUP_SCHEMA: &str =
    include_str!("migrations/010_duplicate_title_lookup.sql");
const SEARCH_INDEX_VERSION: i64 = 1;

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let startup_started = std::time::Instant::now();
        performance::start();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        let mut connection = Connection::open(path)?;
        configure_connection(&connection)?;
        duplicates::initialize(&mut connection)?;
        let search_schema_changed = run_migrations(&mut connection)?;
        if search_schema_changed || !search_index_is_complete(&connection)? {
            rebuild_search_index(&mut connection, None, |_| {})?;
        }
        let timings = performance::finish();
        let mut fields = timings.fields();
        fields.push(("total_ms", performance::millis(startup_started.elapsed())));
        crate::services::logging::event("info", "application_startup_performance", &fields);
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

    pub fn remove_book(&self, id: i64) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        // Record the public identity before foreign-key cascades remove it.
        transaction.execute("UPDATE companion_revision SET revision=revision+1", [])?;
        transaction.execute("INSERT INTO companion_changes SELECT public_id,(SELECT revision FROM companion_revision) FROM companion_books WHERE book_id=?1 ON CONFLICT(public_id) DO UPDATE SET revision=excluded.revision", [id])?;
        // Passage delete triggers need the identity too, so run them before deleting books.
        transaction.execute("DELETE FROM saved_passages WHERE book_id=?1", [id])?;
        transaction.execute("DELETE FROM book_search_fts WHERE book_id=?1", [id])?;
        transaction.execute("DELETE FROM books WHERE id=?1", [id])?;
        transaction.commit()
    }

    /// Marks catalog rows whose source vanished, retaining metadata, overrides,
    /// tags, collections, and reader progress for a later recovery scan.
    pub fn mark_missing_books(
        &self,
        root_id: i64,
        seen_paths: &[String],
    ) -> rusqlite::Result<usize> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute_batch("CREATE TEMP TABLE IF NOT EXISTS scan_seen_paths(root_id INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(root_id, path));")?;
        transaction.execute("DELETE FROM scan_seen_paths WHERE root_id=?1", [root_id])?;
        {
            let mut insert = transaction
                .prepare("INSERT OR IGNORE INTO scan_seen_paths(root_id,path) VALUES(?1,?2)")?;
            for path in seen_paths {
                insert.execute(params![root_id, path])?;
            }
        }
        let changed = transaction.execute(
            "UPDATE books SET extraction_status='unavailable', extraction_error='Source file is unavailable; rescan after restoring the path.', updated_at=?1 WHERE library_root_id=?2 AND file_path NOT IN (SELECT path FROM scan_seen_paths WHERE root_id=?2) AND extraction_status<>'unavailable'",
            params![unix_timestamp(), root_id],
        )?;
        transaction.execute("DELETE FROM scan_seen_paths WHERE root_id=?1", [root_id])?;
        transaction.commit()?;
        Ok(changed)
    }

    pub fn begin_scan(&self, root_id: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS scan_seen_paths(\
             root_id INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(root_id, path));",
        )?;
        connection.execute("DELETE FROM scan_seen_paths WHERE root_id=?1", [root_id])?;
        Ok(())
    }

    pub fn abandon_scan(&self, root_id: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("DELETE FROM scan_seen_paths WHERE root_id=?1", [root_id])?;
        Ok(())
    }

    pub fn reconcile_completed_scan(&self, root_id: i64) -> rusqlite::Result<usize> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS scan_seen_paths(\
             root_id INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(root_id, path));",
        )?;
        let changed = transaction.execute(
            "UPDATE books SET extraction_status='unavailable', extraction_error='Source file is unavailable; rescan after restoring the path.', updated_at=?1 \
             WHERE library_root_id=?2 AND NOT EXISTS (SELECT 1 FROM scan_seen_paths s WHERE s.root_id=?2 AND s.path=books.file_path) AND extraction_status<>'unavailable'",
            params![unix_timestamp(), root_id],
        )?;
        transaction.execute("DELETE FROM scan_seen_paths WHERE root_id=?1", [root_id])?;
        transaction.commit()?;
        Ok(changed)
    }

    /// Creates a consistent SQLite snapshot. It contains catalog state and
    /// paths only; EPUB and cover-cache bytes are never included.
    pub fn backup_to(&self, path: &Path) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.backup(DatabaseName::Main, path, None)
    }

    /// Restores a validated catalog snapshot into the live connection.
    pub fn restore_from(&self, path: &Path) -> rusqlite::Result<()> {
        let source = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.query_row("PRAGMA integrity_check", [], |row| {
            let status: String = row.get(0)?;
            if status == "ok" {
                Ok(())
            } else {
                Err(rusqlite::Error::InvalidQuery)
            }
        })?;
        let has_catalog: i64 = source.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='books'",
            [],
            |row| row.get(0),
        )?;
        if has_catalog != 1 {
            return Err(rusqlite::Error::InvalidQuery);
        }
        drop(source);
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        connection.restore(
            DatabaseName::Main,
            path,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        configure_connection(&connection)?;
        let search_schema_changed = run_migrations(&mut connection)?;
        if search_schema_changed || !search_index_is_complete(&connection)? {
            rebuild_search_index(&mut connection, None, |_| {})?;
        }
        // A restore may have the same numeric revision but different history.
        // Keep public identity, invalidate every pre-restore mobile cursor.
        connection.execute(
            "UPDATE companion_revision SET epoch=lower(hex(randomblob(16)))",
            [],
        )?;
        Ok(())
    }

    pub fn upsert_scanned_book(&self, book: NewBook<'_>) -> rusqlite::Result<ScanUpsert> {
        self.upsert_scanned_books(&[book])
            .map(|mut results| results.remove(0))
    }

    /// Writes one bounded discovery batch in a single transaction. Statements
    /// are prepared once per batch and every result includes the stable row id,
    /// so callers never need a second lookup by path.
    pub fn upsert_scanned_books(&self, books: &[NewBook<'_>]) -> rusqlite::Result<Vec<ScanUpsert>> {
        let now = unix_timestamp();
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS scan_seen_paths(\
             root_id INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY(root_id, path));",
        )?;
        let mut results = Vec::with_capacity(books.len());
        {
            let mut lookup = transaction.prepare_cached(
                "SELECT id, file_size, modified_time, extraction_status FROM books WHERE file_path=?1",
            )?;
            let mut upsert = transaction.prepare_cached(
                "INSERT INTO books (library_root_id, file_path, parent_folder_path, file_name, file_size, modified_time, extraction_status, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?7) \
                 ON CONFLICT(file_path) DO UPDATE SET library_root_id=excluded.library_root_id, parent_folder_path=excluded.parent_folder_path, file_name=excluded.file_name, file_size=excluded.file_size, modified_time=excluded.modified_time, extraction_status='pending', extraction_error=NULL, updated_at=excluded.updated_at \
                 RETURNING id",
            )?;
            let mut mark_seen = transaction.prepare_cached(
                "INSERT OR IGNORE INTO scan_seen_paths(root_id,path) VALUES(?1,?2)",
            )?;
            for book in books {
                mark_seen.execute(params![book.library_root_id, book.file_path])?;
                let existing: Option<(i64, i64, i64, String)> = lookup
                    .query_row([book.file_path], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                    })
                    .optional()?;
                if let Some((id, size, modified, status)) = existing {
                    if size == book.file_size
                        && modified == book.modified_time
                        && status != "unavailable"
                    {
                        results.push(ScanUpsert {
                            book_id: id,
                            changed: false,
                        });
                        continue;
                    }
                }
                let id = upsert.query_row(
                    params![
                        book.library_root_id,
                        book.file_path,
                        book.parent_folder_path,
                        book.file_name,
                        book.file_size,
                        book.modified_time,
                        now
                    ],
                    |row| row.get(0),
                )?;
                results.push(ScanUpsert {
                    book_id: id,
                    changed: true,
                });
            }
        }
        transaction.commit()?;
        Ok(results)
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
        let book = match get_book(&connection, id).optional()? {
            Some(book) => book,
            None => return Ok(None),
        };
        let override_values = connection.query_row(
            "SELECT title, creator, series_name, volume_label, cover_path, notes FROM book_overrides WHERE book_id=?1",
            [id], |row| Ok(BookOverride { title: row.get(0)?, creator: row.get(1)?, series_name: row.get(2)?, volume_label: row.get(3)?, cover_path: row.get(4)?, notes: row.get(5)? }),
        ).optional()?.unwrap_or_default();
        let tags = connection.prepare("SELECT t.id, t.name FROM tags t JOIN book_tags bt ON bt.tag_id=t.id WHERE bt.book_id=?1 ORDER BY t.name COLLATE NOCASE")?
            .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let fallback = filename_fallback(&book.file_name);
        Ok(Some(BookDetails {
            effective_title: nonempty(&override_values.title)
                .or(nonempty(&book.discovered_title))
                .unwrap_or(fallback),
            effective_creator: nonempty(&override_values.creator)
                .or(nonempty(&book.discovered_creator))
                .unwrap_or_default(),
            effective_series: nonempty(&override_values.series_name)
                .or(nonempty(&book.discovered_series))
                .unwrap_or_default(),
            effective_volume: nonempty(&override_values.volume_label)
                .or(nonempty(&book.discovered_series_index))
                .unwrap_or_default(),
            effective_cover_path: nonempty(&override_values.cover_path)
                .or(book.discovered_cover_path.clone()),
            book,
            override_values,
            tags,
        }))
    }

    pub fn reader_book(&self, id: i64) -> rusqlite::Result<Option<ReaderBook>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let book = match get_book(&connection, id).optional()? {
            Some(book) => book,
            None => return Ok(None),
        };
        let title: String = connection.query_row(
            "SELECT COALESCE(NULLIF(o.title,''), NULLIF(b.discovered_title,''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name,1,length(b.file_name)-5) ELSE b.file_name END) FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id WHERE b.id=?1",
            [id], |row| row.get(0),
        )?;
        Ok(Some(ReaderBook {
            id: book.id,
            file_path: book.file_path,
            title,
        }))
    }

    /// Only called after successful display; completion survives reopening.
    pub fn record_reader_open(&self, id: i64) -> rusqlite::Result<bool> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("UPDATE books SET reading_status='reading' WHERE id=?1 AND reading_status IN ('unset','want')", [id])?;
        transaction.execute("INSERT INTO reader_resume(book_id,last_read_at,finished) VALUES(?1,?2,(SELECT reading_status='finished' FROM books WHERE id=?1)) ON CONFLICT(book_id) DO UPDATE SET last_read_at=excluded.last_read_at", params![id, unix_timestamp()])?;
        let finished = transaction.query_row(
            "SELECT reading_status='finished' FROM books WHERE id=?1",
            [id],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(finished)
    }

    pub fn set_reader_finished(&self, id: i64, finished: bool) -> rusqlite::Result<()> {
        self.set_reading_status(id, if finished { "finished" } else { "reading" })
    }

    /// Remove only recent-list membership; the saved CFI and reading status survive.
    pub fn remove_resume_book(&self, id: i64) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("DELETE FROM reader_resume WHERE book_id=?1", [id])?;
        Ok(())
    }

    /// Bounded indexed queries, including a separate primary resume candidate.
    pub fn resume_books(&self, available_only: bool) -> rusqlite::Result<Vec<ResumeBook>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = format!("SELECT b.id,
            COALESCE(NULLIF(o.title,''),NULLIF(b.discovered_title,''),b.file_name),
            COALESCE(NULLIF(o.creator,''),NULLIF(b.discovered_creator,''),''),
            COALESCE(NULLIF(o.cover_path,''),b.discovered_cover_path),
            rr.last_read_at, EXISTS(SELECT 1 FROM reading_progress p WHERE p.book_id=b.id AND p.location_cfi<>''),
            b.extraction_status<>'unavailable'
            FROM reader_resume rr JOIN books b ON b.id=rr.book_id
            JOIN library_roots r ON r.id=b.library_root_id
            LEFT JOIN book_overrides o ON o.book_id=b.id
            WHERE rr.finished=0 AND b.reading_status NOT IN ('paused','finished') {}
            ORDER BY rr.last_read_at DESC,rr.book_id DESC LIMIT {}",
            if available_only { "AND b.extraction_status<>'unavailable'" } else { "" },
            if available_only { 1 } else { 12 });
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map([], |row| {
            Ok(ResumeBook {
                id: row.get(0)?,
                title: row.get(1)?,
                creator: row.get(2)?,
                cover_path: row.get(3)?,
                last_read_at: row.get(4)?,
                has_location: row.get(5)?,
                is_available: row.get(6)?,
            })
        })?;
        rows.collect()
    }

    pub fn reading_location(&self, id: i64) -> rusqlite::Result<Option<String>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT p.location_cfi FROM reading_progress p JOIN books b ON b.id=p.book_id WHERE p.book_id=?1 AND (p.content_version IS NULL OR EXISTS(SELECT 1 FROM companion_content_versions v WHERE v.book_id=p.book_id AND v.version=p.content_version AND v.source_size=b.file_size AND v.source_modified=b.modified_time))",
                [id],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn save_reading_location(&self, id: i64, location_cfi: &str) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute("INSERT INTO reading_progress(book_id,location_cfi,updated_at,content_version) VALUES(?1,?2,?3,(SELECT v.version FROM companion_content_versions v JOIN books b ON b.id=v.book_id WHERE b.id=?1 AND v.source_size=b.file_size AND v.source_modified=b.modified_time)) ON CONFLICT(book_id) DO UPDATE SET location_cfi=excluded.location_cfi, updated_at=excluded.updated_at,content_version=excluded.content_version", params![id, location_cfi, unix_timestamp()])?;
        Ok(())
    }

    pub fn replace_jmdict(
        &self,
        source_path: &str,
        entries: &[crate::services::dictionary::ImportedEntry],
    ) -> rusqlite::Result<usize> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM dictionaries WHERE name='JMdict'", [])?;
        transaction.execute("INSERT INTO dictionaries(name,source_path,enabled,imported_at) VALUES('JMdict',?1,1,?2)", params![source_path, unix_timestamp()])?;
        let dictionary_id = transaction.last_insert_rowid();
        let mut insert = transaction.prepare("INSERT INTO dictionary_entries(dictionary_id,term,reading,term_normalized,reading_normalized,definitions,part_of_speech) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
        for entry in entries {
            insert.execute(params![
                dictionary_id,
                entry.term,
                entry.reading,
                normalize_for_search(&entry.term),
                entry.reading.as_deref().map(normalize_for_search),
                serde_json::to_string(&entry.definitions).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&entry.part_of_speech).unwrap_or_else(|_| "[]".into())
            ])?;
        }
        drop(insert);
        transaction.commit()?;
        Ok(entries.len())
    }

    pub fn dictionaries(&self) -> rusqlite::Result<Vec<DictionarySummary>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT d.id,d.name,d.source_path,d.enabled,d.imported_at,COUNT(e.id) FROM dictionaries d LEFT JOIN dictionary_entries e ON e.dictionary_id=d.id GROUP BY d.id ORDER BY d.name")?;
        let result = statement
            .query_map([], |row| {
                Ok(DictionarySummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    source_path: row.get(2)?,
                    enabled: row.get::<_, i64>(3)? != 0,
                    imported_at: row.get(4)?,
                    entry_count: row.get(5)?,
                })
            })?
            .collect();
        result
    }

    pub fn set_dictionary_enabled(&self, id: i64, enabled: bool) -> rusqlite::Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "UPDATE dictionaries SET enabled=?1 WHERE id=?2",
            params![i64::from(enabled), id],
        )?;
        Ok(())
    }

    pub fn dictionary_lookup(&self, query: &str) -> rusqlite::Result<Vec<DictionaryEntry>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let query = normalize_for_search(query.trim());
        if query.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!("SELECT e.id,e.term,e.reading,e.definitions,e.part_of_speech,d.name FROM dictionary_entries e JOIN dictionaries d ON d.id=e.dictionary_id WHERE d.enabled=1 AND {}", tmw_japanese_core::dictionary::LOOKUP_PREDICATE);
        let mut statement = connection.prepare(&sql)?;
        let result = statement
            .query_map(params![query, format!("{query}%")], |row| {
                Ok(DictionaryEntry {
                    id: row.get(0)?,
                    term: row.get(1)?,
                    reading: row.get(2)?,
                    definitions: serde_json::from_str(&row.get::<_, String>(3)?)
                        .unwrap_or_default(),
                    part_of_speech: serde_json::from_str(&row.get::<_, String>(4)?)
                        .unwrap_or_default(),
                    dictionary_name: row.get(5)?,
                })
            })?
            .collect();
        result
    }

    pub fn save_book_overrides(&self, id: i64, values: &BookOverride) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("INSERT INTO book_overrides(book_id,title,creator,series_name,volume_label,cover_path,notes,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(book_id) DO UPDATE SET title=excluded.title,creator=excluded.creator,series_name=excluded.series_name,volume_label=excluded.volume_label,cover_path=excluded.cover_path,notes=excluded.notes,updated_at=excluded.updated_at", params![id, values.title, values.creator, values.series_name, values.volume_label, values.cover_path, values.notes, unix_timestamp()])?;
        refresh_search_document(&transaction, id)?;
        transaction.commit()
    }

    pub fn reset_book_override(&self, id: i64, field: &str) -> rusqlite::Result<()> {
        let column = match field {
            "title" | "creator" | "series_name" | "volume_label" | "cover_path" | "notes" => field,
            _ => {
                return Err(rusqlite::Error::InvalidParameterName(
                    "Unknown override field".into(),
                ))
            }
        };
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            &format!("UPDATE book_overrides SET {column}=NULL, updated_at=?1 WHERE book_id=?2"),
            params![unix_timestamp(), id],
        )?;
        refresh_search_document(&transaction, id)?;
        transaction.commit()
    }

    pub fn reset_all_book_overrides(&self, id: i64) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM book_overrides WHERE book_id=?1", [id])?;
        refresh_search_document(&transaction, id)?;
        transaction.commit()
    }

    pub fn save_reading_override(
        &self,
        id: i64,
        reading: Option<&str>,
        aliases: Option<&str>,
    ) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute("INSERT INTO book_reading_overrides(book_id,reading,aliases,updated_at) VALUES(?1,?2,?3,?4) ON CONFLICT(book_id) DO UPDATE SET reading=excluded.reading,aliases=excluded.aliases,updated_at=excluded.updated_at", params![id, reading, aliases, unix_timestamp()])?;
        refresh_search_document(&transaction, id)?;
        transaction.commit()
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
        let clean: Vec<String> = names
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        for name in &clean {
            transaction.execute(
                "INSERT INTO tags(name) VALUES(?1) ON CONFLICT(name) DO NOTHING",
                [name],
            )?;
        }
        for id in book_ids {
            transaction.execute("DELETE FROM book_tags WHERE book_id=?1", [id])?;
            for name in &clean {
                transaction.execute(
                    "INSERT INTO book_tags(book_id,tag_id) SELECT ?1,id FROM tags WHERE name=?2",
                    params![id, name],
                )?;
            }
        }
        for id in book_ids {
            refresh_search_document(&transaction, *id)?;
        }
        transaction.commit()
    }

    pub fn save_extracted_metadata(
        &self,
        book_id: i64,
        metadata: &ExtractedBookMetadata,
    ) -> rusqlite::Result<()> {
        self.save_extracted_metadata_batch(&[(book_id, metadata)])
    }

    pub fn save_extracted_metadata_batch(
        &self,
        books: &[(i64, &ExtractedBookMetadata)],
    ) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        {
            let mut update = transaction.prepare_cached(
                "UPDATE books SET discovered_title=?1, discovered_creator=?2, discovered_language=?3, \
                 discovered_identifier=?4, discovered_series=?5, discovered_series_index=?6, discovered_cover_path=?7, \
                 extraction_status=?8, extraction_error=?9, updated_at=?10 WHERE id=?11",
            )?;
            for (book_id, metadata) in books {
                let status = if metadata.extraction_error.is_some() {
                    "error"
                } else {
                    "complete"
                };
                update.execute(params![
                    metadata.title,
                    metadata.creator,
                    metadata.language,
                    metadata.identifier,
                    metadata.series,
                    metadata.series_index,
                    metadata.cover_path,
                    status,
                    metadata.extraction_error,
                    unix_timestamp(),
                    book_id
                ])?;
            }
        }
        for (book_id, _) in books {
            refresh_search_document(&transaction, *book_id)?;
        }
        transaction.commit()
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

    pub fn rebuild_search_index(
        &self,
        cancelled: &AtomicBool,
        progress: impl FnMut((usize, usize)),
    ) -> rusqlite::Result<bool> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        rebuild_search_index(&mut connection, Some(cancelled), progress)
    }

    pub fn browse_books(&self, request: &BrowseBooksRequest) -> rusqlite::Result<Vec<BrowserBook>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        shelves::validate_filter(request)?;
        let query = normalize_for_search(&request.query);
        let romaji_query = normalize_romaji(&request.query).replace(' ', "");
        let sort = match request.sort.as_str() {
            "author" => "effective_creator COLLATE NOCASE, effective_title COLLATE NOCASE",
            "series" => "effective_series COLLATE NOCASE, effective_volume COLLATE NOCASE, effective_title COLLATE NOCASE",
            "dateAdded" => "b.created_at DESC, b.id DESC",
            "modified" => "b.modified_time DESC, b.id DESC",
            "folder" => "b.parent_folder_path COLLATE NOCASE, b.file_name COLLATE NOCASE",
            _ => "effective_title COLLATE NOCASE, b.file_name COLLATE NOCASE",
        };
        // FTS5's trigram tokenizer can satisfy true substring matching without
        // scanning every document. One- and two-character searches retain the
        // deterministic normalized fallback because trigram has no token for them.
        let use_fts = query.chars().count() >= 3 || romaji_query.chars().count() >= 3;
        let fts_query = [query.as_str(), romaji_query.as_str()]
            .into_iter()
            .filter(|value| value.chars().count() >= 3)
            .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let search_join = if use_fts {
            "JOIN (SELECT book_id FROM book_search_fts WHERE book_search_fts MATCH ?10) matched ON matched.book_id=b.id"
        } else {
            ""
        };
        let candidate_predicate = match request.duplicate_filtering.as_deref() {
            Some("off") => "",
            Some("high") => duplicates::HIGH_PREDICATE,
            Some("all") => duplicates::ALL_PREDICATE,
            Some(_) => return Err(rusqlite::Error::InvalidParameterName("duplicateFiltering".into())),
            None if request.hide_duplicate_titles => duplicates::HIGH_PREDICATE,
            None => "",
        };
        // Exact-title collapsing also covers books absent from the candidate file.
        // Consider only candidate survivors so the two filters cannot hide each
        // other's representative (for example when an older copy is unavailable).
        let earlier_candidate_predicate = candidate_predicate.replace("b.", "earlier_book.");
        let title_predicate = if request.hide_duplicate_titles
            && request.duplicate_filtering.as_deref() != Some("off")
        {
            format!("AND (d.title_normalized='' OR NOT EXISTS (SELECT 1 FROM book_search_documents earlier JOIN books earlier_book ON earlier_book.id=earlier.book_id WHERE earlier.book_id<b.id AND lower(earlier.title_normalized)=lower(d.title_normalized) {earlier_candidate_predicate}))")
        } else { String::new() };
        let duplicate_predicate = format!("{candidate_predicate} {title_predicate}");
        let search_predicate = if use_fts {
            ""
        } else {
            "AND (?5='' OR instr(d.title_normalized, ?5)>0 OR instr(d.creator_normalized, ?5)>0 OR instr(d.series_normalized, ?5)>0 OR instr(d.file_name_normalized, ?5)>0 OR instr(d.parent_folder_normalized, ?5)>0 OR instr(d.tags_normalized, ?5)>0 OR instr(d.title_reading, ?5)>0 OR instr(d.creator_reading, ?5)>0 OR instr(d.series_reading, ?5)>0 OR instr(d.file_name_reading, ?5)>0 OR instr(d.aliases_normalized, ?5)>0 OR instr(replace(d.title_romaji, ' ', ''), ?6)>0 OR instr(replace(d.creator_romaji, ' ', ''), ?6)>0 OR instr(replace(d.series_romaji, ' ', ''), ?6)>0 OR instr(replace(d.file_name_romaji, ' ', ''), ?6)>0 OR instr(replace(d.aliases_romaji, ' ', ''), ?6)>0)"
        };
        let rank_order = if query.is_empty() {
            ""
        } else {
            "CASE WHEN (NULLIF(o.title,'') IS NOT NULL OR NULLIF(b.discovered_title,'') IS NOT NULL) AND d.title_normalized=?5 THEN 0 WHEN (NULLIF(o.title,'') IS NOT NULL OR NULLIF(b.discovered_title,'') IS NOT NULL) AND d.title_normalized LIKE ?5 || '%' THEN 1 WHEN d.title_reading=?5 OR replace(d.title_romaji, ' ', '')=?6 THEN 2 WHEN d.title_reading LIKE ?5 || '%' OR replace(d.title_romaji, ' ', '') LIKE ?6 || '%' THEN 3 WHEN replace(d.title_romaji, ' ', '') LIKE '%' || ?6 || '%' THEN 4 ELSE 5 END, "
        };
        let status_predicate = if request.reading_status.is_some() {
            "b.reading_status=?9"
        } else {
            "?9 IS NULL"
        };
        let sql = format!(
            "SELECT b.id, b.library_root_id, b.file_name, b.parent_folder_path, \
             COALESCE(NULLIF(o.title, ''), NULLIF(b.discovered_title, ''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name, 1, length(b.file_name)-5) ELSE b.file_name END) AS effective_title, \
             COALESCE(NULLIF(o.creator, ''), NULLIF(b.discovered_creator, ''), '') AS effective_creator, \
             COALESCE(NULLIF(o.series_name, ''), NULLIF(b.discovered_series, ''), '') AS effective_series, \
             COALESCE(NULLIF(o.volume_label, ''), NULLIF(b.discovered_series_index, ''), '') AS effective_volume, \
             COALESCE(NULLIF(o.cover_path, ''), b.discovered_cover_path), b.created_at, b.modified_time, \
             (b.discovered_title IS NULL OR trim(b.discovered_title) = '' OR b.extraction_status = 'error') AS needs_metadata, b.extraction_status <> 'unavailable' AS is_available, b.reading_status='finished' AS is_finished \
             FROM books b JOIN library_roots r ON r.id=b.library_root_id \
             LEFT JOIN book_overrides o ON o.book_id=b.id \
             LEFT JOIN book_search_documents d ON d.book_id=b.id \
             {search_join} \
             WHERE {status_predicate} \
             AND (?1 IS NULL OR b.library_root_id=?1) \
             AND (?2 IS NULL OR EXISTS (SELECT 1 FROM book_tags bt WHERE bt.book_id=b.id AND bt.tag_id=?2)) \
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM collection_books cb WHERE cb.book_id=b.id AND cb.collection_id=?3)) \
             AND (?4=0 OR b.discovered_title IS NULL OR trim(b.discovered_title)='' OR b.extraction_status='error') \
             {duplicate_predicate} \
             {search_predicate} \
             ORDER BY {rank_order}{sort} LIMIT ?7 OFFSET ?8"
        );
        let mut statement = connection.prepare(&sql)?;
        let mut parameter_values = vec![
            request.library_root_id.map_or(Value::Null, Value::Integer),
            request.tag_id.map_or(Value::Null, Value::Integer),
            request.collection_id.map_or(Value::Null, Value::Integer),
            Value::Integer(request.needs_metadata as i64),
            Value::Text(query),
            Value::Text(romaji_query),
            Value::Integer(request.limit.clamp(1, 200)),
            Value::Integer(request.offset.max(0)),
            request
                .reading_status
                .clone()
                .map_or(Value::Null, Value::Text),
        ];
        if use_fts {
            parameter_values.push(Value::Text(fts_query));
        }
        let books = statement
            .query_map(params_from_iter(parameter_values), |row| {
                Ok(BrowserBook {
                    id: row.get(0)?,
                    library_root_id: row.get(1)?,
                    file_name: row.get(2)?,
                    parent_folder_path: row.get(3)?,
                    effective_title: row.get(4)?,
                    effective_creator: row.get(5)?,
                    effective_series: row.get(6)?,
                    effective_volume: row.get(7)?,
                    effective_cover_path: row.get(8)?,
                    created_at: row.get(9)?,
                    modified_time: row.get(10)?,
                    needs_metadata: row.get(11)?,
                    is_available: row.get(12)?,
                    is_finished: row.get(13)?,
                })
            })?
            .collect();
        books
    }

    pub fn tags(&self) -> rusqlite::Result<Vec<(i64, String)>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement =
            connection.prepare("SELECT id, name FROM tags ORDER BY name COLLATE NOCASE")?;
        let tags = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect();
        tags
    }

    pub fn collections(&self) -> rusqlite::Result<Vec<(i64, String)>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement =
            connection.prepare("SELECT id, name FROM collections ORDER BY name COLLATE NOCASE")?;
        let collections = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect();
        collections
    }

    pub fn folder_group(&self, book_id: i64) -> rusqlite::Result<Option<FolderGroup>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let parent: Option<String> = connection
            .query_row(
                "SELECT parent_folder_path FROM books WHERE id=?1",
                [book_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(parent_folder_path) = parent else {
            return Ok(None);
        };
        let mut statement = connection.prepare("SELECT b.id, COALESCE(NULLIF(o.title,''), NULLIF(b.discovered_title,''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name,1,length(b.file_name)-5) ELSE b.file_name END), COALESCE(NULLIF(o.volume_label,''), NULLIF(b.discovered_series_index,''), ''), b.file_name, NULLIF(o.series_name,'') IS NOT NULL FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id WHERE b.parent_folder_path=?1")?;
        let mut books = statement
            .query_map([&parent_folder_path], |row| {
                Ok(FolderGroupBook {
                    id: row.get(0)?,
                    effective_title: row.get(1)?,
                    effective_volume: row.get(2)?,
                    file_name: row.get(3)?,
                    suggested_volume: None,
                    has_series_override: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for book in &mut books {
            book.suggested_volume = suggested_volume(&book.effective_volume)
                .or_else(|| suggested_volume(&book.effective_title))
                .or_else(|| suggested_volume(&book.file_name));
        }
        books.sort_by(|left, right| {
            left.suggested_volume
                .partial_cmp(&right.suggested_volume)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| natural_compare(&left.file_name, &right.file_name))
        });
        Ok(Some(FolderGroup {
            parent_folder_path,
            books,
        }))
    }

    pub fn create_collection(&self, request: &CreateCollectionRequest) -> rusqlite::Result<i64> {
        let name = request.name.trim();
        if name.is_empty() || request.book_ids.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "A collection name and at least one book are required.".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO collections(name,created_at) VALUES(?1,?2)",
            params![name, unix_timestamp()],
        )?;
        let id = transaction.last_insert_rowid();
        for (order, book_id) in request.book_ids.iter().enumerate() {
            transaction.execute(
                "INSERT INTO collection_books(collection_id,book_id,sort_order) VALUES(?1,?2,?3)",
                params![id, book_id, order as i64],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    pub fn assign_series(&self, request: &AssignSeriesRequest) -> rusqlite::Result<()> {
        let series = request.series_name.trim();
        if series.is_empty() || request.book_ids.is_empty() {
            return Err(rusqlite::Error::InvalidParameterName(
                "A series name and at least one book are required.".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        for book_id in &request.book_ids {
            transaction.execute("INSERT INTO book_overrides(book_id,series_name,updated_at) VALUES(?1,?2,?3) ON CONFLICT(book_id) DO UPDATE SET series_name=excluded.series_name,updated_at=excluded.updated_at", params![book_id, series, unix_timestamp()])?;
        }
        for book_id in &request.book_ids {
            refresh_search_document(&transaction, *book_id)?;
        }
        transaction.commit()
    }

    pub fn books_for_cover_regeneration(&self) -> rusqlite::Result<Vec<Book>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id FROM books ORDER BY id")?;
        let ids: Vec<i64> = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        ids.into_iter()
            .map(|id| get_book(&connection, id))
            .collect()
    }
}

fn configure_connection(connection: &Connection) -> rusqlite::Result<()> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    Ok(())
}

fn run_migrations(connection: &mut Connection) -> rusqlite::Result<bool> {
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
    if version < 5 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(READER_PROGRESS_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 5)?;
        transaction.commit()?;
    }
    if version < 6 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(DICTIONARY_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 6)?;
        transaction.commit()?;
    }
    if version < 7 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(DICTIONARY_LOOKUP_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 7)?;
        transaction.commit()?;
    }
    if version < 8 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(SEARCH_INDEX_STATE_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 8)?;
        transaction.commit()?;
    }
    if version < 9 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(LARGE_LIBRARY_QUERY_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 9)?;
        transaction.commit()?;
    }
    if version < 10 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(DUPLICATE_TITLE_LOOKUP_SCHEMA)?;
        transaction.pragma_update(None, "user_version", 10)?;
        transaction.commit()?;
    }
    if version < 11 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/011_continue_reading.sql"))?;
        transaction.pragma_update(None, "user_version", 11)?;
        transaction.commit()?;
    }
    if version < 12 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/012_saved_passages.sql"))?;
        transaction.pragma_update(None, "user_version", 12)?;
        transaction.commit()?;
    }
    if version < 13 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/013_lookup_history.sql"))?;
        transaction.pragma_update(None, "user_version", 13)?;
        transaction.commit()?;
    }
    if version < 14 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/014_reading_shelves.sql"))?;
        transaction.pragma_update(None, "user_version", 14)?;
        transaction.commit()?;
    }
    if version < 15 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/015_companion_identity.sql"))?;
        transaction.pragma_update(None, "user_version", 15)?;
        transaction.commit()?;
    }
    if version < 16 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/016_companion_revisions.sql"))?;
        // Metadata and membership changes invalidate cursors in the same transaction.
        for (table, key) in [
            ("books", "id"),
            ("book_overrides", "book_id"),
            ("book_reading_overrides", "book_id"),
            ("book_tags", "book_id"),
            ("collection_books", "book_id"),
            ("book_search_documents", "book_id"),
        ] {
            for (operation, row) in [("INSERT", "new"), ("UPDATE", "new"), ("DELETE", "old")] {
                transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS companion_{table}_{operation} AFTER {operation} ON {table} BEGIN
                  UPDATE companion_revision SET revision=revision+1;
                  INSERT INTO companion_changes SELECT public_id,(SELECT revision FROM companion_revision) FROM companion_books WHERE book_id={row}.{key} ON CONFLICT(public_id) DO UPDATE SET revision=excluded.revision; END;"))?;
            }
        }
        for (table, membership, key) in [
            ("tags", "book_tags", "tag_id"),
            ("collections", "collection_books", "collection_id"),
        ] {
            transaction.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS companion_{table}_rename AFTER UPDATE ON {table} BEGIN
                UPDATE companion_revision SET revision=revision+1;
                INSERT INTO companion_changes SELECT c.public_id,(SELECT revision FROM companion_revision) FROM companion_books c JOIN {membership} m ON m.book_id=c.book_id WHERE m.{key}=new.id ON CONFLICT(public_id) DO UPDATE SET revision=excluded.revision; END;"))?;
        }
        transaction.pragma_update(None, "user_version", 16)?;
        transaction.commit()?;
    }
    if version < 17 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/017_user_sync.sql"))?;
        transaction.pragma_update(None, "user_version", 17)?;
        transaction.commit()?;
    }
    if version < 18 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/018_lookup_sync.sql"))?;
        transaction.pragma_update(None, "user_version", 18)?;
        transaction.commit()?;
    }
    if version < 19 {
        let transaction = connection.transaction()?;
        transaction.execute_batch(include_str!("migrations/019_romaji_spacing.sql"))?;
        transaction.pragma_update(None, "user_version", 19)?;
        transaction.commit()?;
    }
    Ok(version < 8)
}

fn search_index_is_complete(connection: &Connection) -> rusqlite::Result<bool> {
    let version = connection
        .query_row(
            "SELECT schema_version FROM search_index_state WHERE singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if version != Some(SEARCH_INDEX_VERSION) {
        return Ok(false);
    }
    let books: i64 = connection.query_row("SELECT COUNT(*) FROM books", [], |row| row.get(0))?;
    let documents: i64 =
        connection.query_row("SELECT COUNT(*) FROM book_search_documents", [], |row| {
            row.get(0)
        })?;
    let fts: i64 =
        connection.query_row("SELECT COUNT(*) FROM book_search_fts", [], |row| row.get(0))?;
    Ok(books == documents && books == fts)
}

fn rebuild_search_index(
    connection: &mut Connection,
    cancelled: Option<&AtomicBool>,
    mut progress: impl FnMut((usize, usize)),
) -> rusqlite::Result<bool> {
    let ids: Vec<i64> = connection
        .prepare("SELECT id FROM books")?
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    let total = ids.len();
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM book_search_documents", [])?;
    transaction.execute("DELETE FROM book_search_fts", [])?;
    for (index, id) in ids.into_iter().enumerate() {
        if cancelled.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Ok(false);
        }
        refresh_search_document(&transaction, id)?;
        if (index + 1) % 100 == 0 || index + 1 == total {
            progress((index + 1, total));
        }
    }
    transaction.execute("INSERT INTO search_index_state(singleton,schema_version,completed_at) VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET schema_version=excluded.schema_version,completed_at=excluded.completed_at", params![SEARCH_INDEX_VERSION, unix_timestamp()])?;
    transaction.commit()?;
    Ok(true)
}

fn refresh_search_document(connection: &Connection, book_id: i64) -> rusqlite::Result<()> {
    performance::measure(Stage::SearchDocumentUpdate, || {
        refresh_search_document_inner(connection, book_id)
    })
}

fn refresh_search_document_inner(connection: &Connection, book_id: i64) -> rusqlite::Result<()> {
    let row: (String, String, String, String, String, String, String, String) = connection.query_row(
        "SELECT COALESCE(NULLIF(o.title,''), NULLIF(b.discovered_title,''), CASE WHEN lower(b.file_name) LIKE '%.epub' THEN substr(b.file_name,1,length(b.file_name)-5) ELSE b.file_name END), COALESCE(NULLIF(o.creator,''), NULLIF(b.discovered_creator,''), ''), COALESCE(NULLIF(o.series_name,''), NULLIF(b.discovered_series,''), ''), b.file_name, b.parent_folder_path, COALESCE((SELECT group_concat(t.name, ' ') FROM book_tags bt JOIN tags t ON t.id=bt.tag_id WHERE bt.book_id=b.id), ''), COALESCE(ro.reading,''), COALESCE(ro.aliases,'') FROM books b LEFT JOIN book_overrides o ON o.book_id=b.id LEFT JOIN book_reading_overrides ro ON ro.book_id=b.id WHERE b.id=?1",
        [book_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)))?;
    let n = [
        normalize_for_search(&row.0),
        normalize_for_search(&row.1),
        normalize_for_search(&row.2),
        normalize_for_search(&row.3),
        normalize_for_search(&row.4),
        normalize_for_search(&row.5),
    ];
    let generated = |value: &str| derive_reading(value).unwrap_or_default();
    let title_reading = if row.6.trim().is_empty() {
        generated(&row.0)
    } else {
        normalize_for_search(&row.6)
    };
    let creator_reading = generated(&row.1);
    let series_reading = generated(&row.2);
    let file_name_reading = generated(&row.3);
    let title_romaji = kana_to_romaji(&title_reading);
    let creator_romaji = kana_to_romaji(&creator_reading);
    let series_romaji = kana_to_romaji(&series_reading);
    let file_name_romaji = kana_to_romaji(&file_name_reading);
    let aliases = normalize_for_search(&row.7);
    let aliases_romaji = normalize_romaji(&row.7);
    connection.execute("INSERT INTO book_search_documents(book_id,title_normalized,creator_normalized,series_normalized,file_name_normalized,parent_folder_normalized,tags_normalized,title_reading,creator_reading,series_reading,file_name_reading,aliases_normalized,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17) ON CONFLICT(book_id) DO UPDATE SET title_normalized=excluded.title_normalized,creator_normalized=excluded.creator_normalized,series_normalized=excluded.series_normalized,file_name_normalized=excluded.file_name_normalized,parent_folder_normalized=excluded.parent_folder_normalized,tags_normalized=excluded.tags_normalized,title_reading=excluded.title_reading,creator_reading=excluded.creator_reading,series_reading=excluded.series_reading,file_name_reading=excluded.file_name_reading,aliases_normalized=excluded.aliases_normalized,title_romaji=excluded.title_romaji,creator_romaji=excluded.creator_romaji,series_romaji=excluded.series_romaji,file_name_romaji=excluded.file_name_romaji,aliases_romaji=excluded.aliases_romaji", params![book_id,n[0],n[1],n[2],n[3],n[4],n[5],title_reading,creator_reading,series_reading,file_name_reading,aliases,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji])?;
    connection.execute("DELETE FROM book_search_fts WHERE book_id=?1", [book_id])?;
    connection.execute("INSERT INTO book_search_fts(book_id,title,creator,series,file_name,parent_folder,tags,title_reading,creator_reading,series_reading,file_name_reading,aliases,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)", params![book_id,n[0],n[1],n[2],n[3],n[4],n[5],title_reading,creator_reading,series_reading,file_name_reading,aliases,romaji_search_variants(&title_romaji),romaji_search_variants(&creator_romaji),romaji_search_variants(&series_romaji),romaji_search_variants(&file_name_romaji),romaji_search_variants(&aliases_romaji)])?;
    Ok(())
}

// Keep readable token spacing and an additional contiguous spelling in FTS.
fn romaji_search_variants(value: &str) -> String {
    let compact = value.replace(' ', "");
    if compact == value { value.to_owned() } else { format!("{value} {compact}") }
}

fn nonempty(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn filename_fallback(file_name: &str) -> String {
    file_name
        .strip_suffix(".epub")
        .or_else(|| file_name.strip_suffix(".EPUB"))
        .unwrap_or(file_name)
        .to_owned()
}

fn natural_compare(left: &str, right: &str) -> std::cmp::Ordering {
    let left = left.nfkc().collect::<String>();
    let right = right.nfkc().collect::<String>();
    left.to_lowercase().cmp(&right.to_lowercase())
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
    use std::time::Instant;
    use tempfile::tempdir;

    const SCAN_BENCHMARK_BATCH_SIZE: usize = 128;
    struct GeneratedScanRow {
        path: String,
        parent: String,
        name: String,
        size: i64,
        modified: i64,
    }
    fn generated_scan_rows(
        start: usize,
        end: usize,
        modified: i64,
        size_base: i64,
    ) -> Vec<GeneratedScanRow> {
        (start..end)
            .map(|index| GeneratedScanRow {
                path: format!(
                    r"F:\generated-performance-fixture\series-{0:04}\本-{0:05}.epub",
                    index
                ),
                parent: format!(r"F:\generated-performance-fixture\series-{0:04}", index),
                name: format!("本-{index:05}.epub"),
                size: size_base + index as i64,
                modified,
            })
            .collect()
    }
    fn scan_row_refs(root_id: i64, rows: &[GeneratedScanRow]) -> Vec<NewBook<'_>> {
        rows.iter()
            .map(|row| NewBook {
                library_root_id: root_id,
                file_path: &row.path,
                parent_folder_path: &row.parent,
                file_name: &row.name,
                file_size: row.size,
                modified_time: row.modified,
            })
            .collect()
    }

    #[test]
    #[ignore = "generated 10,000 and 100,000 row smart-shelf benchmark"]
    fn smart_shelf_large_catalog_benchmark() {
        for count in [10_000, 100_000] {
            let dir = tempdir().unwrap();
            let db = Database::open(&dir.path().join("catalog.sqlite3")).unwrap();
            seed_query_fixture(&db, count);
            db.connection
                .lock()
                .unwrap()
                .execute("UPDATE books SET reading_status='want' WHERE id%5=0", [])
                .unwrap();
            let filter = BrowseBooksRequest {
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
            };
            let connection = db.connection.lock().unwrap();
            let mut stmt = connection
                .prepare("EXPLAIN QUERY PLAN SELECT id FROM books WHERE reading_status='want'")
                .unwrap();
            let plan: Vec<String> = stmt
                .query_map([], |r| r.get(3))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap();
            assert!(plan.iter().any(|s| s.contains("idx_books_reading_status")));
            drop(stmt);
            drop(connection);
            for query in ["", "進撃", "shingeki"] {
                let mut request = filter.clone();
                request.query = query.into();
                let start = Instant::now();
                for _ in 0..20 {
                    let rows = db.browse_books(&request).unwrap();
                    assert!(rows.len() <= 80);
                }
                eprintln!(
                    "smart_shelves rows={count} query={query:?} mean_ms={:.3}",
                    start.elapsed().as_secs_f64() * 1000.0 / 20.0
                );
            }
        }
    }

    fn seed_query_fixture(database: &Database, rows: usize) {
        let mut connection = database.connection.lock().unwrap();
        let transaction = connection.transaction().unwrap();
        transaction
            .execute(
                "INSERT INTO library_roots(path,display_name,added_at) VALUES('F:\\query-fixture','Query fixture',1)",
                [],
            )
            .unwrap();
        let root_id = transaction.last_insert_rowid();
        {
            let mut book = transaction.prepare("INSERT INTO books(library_root_id,file_path,parent_folder_path,file_name,file_size,modified_time,discovered_title,discovered_creator,discovered_series,extraction_status,created_at,updated_at) VALUES(?1,?2,?3,?4,1024,?5,?6,?7,?8,'complete',?9,?9)").unwrap();
            let mut document = transaction.prepare("INSERT INTO book_search_documents(book_id,title_normalized,creator_normalized,series_normalized,file_name_normalized,parent_folder_normalized,tags_normalized,title_reading,creator_reading,series_reading,file_name_reading,aliases_normalized,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,'',?7,'','','','',?8,'','','','')").unwrap();
            let mut fts = transaction.prepare("INSERT INTO book_search_fts(book_id,title,creator,series,file_name,parent_folder,tags,title_reading,creator_reading,series_reading,file_name_reading,aliases,title_romaji,creator_romaji,series_romaji,file_name_romaji,aliases_romaji) VALUES(?1,?2,?3,?4,?5,?6,'',?7,'','','','',?8,'','','','')").unwrap();
            for index in 0..rows {
                let is_match = index % 997 == 0;
                let title = if is_match {
                    format!("進撃の巨人 {index:06}")
                } else {
                    format!("作品 {index:06}")
                };
                let creator = format!("著者 {:04}", index % 500);
                let series = format!("シリーズ {:04}", index % 2_000);
                let file_name = format!("book-{index:06}.epub");
                let parent = format!(r"F:\query-fixture\{:04}", index % 1_000);
                book.execute(params![
                    root_id,
                    format!(r"{parent}\{file_name}"),
                    parent,
                    file_name,
                    index as i64,
                    title,
                    creator,
                    series,
                    index as i64
                ])
                .unwrap();
                let id = transaction.last_insert_rowid();
                let reading = if is_match {
                    "しんげきのきょじん"
                } else {
                    ""
                };
                let romaji = if is_match { "shingeki no kyojin" } else { "" };
                document
                    .execute(params![
                        id, title, creator, series, file_name, parent, reading, romaji
                    ])
                    .unwrap();
                fts.execute(params![
                    id, title, creator, series, file_name, parent, reading, romaji
                ])
                .unwrap();
            }
        }
        transaction.commit().unwrap();
    }

    #[test]
    fn large_library_migration_and_fts_plan_use_indexes() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        seed_query_fixture(&database, 1_000);
        let connection = database.connection.lock().unwrap();
        let indexes: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_books_created_at_id','idx_books_modified_time_id','idx_books_folder_file','idx_books_root_created_at_id')", [], |row| row.get(0)).unwrap();
        assert_eq!(indexes, 4);
        let details = connection.prepare("EXPLAIN QUERY PLAN SELECT book_id FROM book_search_fts WHERE book_search_fts MATCH '\"進撃\"'").unwrap().query_map([], |row| row.get::<_, String>(3)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
        assert!(
            details
                .iter()
                .any(|detail| detail.contains("VIRTUAL TABLE INDEX")),
            "query plan did not use FTS: {details:?}"
        );
    }

    /// Repeatable Phase 6 benchmark. It inserts synthetic catalog/search rows
    /// directly, never reads source EPUBs, and reports browse/search timings.
    #[test]
    #[ignore = "performance benchmark; run explicitly with --ignored --nocapture"]
    fn performance_phase6_queries_10000_and_100000_rows() {
        for rows in [10_000, 100_000] {
            let directory = tempdir().unwrap();
            let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
            seed_query_fixture(&database, rows);
            let run = |query: &str, sort: &str, offset: i64| {
                let request = BrowseBooksRequest {
                    reading_status: None,
                    library_root_id: None,
                    tag_id: None,
                    collection_id: None,
                    needs_metadata: false,
                    hide_duplicate_titles: false, duplicate_filtering: None,
                    query: query.into(),
                    sort: sort.into(),
                    offset,
                    limit: 80,
                };
                let started = Instant::now();
                for _ in 0..25 {
                    std::hint::black_box(database.browse_books(&request).unwrap());
                }
                started.elapsed().as_secs_f64() * 1_000.0 / 25.0
            };
            println!("PERF_PHASE6 rows={rows} recent_page_ms={:.3} deep_page_ms={:.3} japanese_substring_ms={:.3} romaji_substring_ms={:.3}", run("", "dateAdded", 0), run("", "modified", (rows.saturating_sub(80)) as i64), run("進撃", "title", 0), run("shingeki", "title", 0));
        }
    }

    /// Repeatable generated-catalog comparison. The Phase 1 results remain in
    /// README; later phases exercise the current bounded scan-write path.
    #[test]
    #[ignore = "performance baseline; run explicitly with --ignored --nocapture"]
    fn performance_baseline_10000_catalog_rows() {
        const ROWS: usize = 10_000;
        let directory = tempdir().unwrap();
        let catalog = directory.path().join("catalog.sqlite3");
        let database = Database::open(&catalog).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\generated-performance-fixture",
                display_name: "Generated performance fixture",
            })
            .unwrap();

        let started = Instant::now();
        for start in (0..ROWS).step_by(SCAN_BENCHMARK_BATCH_SIZE) {
            let rows = generated_scan_rows(
                start,
                (start + SCAN_BENCHMARK_BATCH_SIZE).min(ROWS),
                1,
                1024,
            );
            let books = scan_row_refs(root.id, &rows);
            database.upsert_scanned_books(&books).unwrap();
        }
        let initial_import = started.elapsed();

        let started = Instant::now();
        for start in (0..ROWS).step_by(SCAN_BENCHMARK_BATCH_SIZE) {
            let rows = generated_scan_rows(
                start,
                (start + SCAN_BENCHMARK_BATCH_SIZE).min(ROWS),
                1,
                1024,
            );
            let books = scan_row_refs(root.id, &rows);
            assert!(database
                .upsert_scanned_books(&books)
                .unwrap()
                .iter()
                .all(|result| !result.changed));
        }
        let unchanged_rescan = started.elapsed();

        let started = Instant::now();
        let rows = generated_scan_rows(0, 100, 2, 2048);
        let books = scan_row_refs(root.id, &rows);
        assert!(database
            .upsert_scanned_books(&books)
            .unwrap()
            .iter()
            .all(|result| result.changed));
        let changed_rescan_100 = started.elapsed();

        let started = Instant::now();
        {
            let mut connection = database.connection.lock().unwrap();
            rebuild_search_index(&mut connection, None, |_| {}).unwrap();
        }
        let search_rebuild = started.elapsed();
        drop(database);

        let started = Instant::now();
        let reopened = Database::open(&catalog).unwrap();
        let startup = started.elapsed();
        assert_eq!(reopened.library_roots().unwrap()[0].book_count, ROWS as i64);

        println!("PERF_BASELINE rows={ROWS} initial_import_ms={:.3} unchanged_rescan_ms={:.3} changed_rescan_100_ms={:.3} search_rebuild_ms={:.3} startup_ms={:.3}",
            initial_import.as_secs_f64() * 1000.0,
            unchanged_rescan.as_secs_f64() * 1000.0,
            changed_rescan_100.as_secs_f64() * 1000.0,
            search_rebuild.as_secs_f64() * 1000.0,
            startup.as_secs_f64() * 1000.0,
        );
    }

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
        assert_eq!(version, 18);
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
    fn resume_state_survives_rescan_reopen_backup_and_completion() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("catalog.sqlite3");
        let database = Database::open(&path).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: "missing-source",
                display_name: "Books",
            })
            .unwrap();
        let new_book = NewBook {
            library_root_id: root.id,
            file_path: "missing-source/book.epub",
            parent_folder_path: "missing-source",
            file_name: "物語.epub",
            file_size: 42,
            modified_time: 1,
        };
        let book = database.add_book(new_book.clone()).unwrap();
        assert!(database.resume_books(false).unwrap().is_empty());
        assert!(!database.record_reader_open(book.id).unwrap());
        database
            .save_reading_location(book.id, "epubcfi(/6/4!/4/2/1:0)")
            .unwrap();
        database.upsert_scanned_book(new_book).unwrap();
        assert!(database.resume_books(false).unwrap()[0].has_location);
        database.set_reader_finished(book.id, true).unwrap();
        assert!(database.record_reader_open(book.id).unwrap());
        assert!(database.resume_books(false).unwrap().is_empty());
        let backup = directory.path().join("backup.sqlite3");
        database.backup_to(&backup).unwrap();
        database.set_reader_finished(book.id, false).unwrap();
        database.restore_from(&backup).unwrap();
        assert!(database.resume_books(false).unwrap().is_empty());
        database.set_reader_finished(book.id, false).unwrap();
        drop(database);
        let database = Database::open(&path).unwrap();
        assert_eq!(database.resume_books(false).unwrap()[0].title, "物語.epub");
        database
            .connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE books SET extraction_status='unavailable' WHERE id=?1",
                [book.id],
            )
            .unwrap();
        assert!(!database.resume_books(false).unwrap()[0].is_available);
        assert!(database.resume_books(true).unwrap().is_empty());
        assert!(database.reading_location(book.id).unwrap().is_some());
        let location = database.reading_location(book.id).unwrap();
        database.remove_resume_book(book.id).unwrap();
        database.remove_resume_book(book.id).unwrap();
        assert!(database.resume_books(false).unwrap().is_empty());
        assert!(database.resume_books(true).unwrap().is_empty());
        assert_eq!(database.reading_location(book.id).unwrap(), location);
        let status: String = database
            .connection
            .lock()
            .unwrap()
            .query_row(
                "SELECT reading_status FROM books WHERE id=?1",
                [book.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "reading");
        drop(database);
        let database = Database::open(&path).unwrap();
        assert!(database.resume_books(false).unwrap().is_empty());
        assert_eq!(database.reading_location(book.id).unwrap(), location);
        database.record_reader_open(book.id).unwrap();
        assert_eq!(database.resume_books(false).unwrap()[0].id, book.id);
        assert_eq!(database.reading_location(book.id).unwrap(), location);
        assert!(!directory.path().join("missing-source").exists());
    }

    #[test]
    #[ignore = "generated 100,000-row resume benchmark"]
    fn resume_large_catalog_benchmark() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: "fixture",
                display_name: "Fixture",
            })
            .unwrap();
        {
            let mut connection = database.connection.lock().unwrap();
            let transaction = connection.transaction().unwrap();
            transaction.execute("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000) INSERT INTO books(library_root_id,file_path,parent_folder_path,file_name,file_size,modified_time,created_at,updated_at) SELECT ?1,'fixture/'||x||'.epub','fixture',x||'.epub',1,1,1,1 FROM n", [root.id]).unwrap();
            transaction
                .execute(
                    "INSERT INTO reader_resume(book_id,last_read_at) SELECT id,id FROM books",
                    [],
                )
                .unwrap();
            transaction
                .execute(
                    "UPDATE books SET extraction_status='unavailable' WHERE id>99980",
                    [],
                )
                .unwrap();
            transaction.commit().unwrap();
            let plan: String = connection.query_row("EXPLAIN QUERY PLAN SELECT book_id FROM reader_resume WHERE finished=0 ORDER BY last_read_at DESC,book_id DESC LIMIT 12", [], |row| row.get(3)).unwrap();
            assert!(plan.contains("idx_reader_resume_recent"), "{plan}");
            eprintln!("resume query plan: {plan}");
        }
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(database.resume_books(false).unwrap().len(), 12);
            assert_eq!(database.resume_books(true).unwrap()[0].id, 99980);
        }
        eprintln!(
            "resume 100k rows, 100 home query pairs: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn reader_location_persists_in_the_catalog_only() {
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
                file_path: r"F:\\books\\reader.epub",
                parent_folder_path: r"F:\\books",
                file_name: "reader.epub",
                file_size: 42,
                modified_time: 123,
            })
            .unwrap();
        database
            .save_reading_location(book.id, "epubcfi(/6/4!/4/2/1:0)")
            .unwrap();
        assert_eq!(
            database.reading_location(book.id).unwrap().as_deref(),
            Some("epubcfi(/6/4!/4/2/1:0)")
        );
        assert_eq!(
            database.reader_book(book.id).unwrap().unwrap().file_path,
            book.file_path
        );
    }

    #[test]
    fn dictionary_entries_are_local_and_can_be_disabled() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        database
            .replace_jmdict(
                "C:\\JMdict.xml",
                &[crate::services::dictionary::ImportedEntry {
                    term: "猫".into(),
                    reading: Some("ねこ".into()),
                    definitions: vec!["cat".into()],
                    part_of_speech: vec!["noun".into()],
                }],
            )
            .unwrap();
        assert_eq!(
            database.dictionary_lookup("猫").unwrap()[0].definitions,
            vec!["cat"]
        );
        let mut dictionaries = database.dictionaries().unwrap();
        let dictionary = dictionaries.remove(0);
        database
            .set_dictionary_enabled(dictionary.id, false)
            .unwrap();
        assert!(database.dictionary_lookup("猫").unwrap().is_empty());
    }

    #[test]
    fn dictionary_phase1_corpus_preserves_legacy_results() {
        use tmw_japanese_core::lookup::{lookup_text, LookupRequest};
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let forms = [
            ("学校", "がっこう"), ("学校生活", "がっこうせいかつ"),
            ("読む", "よむ"), ("食べる", "たべる"), ("高い", "たかい"),
            ("ネコ", "ねこ"), ("猫", "ねこ"), ("生物", "せいぶつ"),
            ("生物", "なまもの"), ("漢字", "かんじ"),
        ];
        let source = serde_json::json!({ "words": forms.iter().map(|(term, reading)| {
            serde_json::json!({ "kanji": [{"text": term}], "kana": [{"text": reading}],
                "sense": [{"gloss": [{"lang": "eng", "text": "original synthetic fixture"}]}] })
        }).collect::<Vec<_>>() });
        let path = directory.path().join("generated-jmdict.json");
        std::fs::write(&path, serde_json::to_vec(&source).unwrap()).unwrap();
        let started = std::time::Instant::now();
        let entries = crate::services::dictionary::import_jmdict(&path).unwrap();
        database.replace_jmdict("generated fixture", &entries).unwrap();
        println!("Synthetic import: {} forms, {:.2} ms including transactional insert", entries.len(), started.elapsed().as_secs_f64() * 1000.0);
        let corpus: serde_json::Value = serde_json::from_str(include_str!("../../../docs/dictionary-comparison-corpus.json")).unwrap();
        for row in corpus["cases"].as_array().unwrap() {
            let request = LookupRequest { text: row["text"].as_str().unwrap().into(), offset: row["offset"].as_u64().unwrap() as usize };
            let target = crate::services::readings::dictionary_target(&request.text, request.offset).unwrap();
            // Independent pre-extraction sequence is the compatibility oracle.
            let mut legacy = database.dictionary_lookup(&target.surface).unwrap();
            if legacy.is_empty() && target.lemma != target.surface {
                legacy = database.dictionary_lookup(&target.lemma).unwrap();
            }
            if legacy.is_empty() {
                if let Some(reading) = &target.reading { legacy = database.dictionary_lookup(reading).unwrap(); }
            }
            let result = lookup_text(&request, &mut |value: &str| database.dictionary_lookup(value).map_err(|e| e.to_string())).unwrap();
            assert_eq!(serde_json::to_value(&result.entries).unwrap(), serde_json::to_value(&legacy).unwrap(), "{}", row["id"]);
            let surface: String = request.text.chars().skip(result.matched_span.start).take(result.matched_span.end - result.matched_span.start).collect();
            assert_eq!(surface, result.target.surface);
            println!("{}", serde_json::json!({"id": row["id"], "surface": result.target.surface,
                "lemma": result.target.lemma, "span": [result.matched_span.start, result.matched_span.end],
                "ranking": result.entries.iter().map(|e| (&e.term, &e.reading)).collect::<Vec<_>>() }));
        }
        let dictionary = database.dictionaries().unwrap().remove(0);
        database.set_dictionary_enabled(dictionary.id, false).unwrap();
        let result = lookup_text(&LookupRequest { text: "猫".into(), offset: 0 }, &mut |value: &str| database.dictionary_lookup(value).map_err(|e| e.to_string())).unwrap();
        assert!(result.entries.is_empty());
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
        assert!(database.upsert_scanned_book(book.clone()).unwrap().changed);
        assert!(!database.upsert_scanned_book(book.clone()).unwrap().changed);
        assert!(
            database
                .upsert_scanned_book(NewBook {
                    file_size: 43,
                    modified_time: 124,
                    ..book
                })
                .unwrap()
                .changed
        );
        assert_eq!(database.library_roots().unwrap()[0].book_count, 1);
        assert!(database.remove_library_root(root.id).unwrap());
        assert!(database.book(1).unwrap().is_none());
    }

    #[test]
    fn scan_batches_return_ids_and_index_only_final_metadata() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        database.begin_scan(root.id).unwrap();
        let books = [
            NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\one.epub",
                parent_folder_path: r"F:\\books",
                file_name: "one.epub",
                file_size: 1,
                modified_time: 1,
            },
            NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\two.epub",
                parent_folder_path: r"F:\\books",
                file_name: "two.epub",
                file_size: 2,
                modified_time: 1,
            },
        ];
        let upserts = database.upsert_scanned_books(&books).unwrap();
        assert!(upserts.iter().all(|result| result.changed));
        assert_ne!(upserts[0].book_id, upserts[1].book_id);
        {
            let connection = database.connection.lock().unwrap();
            let documents: i64 = connection
                .query_row("SELECT COUNT(*) FROM book_search_documents", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(
                documents, 0,
                "discovery must not build a provisional search document"
            );
        }
        let first = ExtractedBookMetadata {
            title: Some("最終タイトル一".into()),
            ..Default::default()
        };
        let second = ExtractedBookMetadata {
            title: Some("最終タイトル二".into()),
            ..Default::default()
        };
        database
            .save_extracted_metadata_batch(&[
                (upserts[0].book_id, &first),
                (upserts[1].book_id, &second),
            ])
            .unwrap();
        {
            let connection = database.connection.lock().unwrap();
            let documents: i64 = connection
                .query_row("SELECT COUNT(*) FROM book_search_documents", [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(documents, 2);
        }
        assert!(database
            .upsert_scanned_books(&books)
            .unwrap()
            .iter()
            .all(|result| !result.changed));
        assert_eq!(database.reconcile_completed_scan(root.id).unwrap(), 0);
    }

    #[test]
    fn metadata_batch_failure_rolls_back_every_book_and_search_document() {
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
                file_path: r"F:\\books\\rollback.epub",
                parent_folder_path: r"F:\\books",
                file_name: "rollback.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let metadata = ExtractedBookMetadata {
            title: Some("Must be rolled back".into()),
            ..Default::default()
        };

        // The missing second id makes search-document refresh fail after the
        // first row was updated. Dropping the transaction must undo both the
        // metadata update and any derived-index work.
        assert!(database
            .save_extracted_metadata_batch(&[(book.id, &metadata), (i64::MAX, &metadata)])
            .is_err());

        let restored = database.book(book.id).unwrap().unwrap();
        assert!(restored.discovered_title.is_none());
        assert_eq!(restored.extraction_status, "pending");
        let connection = database.connection.lock().unwrap();
        let indexed_title: String = connection
            .query_row(
                "SELECT title_normalized FROM book_search_documents WHERE book_id=?1",
                [book.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed_title, "rollback");
    }

    #[test]
    fn abandoning_a_partial_scan_never_marks_unseen_books_unavailable() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        for name in ["seen.epub", "not-reached.epub"] {
            database
                .add_book(NewBook {
                    library_root_id: root.id,
                    file_path: &format!(r"F:\\books\\{name}"),
                    parent_folder_path: r"F:\\books",
                    file_name: name,
                    file_size: 1,
                    modified_time: 1,
                })
                .unwrap();
        }

        database.begin_scan(root.id).unwrap();
        database
            .upsert_scanned_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\seen.epub",
                parent_folder_path: r"F:\\books",
                file_name: "seen.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        // Cancellation deliberately does not call reconcile_completed_scan.
        database.begin_scan(root.id).unwrap();

        assert!(database
            .book_by_path(r"F:\\books\\not-reached.epub")
            .unwrap()
            .is_some_and(|book| book.extraction_status != "unavailable"));
    }

    #[test]
    fn missing_books_keep_catalog_data_and_recover_on_rescan() {
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
            file_path: r"F:\\books\\returns.epub",
            parent_folder_path: r"F:\\books",
            file_name: "returns.epub",
            file_size: 42,
            modified_time: 123,
        };
        assert!(database.upsert_scanned_book(book.clone()).unwrap().changed);
        database
            .save_book_overrides(
                1,
                &BookOverride {
                    title: Some("Keep me".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(database.mark_missing_books(root.id, &[]).unwrap(), 1);
        assert_eq!(
            database.book(1).unwrap().unwrap().extraction_status,
            "unavailable"
        );
        assert!(database.upsert_scanned_book(book).unwrap().changed);
        assert_eq!(
            database.book_details(1).unwrap().unwrap().effective_title,
            "Keep me"
        );
    }

    #[test]
    fn catalog_backup_restores_overrides_without_source_files_or_cover_bytes() {
        let directory = tempdir().unwrap();
        let catalog = directory.path().join("catalog.sqlite3");
        let backup = directory.path().join("backup.sqlite3");
        let database = Database::open(&catalog).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\book.epub",
                parent_folder_path: r"F:\\books",
                file_name: "book.epub",
                file_size: 7,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_book_overrides(
                book.id,
                &BookOverride {
                    title: Some("Backed up".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        database.backup_to(&backup).unwrap();
        database.reset_all_book_overrides(book.id).unwrap();
        database.restore_from(&backup).unwrap();
        assert_eq!(
            database
                .book_details(book.id)
                .unwrap()
                .unwrap()
                .effective_title,
            "Backed up"
        );
        assert!(!directory.path().join("book.epub").exists());
    }

    #[test]
    fn search_matches_normalized_japanese_substrings_and_ranks_titles() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let title_book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\a.epub",
                parent_folder_path: r"F:\\books",
                file_name: "a.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_extracted_metadata(
                title_book.id,
                &ExtractedBookMetadata {
                    title: Some("巨人１巻".into()),
                    creator: Some("著者".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let file_book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\巨人のメモ.epub",
                parent_folder_path: r"F:\\books",
                file_name: "巨人のメモ.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let request = |query: &str| BrowseBooksRequest {
            reading_status: None,
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: false, duplicate_filtering: None,
            query: query.into(),
            sort: "title".into(),
            offset: 0,
            limit: 20,
        };
        let results = database.browse_books(&request("巨人1巻")).unwrap();
        assert_eq!(results[0].id, title_book.id);
        assert_eq!(results[0].effective_title, "巨人１巻");
        assert!(database
            .browse_books(&request("巨人"))
            .unwrap()
            .iter()
            .any(|book| book.id == file_book.id));
    }

    #[test]
    fn romaji_readings_rank_before_filenames() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let title = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\a.epub",
                parent_folder_path: r"F:\\books",
                file_name: "a.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_extracted_metadata(
                title.id,
                &ExtractedBookMetadata {
                    title: Some("進撃の巨人".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let other = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\shingeki no kyojin.epub",
                parent_folder_path: r"F:\\books",
                file_name: "shingeki no kyojin.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let request = |query: &str| BrowseBooksRequest {
            reading_status: None,
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: false, duplicate_filtering: None,
            query: query.into(),
            sort: "title".into(),
            offset: 0,
            limit: 20,
        };
        assert_eq!(
            database
                .browse_books(&request("shingeki no kyojin"))
                .unwrap()[0]
                .id,
            title.id
        );
        assert_ne!(other.id, title.id);
    }

    #[test]
    fn romaji_spacing_matches_existing_and_rebuilt_indexes() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database.add_library_root(NewLibraryRoot { path: r"C:\fixture", display_name: "Fixture" }).unwrap();
        for (index, title) in ["好きな子のいもうと", "好きな子のいもうと２", "好きな子のいもうと３"].iter().enumerate() {
            let name = format!("fixture{index}.epub");
            let book = database.add_book(NewBook { library_root_id: root.id, file_path: &format!(r"C:\fixture\{name}"), parent_folder_path: r"C:\fixture", file_name: &name, file_size: 1, modified_time: 1 }).unwrap();
            database.save_extracted_metadata(book.id, &ExtractedBookMetadata { title: Some((*title).into()), ..Default::default() }).unwrap();
            database.save_reading_override(book.id, Some("すき な こ の いもうと"), None).unwrap();
        }
        let check = || {
            for query in ["sukinako", "suki na ko", "su ki na ko", "SUKI-NA_KO", "su", "好きな子", "sukinakonoimouto"] {
                let request = BrowseBooksRequest { reading_status: None, library_root_id: None, tag_id: None, collection_id: None, needs_metadata: false, hide_duplicate_titles: false, duplicate_filtering: None, query: query.into(), sort: "title".into(), offset: 0, limit: 20 };
                assert_eq!(database.browse_books(&request).unwrap().len(), 3, "{query}");
            }
        };
        check();
        // Emulate a version-18 index and check the one-time backfill.
        {
            let mut connection = database.connection.lock().unwrap();
            connection.execute("UPDATE book_search_fts SET title_romaji='suki na ko no imouto'", []).unwrap();
            connection.pragma_update(None, "user_version", 18).unwrap();
            run_migrations(&mut connection).unwrap();
        }
        check();
        assert!(database.rebuild_search_index(&AtomicBool::new(false), |_| {}).unwrap());
        check();
    }

    #[test]
    fn duplicate_title_filter_keeps_one_normalized_effective_title_and_honors_overrides() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let mut ids = Vec::new();
        for (index, title) in [
            "同じ　本10",
            "同じ 本１０",
            "同じ 本10【書店特典】",
            "別の本",
        ]
        .into_iter()
        .enumerate()
        {
            let file_name = format!("{index}.epub");
            let file_path = format!(r"F:\\books\\{file_name}");
            let book = database
                .add_book(NewBook {
                    library_root_id: root.id,
                    file_path: &file_path,
                    parent_folder_path: r"F:\\books",
                    file_name: &file_name,
                    file_size: 1,
                    modified_time: 1,
                })
                .unwrap();
            database
                .save_extracted_metadata(
                    book.id,
                    &ExtractedBookMetadata {
                        title: Some(title.into()),
                        ..Default::default()
                    },
                )
                .unwrap();
            ids.push(book.id);
        }
        let request = BrowseBooksRequest {
            reading_status: None,
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: true, duplicate_filtering: None,
            query: String::new(),
            sort: "folder".into(),
            offset: 0,
            limit: 20,
        };

        let before = database.browse_books(&request).unwrap();
        assert_eq!(
            before.iter().map(|book| book.id).collect::<Vec<_>>(),
            vec![ids[0], ids[2], ids[3]]
        );

        database
            .save_book_overrides(
                ids[2],
                &BookOverride {
                    title: Some("同じ 本10".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let after = database.browse_books(&request).unwrap();
        assert_eq!(
            after.iter().map(|book| book.id).collect::<Vec<_>>(),
            vec![ids[0], ids[3]]
        );
        // These generated paths are absent from the JSON: exact-title hiding
        // must still apply with either Settings confidence level.
        let mut request = request;
        for mode in ["high", "all"] {
            request.duplicate_filtering = Some(mode.into());
            assert_eq!(database.browse_books(&request).unwrap().iter().map(|book| book.id).collect::<Vec<_>>(), vec![ids[0], ids[3]]);
        }
        request.duplicate_filtering = Some("off".into());
        assert_eq!(database.browse_books(&request).unwrap().len(), 4);
        // A candidate group's available survivor must not be suppressed by
        // its older unavailable exact-title twin.
        {
            let connection = database.connection.lock().unwrap();
            connection.execute("INSERT INTO duplicate_candidates SELECT file_path,99999,1 FROM books WHERE id IN (?1,?2)", params![ids[0], ids[1]]).unwrap();
            connection.execute("UPDATE books SET extraction_status='unavailable' WHERE id=?1", [ids[0]]).unwrap();
        }
        request.duplicate_filtering = Some("high".into());
        assert_eq!(database.browse_books(&request).unwrap().iter().map(|book| book.id).collect::<Vec<_>>(), vec![ids[1], ids[3]]);
    }

    #[test]
    fn duplicate_title_filter_uses_the_index_for_an_empty_search() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        seed_query_fixture(&database, 10_000);

        let connection = database.connection.lock().unwrap();
        let plan = connection
            .prepare(
                "EXPLAIN QUERY PLAN SELECT b.id FROM books b \
                 LEFT JOIN book_search_documents d ON d.book_id=b.id \
                 WHERE d.title_normalized='' OR NOT EXISTS (\
                   SELECT 1 FROM book_search_documents earlier \
                   WHERE earlier.book_id<b.id \
                     AND lower(earlier.title_normalized)=lower(d.title_normalized)\
                 ) LIMIT 80",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert!(
            plan.iter().any(|detail| {
                detail.contains("idx_book_search_duplicate_title")
                    && detail.contains("<expr>=?")
                    && detail.contains("book_id<?")
            }),
            "duplicate lookup did not use the compound expression index: {plan:?}"
        );
    }

    /// Repeatable regression benchmark for both the initial duplicate-title
    /// page and subsequent infinite-scroll pages. Uses generated rows only.
    #[test]
    #[ignore = "performance benchmark; run explicitly with --ignored --nocapture"]
    fn performance_duplicate_title_pages_100000_rows() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        seed_query_fixture(&database, 100_000);
        let mut request = BrowseBooksRequest {
            reading_status: None,
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: true, duplicate_filtering: None,
            query: String::new(),
            sort: "title".into(),
            offset: 0,
            limit: 80,
        };

        for offset in [0, 80, 4_000, 40_000] {
            request.offset = offset;
            let started = Instant::now();
            let rows = database.browse_books(&request).unwrap();
            println!(
                "PERF_DUPLICATE_TITLE rows=100000 offset={offset} result_rows={} duration_ms={:.3}",
                rows.len(),
                started.elapsed().as_secs_f64() * 1_000.0
            );
        }
    }

    #[test]
    fn startup_keeps_complete_index_and_repairs_an_incomplete_one() {
        let directory = tempdir().unwrap();
        let catalog = directory.path().join("catalog.sqlite3");
        let database = Database::open(&catalog).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let book = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\indexed.epub",
                parent_folder_path: r"F:\\books",
                file_name: "indexed.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        {
            let connection = database.connection.lock().unwrap();
            connection.execute("UPDATE book_search_documents SET title_normalized='startup-sentinel' WHERE book_id=?1", [book.id]).unwrap();
        }
        drop(database);

        let reopened = Database::open(&catalog).unwrap();
        {
            let connection = reopened.connection.lock().unwrap();
            let value: String = connection
                .query_row(
                    "SELECT title_normalized FROM book_search_documents WHERE book_id=?1",
                    [book.id],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(
                value, "startup-sentinel",
                "a complete index must not be rewritten at startup"
            );
            connection
                .execute(
                    "DELETE FROM book_search_documents WHERE book_id=?1",
                    [book.id],
                )
                .unwrap();
        }
        drop(reopened);

        let repaired = Database::open(&catalog).unwrap();
        let connection = repaired.connection.lock().unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM book_search_documents WHERE book_id=?1",
                [book.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "startup must repair an incomplete derived index");
    }

    #[test]
    fn manual_rebuild_is_cancellable_and_reading_aliases_update_immediately() {
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
                file_path: r"F:\\books\\alias.epub",
                parent_folder_path: r"F:\\books",
                file_name: "alias.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_reading_override(book.id, Some("しんげき"), Some("attack titan"))
            .unwrap();
        let request = |query: &str| BrowseBooksRequest {
            reading_status: None,
            library_root_id: None,
            tag_id: None,
            collection_id: None,
            needs_metadata: false,
            hide_duplicate_titles: false, duplicate_filtering: None,
            query: query.into(),
            sort: "title".into(),
            offset: 0,
            limit: 20,
        };
        assert_eq!(
            database.browse_books(&request("shingeki")).unwrap()[0].id,
            book.id
        );
        assert_eq!(
            database.browse_books(&request("attack titan")).unwrap()[0].id,
            book.id
        );

        let cancelled = AtomicBool::new(true);
        assert!(!database.rebuild_search_index(&cancelled, |_| {}).unwrap());
        assert_eq!(
            database.browse_books(&request("attack titan")).unwrap()[0].id,
            book.id,
            "cancelled rebuild must roll back atomically"
        );
        let before: Vec<String> = {
            let connection = database.connection.lock().unwrap();
            let mut statement = connection.prepare("SELECT title_normalized || '|' || title_reading || '|' || aliases_normalized || '|' || aliases_romaji FROM book_search_documents ORDER BY book_id").unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };
        let active = AtomicBool::new(false);
        assert!(database.rebuild_search_index(&active, |_| {}).unwrap());
        let after: Vec<String> = {
            let connection = database.connection.lock().unwrap();
            let mut statement = connection.prepare("SELECT title_normalized || '|' || title_reading || '|' || aliases_normalized || '|' || aliases_romaji FROM book_search_documents ORDER BY book_id").unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<rusqlite::Result<_>>()
                .unwrap()
        };
        assert_eq!(
            after, before,
            "forced rebuild must equal incremental indexing"
        );
    }

    #[test]
    fn overrides_and_tags_persist_and_refresh_effective_metadata() {
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
                file_path: r"F:\\books\\old.epub",
                parent_folder_path: r"F:\\books",
                file_name: "old.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        database
            .save_extracted_metadata(
                book.id,
                &ExtractedBookMetadata {
                    title: Some("Discovered".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        database
            .save_book_overrides(
                book.id,
                &BookOverride {
                    title: Some("Corrected".into()),
                    notes: Some("Personal note".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        database
            .replace_book_tags(book.id, &["読書中".into(), "漫画".into()])
            .unwrap();
        let details = database.book_details(book.id).unwrap().unwrap();
        assert_eq!(details.effective_title, "Corrected");
        assert_eq!(details.tags.len(), 2);
        database.reset_book_override(book.id, "title").unwrap();
        assert_eq!(
            database
                .book_details(book.id)
                .unwrap()
                .unwrap()
                .effective_title,
            "Discovered"
        );
    }

    #[test]
    fn folder_group_sorts_volume_suggestions_and_actions_are_explicit() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("catalog.sqlite3")).unwrap();
        let root = database
            .add_library_root(NewLibraryRoot {
                path: r"F:\\books",
                display_name: "Books",
            })
            .unwrap();
        let second = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\series\\two.epub",
                parent_folder_path: r"F:\\books\\series",
                file_name: "two ２巻.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let first = database
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: r"F:\\books\\series\\one.epub",
                parent_folder_path: r"F:\\books\\series",
                file_name: "one １巻.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let group = database.folder_group(second.id).unwrap().unwrap();
        assert_eq!(
            group.books.iter().map(|book| book.id).collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
        database
            .assign_series(&AssignSeriesRequest {
                series_name: "Chosen series".into(),
                book_ids: vec![first.id, second.id],
            })
            .unwrap();
        assert_eq!(
            database
                .book_details(first.id)
                .unwrap()
                .unwrap()
                .effective_series,
            "Chosen series"
        );
        let collection = database
            .create_collection(&CreateCollectionRequest {
                name: "My set".into(),
                book_ids: vec![first.id, second.id],
            })
            .unwrap();
        assert!(database
            .collections()
            .unwrap()
            .iter()
            .any(|(id, name)| *id == collection && name == "My set"));
    }
}
