use std::{fs::File, path::PathBuf};
use tauri::{Manager, State};
use tauri_plugin_dialog::DialogExt;
use tmw_japanese_core::dictionary_storage::{ImportReport, JobStatus, Jobs, Storage};

pub(crate) fn root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let canonical = directory.canonicalize().map_err(|e| e.to_string())?;
    let target = directory.join("yomitan-v1");
    let target = if target.exists() {
        target.canonicalize().map_err(|e| e.to_string())?
    } else {
        canonical.join("yomitan-v1")
    };
    let database = app.state::<crate::db::Database>();
    for library in database.library_roots().map_err(|e| e.to_string())? {
        let source = PathBuf::from(library.root.path);
        let source = source.canonicalize().unwrap_or(source);
        if target.starts_with(&source) || source.starts_with(&target) {
            return Err("Dictionary storage must be separate from source library folders".into());
        }
    }
    Ok(directory.join("yomitan-v1"))
}

#[tauri::command]
pub async fn dictionary_import(
    app: tauri::AppHandle,
    jobs: State<'_, Jobs>,
    replace: Option<i64>,
) -> Result<ImportReport, String> {
    jobs.begin()?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        let jobs = app.state::<Jobs>();
        let result = (|| {
            let directory = root(&app)?;
            let source = app
                .dialog()
                .file()
                .set_title("Import Yomitan dictionary")
                .add_filter("Dictionary ZIP", &["zip"])
                .blocking_pick_file()
                .ok_or("Dictionary import canceled")?
                .into_path()
                .map_err(|e| e.to_string())?;
            jobs.import(
                &directory,
                File::open(source).map_err(|e| e.to_string())?,
                replace,
            )
        })();
        jobs.finish(&result);
        result
    })
    .await
    .map_err(|e| e.to_string());
    match result {
        Ok(result) => result,
        Err(error) => {
            let result = Err(error);
            jobs.finish(&result);
            result
        }
    }
}

#[tauri::command]
pub async fn dictionary_manage(
    app: tauri::AppHandle,
    action: String,
    id: Option<i64>,
    enabled: Option<bool>,
    priority: Option<i64>,
    result_limit: Option<i64>,
) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let storage = Storage::open(&root(&app)?)?;
        match action.as_str() {
            "list" => {}
            "update" => storage.update(
                id.ok_or("Dictionary ID required")?,
                enabled.ok_or("Enabled state required")?,
                priority.ok_or("Priority required")?,
            )?,
            "limit" => storage.set_result_limit(id.ok_or("Dictionary ID required")?, result_limit.ok_or("Result limit required")?)?,
            "remove" => storage.remove(id.ok_or("Dictionary ID required")?)?,
            _ => return Err("Invalid dictionary action".into()),
        }
        serde_json::to_value(storage.list()?).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn dictionary_import_status(jobs: State<'_, Jobs>) -> JobStatus {
    jobs.status()
}

#[tauri::command]
pub fn cancel_dictionary_import(jobs: State<'_, Jobs>) {
    jobs.cancel();
}

// Imported sources are the only desktop reader lookup source. Existing catalog
// dictionary rows remain available to legacy history and metadata consumers.
pub(crate) fn lookup(app: &tauri::AppHandle, request: &tmw_japanese_core::lookup::LookupRequest) -> Result<tmw_japanese_core::chunk_lookup::Response, String> {
    let storage = Storage::open(&root(app)?)?;
    if !storage.list()?.iter().any(|source| source.enabled && source.term_count > 0) {
        return Err("Import a local Yomitan term dictionary ZIP or enable a term dictionary in Manage dictionaries.".into());
    }
    struct Imported(Storage);
    impl tmw_japanese_core::chunk_lookup::Store for Imported {
        fn query_batch(&mut self, keys: &[String]) -> Result<Vec<tmw_japanese_core::chunk_lookup::Entry>, String> {
            self.0.query_batch(keys)
        }
    }
    let mut store = Imported(storage);
    let mut response = tmw_japanese_core::chunk_lookup::lookup(request, &mut store)?;
    store.0.limit_response(&mut response)?;
    store.0.decorate_response(&mut response)?;
    Ok(response)
}
