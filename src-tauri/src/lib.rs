pub mod db;
pub mod models;
pub mod services;

use std::path::PathBuf;

use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

use crate::{
    models::library_root::{LibraryRoot, LibraryRootSummary, NewLibraryRoot},
    models::book::{BatchTagRequest, BookDetails, BookOverride, BrowseBooksRequest, BrowserBook},
    services::scanner::ScanController,
};

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
    scans: State<'_, ScanController>,
    path: String,
) -> Result<LibraryRoot, String> {
    let directory = std::path::Path::new(&path);
    if !directory.is_dir() {
        return Err("The selected path is not an accessible folder.".into());
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
    services::scanner::scan_root(
        &app,
        &database,
        &scans,
        root.id,
        &root.path,
        &cache,
        &format!("add-{}", root.id),
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
    scans: State<'_, ScanController>,
    root_id: i64,
    scan_id: String,
) -> Result<(), String> {
    let root = database
        .library_root(root_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Library root not found.".to_string())?;
    let cache = cover_cache_directory(&app, &database)?;
    services::scanner::scan_root(
        &app, &database, &scans, root.id, &root.path, &cache, &scan_id,
    )
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
fn remove_library_root(database: State<'_, db::Database>, root_id: i64) -> Result<bool, String> {
    database
        .remove_library_root(root_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn browse_books(database: State<'_, db::Database>, request: BrowseBooksRequest) -> Result<Vec<BrowserBook>, String> {
    database.browse_books(&request).map_err(|error| error.to_string())
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
fn get_book_details(database: State<'_, db::Database>, book_id: i64) -> Result<Option<BookDetails>, String> {
    database.book_details(book_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn save_book_overrides(database: State<'_, db::Database>, book_id: i64, values: BookOverride) -> Result<(), String> {
    database.save_book_overrides(book_id, &values).map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_book_override(database: State<'_, db::Database>, book_id: i64, field: String) -> Result<(), String> {
    database.reset_book_override(book_id, &field).map_err(|error| error.to_string())
}

#[tauri::command]
fn reset_all_book_overrides(database: State<'_, db::Database>, book_id: i64) -> Result<(), String> {
    database.reset_all_book_overrides(book_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn set_book_tags(database: State<'_, db::Database>, book_id: i64, tag_names: Vec<String>) -> Result<(), String> {
    database.replace_book_tags(book_id, &tag_names).map_err(|error| error.to_string())
}

#[tauri::command]
fn batch_set_book_tags(database: State<'_, db::Database>, request: BatchTagRequest) -> Result<(), String> {
    database.batch_replace_tags(&request).map_err(|error| error.to_string())
}

#[tauri::command]
fn choose_cover_replacement(app: tauri::AppHandle) -> Option<String> {
    app.dialog().file().set_title("Select a replacement cover image").add_filter("Images", &["png", "jpg", "jpeg", "webp"]).blocking_pick_file().map(|path| path.to_string())
}

#[tauri::command]
fn open_book_folder(app: tauri::AppHandle, path: String) -> Result<(), String> {
    let directory = std::path::Path::new(&path);
    if !directory.is_dir() { return Err("The cataloged folder is no longer available.".into()); }
    app.opener().open_path(&path, None::<&str>).map_err(|error| error.to_string())
}

/// Rebuilds only the regenerable cache; EPUB files are opened read-only.
#[tauri::command]
fn regenerate_cover_cache(app: tauri::AppHandle, database: State<'_, db::Database>) -> Result<usize, String> {
    let cache = cover_cache_directory(&app, &database)?;
    let books = database.books_for_cover_regeneration().map_err(|error| error.to_string())?;
    let mut regenerated = 0;
    for book in books {
        let extracted = services::metadata::extract_epub(std::path::Path::new(&book.file_path), book.id, &cache);
        database.save_extracted_metadata(book.id, &extracted).map_err(|error| error.to_string())?;
        regenerated += 1;
    }
    Ok(regenerated)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let database_path: PathBuf = app.path().app_data_dir()?.join("catalog.sqlite3");
            app.manage(db::Database::open(&database_path)?);
            app.manage(ScanController::default());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            choose_library_folder,
            add_library_root,
            list_library_roots,
            rescan_library_root,
            cancel_scan,
            remove_library_root,
            choose_cover_cache_folder,
            get_cover_cache_directory,
            set_cover_cache_directory
            ,browse_books
            ,list_tags
            ,list_collections
            ,regenerate_cover_cache
            ,get_book_details
            ,save_book_overrides
            ,reset_book_override
            ,reset_all_book_overrides
            ,set_book_tags
            ,batch_set_book_tags
            ,choose_cover_replacement
            ,open_book_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running TMW Library");
}
