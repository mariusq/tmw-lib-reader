use std::fs::File;
use tauri::{Manager, State};
use tmw_japanese_core::dictionary_storage::{ImportReport, JobStatus, Jobs, Storage};

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
            let root = app
                .path()
                .app_data_dir()
                .map_err(|e| e.to_string())?
                .join("yomitan-v1");
            let response = crate::connection::dictionary_document(&app, false)?;
            let temporary = std::path::PathBuf::from(
                response["path"]
                    .as_str()
                    .ok_or("Missing dictionary document")?,
            );
            let result = (|| {
                jobs.import(
                    &root,
                    File::open(&temporary).map_err(|e| e.to_string())?,
                    replace,
                )
            })();
            // This is the native bridge's fixed private staging file, never the provider document.
            let _ = std::fs::remove_file(temporary);
            result
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
        let root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("yomitan-v1");
        let storage = Storage::open(&root)?;
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
pub async fn cancel_dictionary_import(
    app: tauri::AppHandle,
    jobs: State<'_, Jobs>,
) -> Result<(), String> {
    jobs.cancel();
    tauri::async_runtime::spawn_blocking(move || {
        crate::connection::dictionary_document(&app, true).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}
