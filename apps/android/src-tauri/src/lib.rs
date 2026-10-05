mod connection;
mod dictionary_import;
use rusqlite::Connection;
use serde::Serialize;
use std::{fs, path::Path, time::Instant};
use tauri::Manager;
use tmw_japanese_core::{dictionary::import_jmdict, readings::dictionary_target};
mod lookup;

#[tauri::command]
async fn lookup_text(
    app: tauri::AppHandle,
    text: String,
    offset: usize,
) -> Result<lookup::Report, String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || lookup::lookup(&root, &text, offset))
        .await
        .map_err(|e| e.to_string())?
}

// Synthetic import fixture, not a bundled JMdict dataset. Full provisioning is Phase 2.
const IMPORT_FIXTURE: &str = r#"{"tags":{"n":"noun"},"words":[{"kanji":[{"text":"猫"}],"kana":[{"text":"ねこ"}],"sense":[{"partOfSpeech":["n"],"gloss":[{"lang":"eng","text":"cat"}]}]}]}"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProbeReport {
    architecture: String,
    surface: String,
    lemma: String,
    reading: Option<String>,
    tokenizer_ms: f64,
    import_forms: usize,
    sqlite_version: String,
    persisted_runs: i64,
    total_ms: f64,
}

fn probe(directory: &Path) -> Result<ProbeReport, String> {
    let started = Instant::now();
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let token_started = Instant::now();
    let target = dictionary_target("猫を食べました。", 3)?;
    if target.lemma != "食べる" {
        return Err(format!("Unexpected tokenizer result: {}", target.lemma));
    }
    let tokenizer_ms = token_started.elapsed().as_secs_f64() * 1000.0;
    let fixture = directory.join("feasibility-import.json");
    fs::write(&fixture, IMPORT_FIXTURE).map_err(|e| e.to_string())?;
    let imported = import_jmdict(&fixture);
    fs::remove_file(&fixture).map_err(|e| e.to_string())?;
    let entries = imported?;
    if entries.len() != 2 || entries.iter().any(|entry| entry.definitions != ["cat"]) {
        return Err("JMdict importer smoke check failed".into());
    }
    let mut connection =
        Connection::open(directory.join("feasibility.sqlite3")).map_err(|e| e.to_string())?;
    connection.execute_batch("CREATE TABLE IF NOT EXISTS probe_runs(id INTEGER PRIMARY KEY CHECK(id=1), runs INTEGER NOT NULL);")
        .map_err(|e| e.to_string())?;
    let transaction = connection.transaction().map_err(|e| e.to_string())?;
    transaction
        .execute(
            "INSERT INTO probe_runs(id,runs) VALUES(1,1) ON CONFLICT(id) DO UPDATE SET runs=runs+1",
            [],
        )
        .map_err(|e| e.to_string())?;
    let persisted_runs = transaction
        .query_row("SELECT runs FROM probe_runs WHERE id=1", [], |row| {
            row.get(0)
        })
        .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(ProbeReport {
        architecture: std::env::consts::ARCH.into(),
        surface: target.surface,
        lemma: target.lemma,
        reading: target.reading,
        tokenizer_ms,
        import_forms: entries.len(),
        sqlite_version: rusqlite::version().into(),
        persisted_runs,
        total_ms: started.elapsed().as_secs_f64() * 1000.0,
    })
}

#[tauri::command]
async fn run_feasibility_probe(app: tauri::AppHandle) -> Result<ProbeReport, String> {
    let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Serializing this tiny diagnostic avoids concurrent fixture writes.
    tauri::async_runtime::spawn_blocking(move || {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().map_err(|e| e.to_string())?;
        probe(&directory)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(connection::plugin())
        .manage(tmw_japanese_core::dictionary_storage::Jobs::default())
        .invoke_handler(tauri::generate_handler![
            dictionary_import::dictionary_import,
            dictionary_import::dictionary_manage,
            dictionary_import::dictionary_import_status,
            dictionary_import::cancel_dictionary_import,
            run_feasibility_probe,
            lookup_text,
            connection::private_connection,
            connection::mobile_storage,
            read_mobile_book
        ])
        .run(tauri::generate_context!())
        .expect("Could not start TMW Companion");
}

/// Binary IPC: no full-book JSON/base64, no caller-supplied path. Exactly one
/// validated complete app-private file is loaded for epub.js.
#[tauri::command]
async fn read_mobile_book(
    app: tauri::AppHandle,
    file: String,
) -> Result<tauri::ipc::Response, String> {
    if file.len() != 69
        || !file.ends_with(".epub")
        || !file
            .get(..64)
            .is_some_and(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err("Invalid local copy".into());
    }
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let path = root.join("files/tmw-mobile/books").join(file);
        let size = fs::metadata(&path)
            .map_err(|_| "Local copy unavailable")?
            .len();
        if size > 64_000_000 {
            return Err("Reader input exceeds 64 MB".into());
        }
        let bytes = fs::read(path).map_err(|_| "Local copy unavailable")?;
        Ok(tauri::ipc::Response::new(bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_tokenizer_importer_and_sqlite_survive_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let first = probe(directory.path()).unwrap();
        assert_eq!(first.lemma, "食べる");
        assert_eq!(first.import_forms, 2);
        assert_eq!(first.persisted_runs, 1);
        assert_eq!(probe(directory.path()).unwrap().persisted_runs, 2);
        assert!(!directory.path().join("feasibility-import.json").exists());
    }
}
