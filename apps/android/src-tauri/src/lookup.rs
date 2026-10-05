use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{fs, path::Path, sync::Mutex, time::Instant};
use tmw_japanese_core::{
    chunk_lookup::{self, Response},
    dictionary_storage::Storage,
    lookup::LookupRequest,
    lookup_storage::{Schema, SqliteStore},
};
#[cfg(test)]
use tmw_japanese_core::{
    dictionary::{normalize_query, LOOKUP_PREDICATE},
    lookup::DictionaryEntry as Entry,
};
static LOCK: Mutex<()> = Mutex::new(());
const DATA: &[u8] = include_bytes!("../assets/jmdict-20260928-v2.sqlite3.gz");

fn database(root: &Path) -> Result<Connection, String> {
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let path = root.join("jmdict-20260928-v2.sqlite3");
    if !path.exists() {
        let temp = root.join("dictionary-install.tmp");
        let mut decoder = flate2::read::GzDecoder::new(DATA);
        let mut output = fs::File::create(&temp).map_err(|e| e.to_string())?;
        std::io::copy(&mut decoder, &mut output).map_err(|e| e.to_string())?;
        output.sync_all().map_err(|e| e.to_string())?;
        drop(output);
        let db = Connection::open_with_flags(&temp, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        let ok: String = db
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if ok != "ok" {
            return Err(ok);
        }
        drop(db);
        fs::rename(temp, &path).map_err(|e| e.to_string())?;
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}
#[cfg(test)]
fn query(db: &Connection, value: &str) -> Result<Vec<Entry>, String> {
    let normalized = normalize_query(value);
    if normalized.is_empty() {
        return Ok(vec![]);
    }
    let sql = format!(
        "SELECT term,reading,definitions,part_of_speech,id FROM entries e WHERE {LOOKUP_PREDICATE}"
    );
    let mut statement = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(
            rusqlite::params![normalized, format!("{normalized}%")],
            |r| {
                Ok(Entry {
                    id: r.get(4)?,
                    dictionary_name: "JMdict".into(),
                    term: r.get(0)?,
                    reading: r.get(1)?,
                    definitions: serde_json::from_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                    part_of_speech: serde_json::from_str(&r.get::<_, String>(3)?)
                        .unwrap_or_default(),
                })
            },
        )
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    #[serde(flatten)]
    pub result: Response,
    elapsed_ms: f64,
    dictionary_bytes: usize,
}
pub fn lookup(root: &Path, text: &str, offset: usize) -> Result<Report, String> {
    let _guard = LOCK.lock().map_err(|e| e.to_string())?;
    let started = Instant::now();
    let db = database(root)?;
    let mut imported = Storage::open(&root.join("yomitan-v1"))?;
    let mut bundled = SqliteStore {
        connection: &db,
        schema: Schema::Bundled,
    };
    let mut result = chunk_lookup::lookup(
        &LookupRequest {
            text: text.into(),
            offset,
        },
        &mut (&mut imported, &mut bundled),
    )?;
    imported.limit_response(&mut result)?;
    imported.decorate_response(&mut result)?;
    Ok(Report {
        result,
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        dictionary_bytes: fs::metadata(root.join("jmdict-20260928-v2.sqlite3"))
            .map_err(|e| e.to_string())?
            .len() as usize,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_lookup_and_atomic_install() {
        let root = tempfile::tempdir().unwrap();
        let r = lookup(root.path(), "猫を食べました。", 3).unwrap();
        assert_eq!(r.result.groups[0].term, "食べる");
        assert_eq!(
            tmw_japanese_core::readings::dictionary_target("😀 猫を食べました。", 2)
                .unwrap()
                .surface,
            "猫"
        );
        assert!(r.result.groups.iter().any(|e| e.term == "食べる"));
        let serialized = serde_json::to_value(&r).unwrap();
        assert_eq!(serialized["engineVersion"], 2);
        assert!(serialized.get("target").is_some());
        assert!(serialized.get("result").is_none());
        assert_eq!(
            serialized["groups"][0]["matches"][0]["entry"]["provenance"]["title"],
            "JMdict"
        );
        assert_eq!(
            serialized["matchedSpan"],
            serde_json::json!({"start": 2, "end": 7})
        );
        let db = database(root.path()).unwrap();
        assert_eq!(query(&db, "猫").unwrap()[0].term, "猫");
        assert_eq!(
            query(&db, "ﾈｺ").unwrap().len(),
            query(&db, "ネコ").unwrap().len()
        );
        assert!(!root.path().join("dictionary-install.tmp").exists());
    }
}
