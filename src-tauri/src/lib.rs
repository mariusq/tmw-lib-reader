mod dictionary_import;
pub mod companion;
pub mod db;
pub mod models;
pub mod services;

use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    models::book::{
        AssignSeriesRequest, BatchTagRequest, BookDetails, BookOverride, BrowseBooksRequest,
        BrowserBook, CreateCollectionRequest, FolderGroup,
        ReaderBook, ResumeBook, SavePassageRequest, SavedPassage,
    },
    models::library_root::{LibraryRoot, LibraryRootSummary, NewLibraryRoot},
    services::scanner::ScanController,
};

#[derive(Default)]
struct IndexRebuildController(Mutex<Option<(String, Arc<AtomicBool>)>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IndexRebuildProgress {
    rebuild_id: String,
    completed: usize,
    total: usize,
}

#[tauri::command]
fn choose_library_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Select an EPUB library folder")
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

#[tauri::command]
fn add_library_root(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
    path: String,
) -> Result<LibraryRoot, String> {
    let directory = std::path::Path::new(&path);
    if !directory.is_dir() {
        return Err("The selected path is not an accessible folder.".into());
    }
    let source = directory.canonicalize().map_err(|e| e.to_string())?;
    let dictionary_root = app.path().app_data_dir().map_err(|e| e.to_string())?.join("yomitan-v1");
    let dictionary_root = if dictionary_root.exists() { dictionary_root.canonicalize().map_err(|e| e.to_string())? } else { dictionary_root };
    if dictionary_root.starts_with(&source) || source.starts_with(&dictionary_root) {
        return Err("A library folder must be separate from app-managed dictionary storage.".into());
    }
    let cache = cover_cache_directory(&app, &database)?;
    if cache.starts_with(directory) {
        return Err(
            "A library root cannot contain the cover cache. Choose a separate cache folder first."
                .into(),
        );
    }
    let display_name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(&path);
    let root = match database.add_library_root(NewLibraryRoot {
        path: &path,
        display_name,
    }) {
        Ok(root) => root,
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            database
                .library_roots()
                .map_err(|error| error.to_string())?
                .into_iter()
                .find(|summary| summary.root.path == path)
                .map(|summary| summary.root)
                .ok_or_else(|| "This library root already exists.".to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    services::scanner::start_scan(
        app,
        root.id,
        root.path.clone(),
        cache,
        format!("add-{}", root.id),
    )?;
    Ok(root)
}

#[tauri::command]
fn list_library_roots(
    database: State<'_, db::Database>,
) -> Result<Vec<LibraryRootSummary>, String> {
    database.library_roots().map_err(|error| error.to_string())
}

#[tauri::command]
fn rescan_library_root(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
    root_id: i64,
    scan_id: String,
) -> Result<(), String> {
    let root = database
        .library_root(root_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Library root not found.".to_string())?;
    let cache = cover_cache_directory(&app, &database)?;
    services::scanner::start_scan(app, root.id, root.path, cache, scan_id)
}

const COVER_CACHE_SETTING: &str = "cover_cache_directory";

fn default_cover_cache_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("covers"))
        .map_err(|error| error.to_string())
}

fn cover_cache_directory(
    app: &tauri::AppHandle,
    database: &db::Database,
) -> Result<PathBuf, String> {
    Ok(database
        .setting(COVER_CACHE_SETTING)
        .map_err(|error| error.to_string())?
        .map(PathBuf::from)
        .unwrap_or(default_cover_cache_directory(app)?))
}

#[tauri::command]
fn choose_cover_cache_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Select a separate folder for EPUB cover cache")
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

#[tauri::command]
fn get_cover_cache_directory(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
) -> Result<String, String> {
    Ok(cover_cache_directory(&app, &database)?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
fn set_cover_cache_directory(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
    path: String,
) -> Result<(), String> {
    let cache = PathBuf::from(&path);
    if !cache.is_dir() {
        return Err("The cover cache location must be an accessible folder.".into());
    }
    for root in database
        .library_root_paths()
        .map_err(|error| error.to_string())?
    {
        let root = PathBuf::from(root);
        if cache.starts_with(&root) || root.starts_with(&cache) {
            return Err("The cover cache must be separate from every library root; it cannot contain or be contained by one.".into());
        }
    }
    let _ = app; // The selected path is persisted as a user preference, not created in a source root.
    database
        .set_setting(COVER_CACHE_SETTING, &path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_scan(scans: State<'_, ScanController>, scan_id: String) -> bool {
    scans.cancel(&scan_id)
}

#[tauri::command]
async fn rebuild_search_index(
    app: tauri::AppHandle,
    controller: State<'_, IndexRebuildController>,
    rebuild_id: String,
) -> Result<bool, String> {
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = controller.0.lock().expect("index rebuild mutex poisoned");
        if active.is_some() {
            return Err("A search-index rebuild is already running.".into());
        }
        *active = Some((rebuild_id.clone(), cancelled.clone()));
    }
    // Synchronous Tauri commands run on the window thread. Keep the entire
    // transaction (including tokenizer work) off it so progress and Cancel work.
    let worker_app = app.clone();
    let worker_id = rebuild_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        worker_app
            .state::<db::Database>()
            .rebuild_search_index(&cancelled, |(completed, total)| {
                let _ = worker_app.emit(
                    "search-index-progress",
                    IndexRebuildProgress {
                        rebuild_id: worker_id.clone(),
                        completed,
                        total,
                    },
                );
            })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("Search-index worker failed: {error}"))
    .and_then(|result| result);
    controller
        .0
        .lock()
        .expect("index rebuild mutex poisoned")
        .take();
    match result {
        Ok(completed) => {
            let _ = app.emit(
                "search-index-completed",
                IndexRebuildProgress {
                    rebuild_id,
                    completed: usize::from(completed),
                    total: usize::from(completed),
                },
            );
            Ok(completed)
        }
        Err(error) => Err(error),
    }
}

#[tauri::command]
fn cancel_search_index_rebuild(
    controller: State<'_, IndexRebuildController>,
    rebuild_id: String,
) -> bool {
    let active = controller.0.lock().expect("index rebuild mutex poisoned");
    if let Some((id, flag)) = active.as_ref().filter(|(id, _)| id == &rebuild_id) {
        let _ = id;
        flag.store(true, Ordering::Relaxed);
        true
    } else {
        false
    }
}

#[tauri::command]
fn remove_library_root(database: State<'_, db::Database>, root_id: i64) -> Result<bool, String> {
    database
        .remove_library_root(root_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn delete_book(app: tauri::AppHandle, book_id: i64, expected_path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let database = app.state::<db::Database>();
        app.state::<ScanController>().while_idle(|| {
            let details = database.book_details(book_id).map_err(|e| e.to_string())?
                .ok_or("This book is no longer in the catalog.")?;
            let source = PathBuf::from(&details.book.file_path);
            if details.book.file_path != expected_path {
                return Err("The book path changed. Reopen Book Details before deleting.".into());
            }
            let root = database.library_root(details.book.library_root_id).map_err(|e| e.to_string())?
                .ok_or("The library root is unavailable.")?;
            let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
            let canonical_data = app_data.canonicalize().map_err(|e| e.to_string())?;
            for library in database.library_roots().map_err(|e| e.to_string())? {
                if let Ok(library_path) = PathBuf::from(library.root.path).canonicalize() {
                    if canonical_data.starts_with(library_path) {
                        return Err("Recovery backups cannot be stored inside a source library.".into());
                    }
                }
            }
            let backup = app_data
                .join("backups").join(format!("deleted-book-{book_id}-{}", std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos()));
            fs::create_dir_all(&backup).map_err(|e| e.to_string())?;
            database.backup_to(&backup.join("catalog.sqlite3")).map_err(|e| e.to_string())?;
            match fs::symlink_metadata(&source) {
                Ok(metadata) => {
                    if !metadata.is_file() || metadata.file_type().is_symlink()
                        || !source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("epub")) {
                        return Err("Deletion requires a regular EPUB file, not a directory or link.".into());
                    }
                    let canonical = source.canonicalize().map_err(|e| e.to_string())?;
                    let canonical_root = PathBuf::from(root.path).canonicalize().map_err(|e| e.to_string())?;
                    if !canonical.starts_with(&canonical_root) {
                        return Err("The EPUB resolves outside its library root.".into());
                    }
                    let backup_file = backup.join("book.epub");
                    fs::copy(&source, &backup_file).map_err(|e| format!("Could not back up EPUB: {e}"))?;
                    fs::File::open(&backup_file).and_then(|file| file.sync_all()).map_err(|e| e.to_string())?;
                    fs::remove_file(&source).map_err(|e| format!("Could not delete EPUB: {e}"))?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            database.remove_book(book_id).map_err(|e| format!("Catalog removal failed: {e}. Backup: {}", backup.display()))?;
            Ok(backup.display().to_string())
        })
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn export_catalog_backup(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
) -> Result<Option<String>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Export catalog backup")
        .set_file_name("tmw-catalog-backup.sqlite3")
        .add_filter("SQLite catalog", &["sqlite3"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|_| {
        "The selected backup destination is not a local filesystem path.".to_string()
    })?;
    if path.exists() {
        fs::remove_file(&path)
            .map_err(|e| format!("Could not replace the selected backup: {e}"))?;
    }
    database.backup_to(&path).map_err(|e| e.to_string())?;
    services::logging::event(
        "info",
        "catalog_backup_exported",
        &[("path", path.display().to_string())],
    );
    Ok(Some(path.to_string_lossy().into_owned()))
}

#[tauri::command]
fn import_catalog_backup(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
) -> Result<Option<String>, String> {
    let Some(source) = app
        .dialog()
        .file()
        .set_title("Restore catalog backup")
        .add_filter("SQLite catalog", &["sqlite3"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let source = source
        .into_path()
        .map_err(|_| "The selected backup is not a local filesystem path.".to_string())?;
    let safety_directory = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("backups");
    fs::create_dir_all(&safety_directory).map_err(|e| e.to_string())?;
    let safety = safety_directory.join(format!(
        "pre-restore-{}.sqlite3",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));
    database
        .backup_to(&safety)
        .map_err(|e| format!("Could not create the required safety backup: {e}"))?;
    if let Err(error) = database.restore_from(&source) {
        services::logging::event(
            "error",
            "catalog_restore_failed",
            &[
                ("source", source.display().to_string()),
                ("safety_backup", safety.display().to_string()),
                ("error", error.to_string()),
            ],
        );
        return Err(format!(
            "Restore failed. Your current catalog is unchanged or recoverable from {}. {error}",
            safety.display()
        ));
    }
    services::logging::event(
        "info",
        "catalog_restored",
        &[
            ("source", source.display().to_string()),
            ("safety_backup", safety.display().to_string()),
        ],
    );
    Ok(Some(safety.to_string_lossy().into_owned()))
}

#[tauri::command]
fn browse_books(
    database: State<'_, db::Database>,
    request: BrowseBooksRequest,
) -> Result<Vec<BrowserBook>, String> {
    database
        .browse_books(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_tags(database: State<'_, db::Database>) -> Result<Vec<(i64, String)>, String> {
    database.tags().map_err(|error| error.to_string())
}

#[tauri::command]
fn list_collections(database: State<'_, db::Database>) -> Result<Vec<(i64, String)>, String> {
    database.collections().map_err(|error| error.to_string())
}

#[tauri::command]
fn get_book_details(
    database: State<'_, db::Database>,
    book_id: i64,
) -> Result<Option<BookDetails>, String> {
    database
        .book_details(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_passage(
    database: State<'_, db::Database>,
    request: SavePassageRequest,
) -> Result<i64, String> {
    database.save_passage(&request).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_saved_passages(
    database: State<'_, db::Database>,
    book_id: Option<i64>,
    offset: i64,
) -> Result<Vec<SavedPassage>, String> {
    database
        .saved_passages(book_id, offset)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn edit_passage(
    database: State<'_, db::Database>,
    id: i64,
    sentence: String,
    note: String,
) -> Result<(), String> {
    database
        .edit_passage(id, &sentence, &note)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_passage(database: State<'_, db::Database>, id: i64) -> Result<(), String> {
    database.delete_passage(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_passage_location(database: State<'_, db::Database>, id: i64) -> Result<String, String> {
    database.passage_location(id)
}

#[tauri::command]
fn get_reader_book(
    database: State<'_, db::Database>,
    book_id: i64,
) -> Result<Option<ReaderBook>, String> {
    database
        .reader_book(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn remove_resume_book(database: State<'_, db::Database>, book_id: i64) -> Result<(), String> {
    database
        .remove_resume_book(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_resume_books(
    database: State<'_, db::Database>,
    available_only: bool,
) -> Result<Vec<ResumeBook>, String> {
    database
        .resume_books(available_only)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn record_reader_open(database: State<'_, db::Database>, book_id: i64) -> Result<bool, String> {
    database
        .record_reader_open(book_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_reading_state(
    database: State<'_, db::Database>,
    book_id: i64,
) -> Result<db::shelves::ReadingState, String> {
    database.reading_state(book_id).map_err(|e| e.to_string())
}
#[tauri::command]
fn set_reading_status(
    database: State<'_, db::Database>,
    book_id: i64,
    status: String,
) -> Result<(), String> {
    database
        .set_reading_status(book_id, &status)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn list_smart_shelves(
    database: State<'_, db::Database>,
) -> Result<Vec<db::shelves::SmartShelf>, String> {
    database.smart_shelves().map_err(|e| e.to_string())
}
#[tauri::command]
fn save_smart_shelf(
    database: State<'_, db::Database>,
    shelf: db::shelves::SmartShelf,
) -> Result<i64, String> {
    database.save_smart_shelf(&shelf).map_err(|e| e.to_string())
}
#[tauri::command]
fn delete_smart_shelf(database: State<'_, db::Database>, shelf_id: i64) -> Result<(), String> {
    database
        .delete_smart_shelf(shelf_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_reader_finished(
    database: State<'_, db::Database>,
    book_id: i64,
    finished: bool,
) -> Result<(), String> {
    database
        .set_reader_finished(book_id, finished)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_reading_location(
    database: State<'_, db::Database>,
    book_id: i64,
) -> Result<Option<String>, String> {
    database
        .reading_location(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_reading_location(
    database: State<'_, db::Database>,
    book_id: i64,
    location_cfi: String,
) -> Result<(), String> {
    if location_cfi.len() > 4_096 {
        return Err("Reading location is invalid.".into());
    }
    database
        .save_reading_location(book_id, &location_cfi)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn tokenize_dictionary_target(
    text: String,
    offset: usize,
) -> Result<services::readings::DictionaryTarget, String> {
    services::readings::dictionary_target(&text, offset)
}

#[tauri::command]
async fn lookup_reader_text(
    app: tauri::AppHandle,
    request: tmw_japanese_core::lookup::LookupRequest,
) -> Result<tmw_japanese_core::chunk_lookup::Response, String> {
    tauri::async_runtime::spawn_blocking(move || dictionary_import::lookup(&app, &request))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_app_setting(
    database: State<'_, db::Database>,
    key: String,
) -> Result<Option<String>, String> {
    database.setting(&key).map_err(|e| e.to_string())
}
#[tauri::command]
fn set_app_setting(
    database: State<'_, db::Database>,
    key: String,
    value: String,
) -> Result<(), String> {
    database
        .set_setting(&key, &value)
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn lookup_dictionary(
    app: tauri::AppHandle,
    query: String,
) -> Result<tmw_japanese_core::chunk_lookup::Response, String> {
    tauri::async_runtime::spawn_blocking(move || dictionary_import::lookup(&app, &tmw_japanese_core::lookup::LookupRequest { text: query, offset: 0 }))
        .await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn record_lookup_history(
    database: State<'_, db::Database>,
    request: db::history::LookupRecord,
) -> Result<Option<i64>, String> {
    database.record_lookup(&request).map_err(|e| e.to_string())
}
#[tauri::command]
fn list_lookup_history(
    database: State<'_, db::Database>,
    query: String,
    offset: i64,
) -> Result<Vec<db::history::LookupHistory>, String> {
    database
        .lookup_history(&query, offset)
        .map_err(|e| e.to_string())
}
#[tauri::command]
fn clear_lookup_history(database: State<'_, db::Database>) -> Result<(), String> {
    database.clear_lookup_history().map_err(|e| e.to_string())
}
#[tauri::command]
fn get_lookup_history_location(
    database: State<'_, db::Database>,
    id: i64,
) -> Result<String, String> {
    database.lookup_history_location(id)
}

#[tauri::command]
fn save_book_overrides(
    database: State<'_, db::Database>,
    book_id: i64,
    values: BookOverride,
) -> Result<(), String> {
    database
        .save_book_overrides(book_id, &values)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_book_override(
    database: State<'_, db::Database>,
    book_id: i64,
    field: String,
) -> Result<(), String> {
    database
        .reset_book_override(book_id, &field)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_all_book_overrides(database: State<'_, db::Database>, book_id: i64) -> Result<(), String> {
    database
        .reset_all_book_overrides(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_book_tags(
    database: State<'_, db::Database>,
    book_id: i64,
    tag_names: Vec<String>,
) -> Result<(), String> {
    database
        .replace_book_tags(book_id, &tag_names)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_set_book_tags(
    database: State<'_, db::Database>,
    request: BatchTagRequest,
) -> Result<(), String> {
    database
        .batch_replace_tags(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_folder_group(
    database: State<'_, db::Database>,
    book_id: i64,
) -> Result<Option<FolderGroup>, String> {
    database
        .folder_group(book_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_collection(
    database: State<'_, db::Database>,
    request: CreateCollectionRequest,
) -> Result<i64, String> {
    database
        .create_collection(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn assign_series(
    database: State<'_, db::Database>,
    request: AssignSeriesRequest,
) -> Result<(), String> {
    database
        .assign_series(&request)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn choose_cover_replacement(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Select a replacement cover image")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
        .blocking_pick_file()
        .map(|path| path.to_string())
}

#[tauri::command]
fn open_book_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let directory = std::path::Path::new(&path);
    if !directory.is_dir() {
        return Err("The cataloged folder is no longer available.".into());
    }
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|error| error.to_string())
}

/// Rebuilds only the regenerable cache; EPUB files are opened read-only.
#[tauri::command]
fn regenerate_cover_cache(
    app: tauri::AppHandle,
    database: State<'_, db::Database>,
) -> Result<usize, String> {
    let cache = cover_cache_directory(&app, &database)?;
    let books = database
        .books_for_cover_regeneration()
        .map_err(|error| error.to_string())?;
    let mut regenerated = 0;
    for book in books {
        let extracted = services::metadata::extract_epub(
            std::path::Path::new(&book.file_path),
            book.id,
            &cache,
        );
        database
            .save_extracted_metadata(book.id, &extracted)
            .map_err(|error| error.to_string())?;
        regenerated += 1;
    }
    Ok(regenerated)
}

#[tauri::command]
fn companion_status(service: State<'_, companion::Service>) -> companion::Status {
    service.status()
}
#[tauri::command]
fn companion_enable(
    app: tauri::AppHandle,
    service: State<'_, companion::Service>,
) -> Result<companion::Status, String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    service.enable(
        root.join("catalog.sqlite3"),
        root.join("companion-devices.sqlite3"),
    )
}
#[tauri::command]
fn companion_disable(service: State<'_, companion::Service>) {
    service.disable();
}
#[tauri::command]
fn companion_pairing_code(service: State<'_, companion::Service>) -> Result<String, String> {
    service.pairing_code()
}
#[tauri::command]
fn companion_revoke(
    service: State<'_, companion::Service>,
    device_id: String,
) -> Result<(), String> {
    service.revoke(&device_id)
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data = app.path().app_data_dir()?;
            services::logging::initialize(&app_data.join("logs"))?;
            let database_path: PathBuf = app_data.join("catalog.sqlite3");
            app.manage(db::Database::open(&database_path)?);
            app.manage(ScanController::default());
            app.manage(companion::Service::default());
            app.manage(IndexRebuildController::default());
            app.manage(tmw_japanese_core::dictionary_storage::Jobs::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dictionary_import::dictionary_import,
            dictionary_import::dictionary_manage,
            dictionary_import::dictionary_import_status,
            dictionary_import::cancel_dictionary_import,
            companion_status,
            companion_enable,
            companion_disable,
            companion_pairing_code,
            companion_revoke,
            choose_library_folder,
            add_library_root,
            list_library_roots,
            rescan_library_root,
            cancel_scan,
            rebuild_search_index,
            cancel_search_index_rebuild,
            remove_library_root,
            delete_book,
            export_catalog_backup,
            import_catalog_backup,
            choose_cover_cache_folder,
            get_cover_cache_directory,
            set_cover_cache_directory,
            browse_books,
            list_tags,
            list_collections,
            regenerate_cover_cache,
            get_book_details,
            save_book_overrides,
            reset_book_override,
            reset_all_book_overrides,
            set_book_tags,
            batch_set_book_tags,
            get_folder_group,
            create_collection,
            assign_series,
            choose_cover_replacement,
            open_book_folder,
            get_reader_book,
            save_passage,
            list_saved_passages,
            edit_passage,
            delete_passage,
            get_passage_location,
            list_resume_books,
            remove_resume_book,
            record_reader_open,
            set_reader_finished,
            get_reading_state,
            set_reading_status,
            list_smart_shelves,
            save_smart_shelf,
            delete_smart_shelf,
            get_reading_location,
            save_reading_location,
            tokenize_dictionary_target,
            lookup_reader_text,
            get_app_setting,
            set_app_setting,
            lookup_dictionary,
            record_lookup_history,
            list_lookup_history,
            clear_lookup_history,
            get_lookup_history_location
        ])
        .run(tauri::generate_context!())
        .expect("error while running TMW Library");
}
