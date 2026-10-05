//! App-private, versioned import registry shared by Windows and Android.
//! One transaction publishes entries, assets, notices and settings together.
//! Failed/unpublished sinks roll back on drop; the catalog/JMdict are never opened.
pub use crate::yomitan::ImportReport;
use crate::{
    dictionary::{
        normalize_query, DictionaryImportSink, DictionaryManifest, TagRecord, TermRecord,
    },
    yomitan::{self, ImportProgress},
};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    io::{Read, Seek},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};

fn err(e: impl std::fmt::Display) -> String {
    format!("Dictionary storage: {e}. Check free app-storage space and retry.")
}
pub struct Storage {
    connection: Connection,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: i64,
    pub title: String,
    pub revision: String,
    pub enabled: bool,
    pub priority: i64,
    pub result_limit: i64,
    pub term_count: i64,
    pub attribution: Option<String>,
    pub warnings: Vec<String>,
}
impl Storage {
    pub fn open(root: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(root).map_err(err)?;
        let connection = Connection::open(root.join("dictionaries.sqlite3")).map_err(err)?;
        connection
            .busy_timeout(Duration::from_millis(500))
            .map_err(err)?;
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(err)?;
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(err)?;
        if version > 1 {
            return Err(
                "Dictionary storage belongs to a newer app version; update this app.".into(),
            );
        }
        if version == 0 {
            connection.execute_batch("BEGIN IMMEDIATE;
                CREATE TABLE dictionaries(id INTEGER PRIMARY KEY AUTOINCREMENT,title TEXT NOT NULL UNIQUE,revision TEXT NOT NULL,manifest TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,priority INTEGER NOT NULL DEFAULT 0,term_count INTEGER NOT NULL DEFAULT 0,attribution TEXT,warnings TEXT NOT NULL DEFAULT '[]');
                CREATE TABLE terms(id INTEGER PRIMARY KEY,dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,term TEXT NOT NULL,reading TEXT NOT NULL,term_normalized TEXT NOT NULL,reading_normalized TEXT NOT NULL,definition_tags TEXT NOT NULL,rules TEXT NOT NULL,score REAL NOT NULL,glossary TEXT NOT NULL,safe_glossary TEXT NOT NULL,sequence INTEGER NOT NULL,term_tags TEXT NOT NULL);
                CREATE INDEX terms_term ON terms(term_normalized,dictionary_id);
                CREATE INDEX terms_reading ON terms(reading_normalized,dictionary_id);
                CREATE INDEX terms_sequence ON terms(dictionary_id,sequence);
                CREATE TABLE tags(dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,name TEXT NOT NULL,category TEXT NOT NULL,sort_order REAL NOT NULL,notes TEXT NOT NULL,score REAL NOT NULL,PRIMARY KEY(dictionary_id,name));
                CREATE TABLE assets(dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,path TEXT NOT NULL,bytes BLOB NOT NULL,PRIMARY KEY(dictionary_id,path));
                PRAGMA user_version=1; COMMIT;").map_err(err)?;
        }
        connection.execute_batch("CREATE TABLE IF NOT EXISTS term_metadata(id INTEGER PRIMARY KEY,dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,term_normalized TEXT NOT NULL,mode TEXT NOT NULL,data TEXT NOT NULL); CREATE INDEX IF NOT EXISTS metadata_term ON term_metadata(term_normalized,dictionary_id);").map_err(err)?;
        let has_limit = connection.prepare("PRAGMA table_info(dictionaries)").map_err(err)?
            .query_map([], |row| row.get::<_, String>(1)).map_err(err)?
            .collect::<Result<Vec<_>, _>>().map_err(err)?.iter().any(|name| name == "result_limit");
        if !has_limit {
            connection.execute_batch("ALTER TABLE dictionaries ADD COLUMN result_limit INTEGER NOT NULL DEFAULT 0").map_err(err)?;
        }
        // Additive covering indexes rank before the per-source cap, including old imports.
        let ranked_indexes: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name IN ('terms_term_ranked','terms_reading_ranked')",
            [], |row| row.get(0),
        ).map_err(err)?;
        if ranked_indexes != 2 {
            connection.execute_batch("BEGIN IMMEDIATE; CREATE INDEX IF NOT EXISTS terms_term_ranked ON terms(term_normalized,dictionary_id,score DESC,id); CREATE INDEX IF NOT EXISTS terms_reading_ranked ON terms(reading_normalized,dictionary_id,score DESC,id); COMMIT;").map_err(err)?;
        }
        Ok(Self { connection })
    }
    #[cfg(feature = "tokenizer")]
    pub fn query_batch(&self, keys: &[String]) -> Result<Vec<crate::chunk_lookup::Entry>, String> {
        use crate::chunk_lookup::Store;
        crate::lookup_storage::SqliteStore {
            connection: &self.connection,
            schema: crate::lookup_storage::Schema::Imported,
        }
        .query_batch(keys)
    }
    /// Hydrate only ranked results: never decode assets for every candidate row.
    #[cfg(feature = "tokenizer")]
    pub fn decorate_response(
        &self,
        response: &mut crate::chunk_lookup::Response,
    ) -> Result<(), String> {
        use crate::chunk_lookup::TermMetadata;
        use base64::Engine;
        use std::collections::BTreeSet;
        fn paths(value: &serde_json::Value, out: &mut BTreeSet<String>) {
            match value {
                serde_json::Value::Array(a) => {
                    for v in a {
                        paths(v, out);
                    }
                }
                serde_json::Value::Object(o) => {
                    if o.get("tag").and_then(serde_json::Value::as_str) == Some("img") {
                        if let Some(p) = o.get("path").and_then(serde_json::Value::as_str) {
                            out.insert(p.into());
                        }
                    }
                    if let Some(v) = o.get("content") {
                        paths(v, out);
                    }
                }
                _ => {}
            }
        }
        let mut asset_bytes = 0usize;
        let mut metadata_bytes = 0usize;
        for group in &mut response.groups {
            for hit in &mut group.matches {
                let entry = &mut hit.entry;
                // Metadata dictionaries annotate both imported and bundled headwords.
                let mut query=self.connection.prepare_cached("SELECT d.title,m.mode,m.data FROM term_metadata m INDEXED BY metadata_term JOIN dictionaries d ON d.id=m.dictionary_id WHERE m.term_normalized=?1 AND d.enabled=1 ORDER BY d.priority DESC,d.title,m.id LIMIT 64").map_err(err)?;
                let rows = query
                    .query_map([normalize_query(&entry.term)], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(err)?;
                for row in rows {
                    let (title, mode, json) = row.map_err(err)?;
                    metadata_bytes += json.len();
                    if metadata_bytes > 512 * 1024 {
                        return Err("Dictionary metadata exceeds 512 KiB result limit".into());
                    }
                    let data: serde_json::Value = serde_json::from_str(&json).map_err(err)?;
                    if data
                        .get("reading")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|r| normalize_query(r) != normalize_query(&entry.reading))
                    {
                        continue;
                    }
                    entry.metadata.push(TermMetadata {
                        source: format!("yomitan:{title}"),
                        title,
                        mode,
                        data,
                    });
                }
                if !entry.provenance.source.starts_with("yomitan:") {
                    continue;
                }
                let mut references = BTreeSet::new();
                paths(&entry.glossary, &mut references);
                for path in references.into_iter().take(16) {
                    if !crate::yomitan::safe_path(&path) {
                        continue;
                    }
                    let found=self.connection.query_row("SELECT a.bytes FROM assets a JOIN dictionaries d ON d.id=a.dictionary_id WHERE d.title=?1 AND d.revision=?2 AND d.enabled=1 AND a.path=?3",params![entry.provenance.title,entry.provenance.revision,path],|r|r.get::<_,Vec<u8>>(0));
                    let bytes = match found {
                        Ok(b) => b,
                        Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                        Err(e) => return Err(err(e)),
                    };
                    // Imported encodings/dimensions were validated. Cap IPC/base64 payload.
                    if bytes.len() > 1024 * 1024 || asset_bytes + bytes.len() > 3 * 1024 * 1024 {
                        continue;
                    }
                    let mime = match image::guess_format(&bytes).map_err(err)? {
                        image::ImageFormat::Png => "image/png",
                        image::ImageFormat::Jpeg => "image/jpeg",
                        image::ImageFormat::WebP => "image/webp",
                        _ => continue,
                    };
                    asset_bytes += bytes.len();
                    entry.assets.insert(
                        path,
                        format!(
                            "data:{mime};base64,{}",
                            base64::engine::general_purpose::STANDARD.encode(&bytes)
                        ),
                    );
                }
            }
        }
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<Summary>, String> {
        let mut statement=self.connection.prepare("SELECT id,title,revision,enabled,priority,term_count,attribution,warnings,result_limit FROM dictionaries ORDER BY priority DESC,title,id").map_err(err)?;
        let rows = statement
            .query_map([], |r| {
                let warnings: String = r.get(7)?;
                Ok(Summary {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    revision: r.get(2)?,
                    enabled: r.get(3)?,
                    priority: r.get(4)?,
                    result_limit: r.get(8)?,
                    term_count: r.get(5)?,
                    attribution: r.get(6)?,
                    warnings: serde_json::from_str(&warnings).unwrap_or_default(),
                })
            })
            .map_err(err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(err)
    }
    pub fn set_result_limit(&self, id: i64, limit: i64) -> Result<(), String> {
        if !(0..=256).contains(&limit) { return Err("Result limit must be between 0 and 256 (0 uses the default).".into()); }
        self.connection.execute("UPDATE dictionaries SET result_limit=?2 WHERE id=?1", params![id, limit]).map_err(err)?;
        Ok(())
    }
    #[cfg(feature = "tokenizer")]
    pub fn limit_response(&self, response: &mut crate::chunk_lookup::Response) -> Result<(), String> {
        let limits: std::collections::HashMap<_, _> = self.list()?.into_iter().map(|d| (d.title, d.result_limit)).collect();
        let mut counts = std::collections::HashMap::new();
        for group in &mut response.groups {
            group.matches.retain(|hit| {
                let title = &hit.entry.provenance.title;
                let limit = limits.get(title).copied().unwrap_or(0);
                let count = counts.entry(title.clone()).or_insert(0);
                *count += 1;
                limit == 0 || *count <= limit
            });
        }
        response.groups.retain(|group| !group.matches.is_empty());
        Ok(())
    }
    pub fn update(&self, id: i64, enabled: bool, priority: i64) -> Result<(), String> {
        if !(-1000..=1000).contains(&priority) {
            return Err("Dictionary priority must be between -1000 and 1000.".into());
        }
        if self
            .connection
            .execute(
                "UPDATE dictionaries SET enabled=?2,priority=?3 WHERE id=?1",
                params![id, enabled, priority],
            )
            .map_err(err)?
            != 1
        {
            return Err("Imported dictionary no longer exists. Refresh the list.".into());
        }
        Ok(())
    }
    pub fn remove(&self, id: i64) -> Result<(), String> {
        // Only generated SQLite rows/blobs; no paths or source material are accepted.
        if self
            .connection
            .execute("DELETE FROM dictionaries WHERE id=?1", [id])
            .map_err(err)?
            != 1
        {
            return Err("Imported dictionary no longer exists. Refresh the list.".into());
        }
        Ok(())
    }
    pub fn import<R: Read + Seek>(
        self,
        reader: R,
        replace: Option<i64>,
        cancel: &AtomicBool,
        notify: impl FnMut(&ImportProgress),
    ) -> Result<ImportReport, String> {
        yomitan::import(
            reader,
            |manifest| Sink::new(self.connection, manifest, replace),
            cancel,
            notify,
        )
    }
}

struct Sink {
    connection: Connection,
    id: i64,
    terms: i64,
    published: bool,
}
impl Sink {
    fn new(
        connection: Connection,
        manifest: &DictionaryManifest,
        replace: Option<i64>,
    ) -> Result<Self, String> {
        connection.execute_batch("BEGIN IMMEDIATE").map_err(err)?;
        let mut sink = Self {
            connection,
            id: 0,
            terms: 0,
            published: false,
        };
        let json = serde_json::to_string(manifest).map_err(err)?;
        if let Some(id) = replace {
            let title: String = sink
                .connection
                .query_row("SELECT title FROM dictionaries WHERE id=?1", [id], |r| {
                    r.get(0)
                })
                .map_err(|_| "Replacement target no longer exists.".to_owned())?;
            if title != manifest.title {
                return Err("Replacement ZIP title differs from the selected dictionary. Import it as a new dictionary instead.".into());
            }
            for table in ["terms", "tags", "assets", "term_metadata"] {
                sink.connection
                    .execute(&format!("DELETE FROM {table} WHERE dictionary_id=?1"), [id])
                    .map_err(err)?;
            }
            sink.connection.execute("UPDATE dictionaries SET revision=?2,manifest=?3,attribution=?4,term_count=0,warnings='[]' WHERE id=?1",params![id,manifest.revision,json,manifest.attribution]).map_err(err)?;
            sink.id = id;
        } else {
            sink.connection.execute("INSERT INTO dictionaries(title,revision,manifest,attribution) VALUES(?1,?2,?3,?4)",params![manifest.title,manifest.revision,json,manifest.attribution]).map_err(|e| {
                if matches!(e,rusqlite::Error::SqliteFailure(ref code,_) if code.code == rusqlite::ErrorCode::ConstraintViolation) {"This dictionary title is already imported. Use its Replace ZIP button to replace it explicitly.".into()} else {err(e)}
            })?;
            sink.id = sink.connection.last_insert_rowid();
        }
        Ok(sink)
    }
}
impl DictionaryImportSink for Sink {
    fn write_metadata(
        &mut self,
        rows: &[(String, String, serde_json::Value)],
    ) -> Result<(), String> {
        let mut statement = self.connection.prepare_cached("INSERT INTO term_metadata(dictionary_id,term_normalized,mode,data) VALUES(?1,?2,?3,?4)").map_err(err)?;
        for (term, mode, data) in rows {
            statement
                .execute(params![
                    self.id,
                    normalize_query(term),
                    mode,
                    serde_json::to_string(data).map_err(err)?
                ])
                .map_err(err)?;
        }
        Ok(())
    }
    fn write_terms(&mut self, terms: &[TermRecord]) -> Result<(), String> {
        let mut statement=self.connection.prepare_cached("INSERT INTO terms(dictionary_id,term,reading,term_normalized,reading_normalized,definition_tags,rules,score,glossary,safe_glossary,sequence,term_tags) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)").map_err(err)?;
        for term in terms {
            statement
                .execute(params![
                    self.id,
                    term.term,
                    term.reading,
                    normalize_query(&term.term),
                    normalize_query(if term.reading.is_empty() {
                        &term.term
                    } else {
                        &term.reading
                    }),
                    serde_json::to_string(&term.definition_tags).map_err(err)?,
                    serde_json::to_string(&term.rules).map_err(err)?,
                    term.score,
                    serde_json::to_string(&term.glossary).map_err(err)?,
                    serde_json::to_string(&yomitan::safe_glossary(&serde_json::json!(
                        term.glossary
                    )))
                    .map_err(err)?,
                    term.sequence,
                    serde_json::to_string(&term.term_tags).map_err(err)?
                ])
                .map_err(err)?;
            self.terms += 1;
        }
        Ok(())
    }
    fn write_tags(&mut self, tags: &[TagRecord]) -> Result<(), String> {
        let mut statement=self.connection.prepare_cached("INSERT INTO tags(dictionary_id,name,category,sort_order,notes,score) VALUES(?1,?2,?3,?4,?5,?6)").map_err(err)?;
        for tag in tags {
            statement
                .execute(params![
                    self.id,
                    tag.name,
                    tag.category,
                    tag.order,
                    tag.notes,
                    tag.score
                ])
                .map_err(err)?;
        }
        Ok(())
    }
    fn stage_asset(&mut self, path: &str, reader: &mut dyn Read) -> Result<(), String> {
        if !yomitan::safe_path(path) {
            return Err("Unsafe asset path".into());
        }
        let mut bytes = Vec::new();
        reader
            .take(16_000_001)
            .read_to_end(&mut bytes)
            .map_err(err)?;
        if bytes.len() > 16_000_000 {
            return Err("Image exceeds 16 MB".into());
        }
        self.connection
            .execute(
                "INSERT INTO assets(dictionary_id,path,bytes) VALUES(?1,?2,?3)",
                params![self.id, path, bytes],
            )
            .map_err(err)?;
        Ok(())
    }
    fn write_warnings(&mut self, warnings: &[String]) -> Result<(), String> {
        self.connection
            .execute(
                "UPDATE dictionaries SET warnings=?2 WHERE id=?1",
                params![self.id, serde_json::to_string(warnings).map_err(err)?],
            )
            .map_err(err)?;
        Ok(())
    }
    fn publish(mut self) -> Result<(), String> {
        self.connection
            .execute(
                "UPDATE dictionaries SET term_count=?2 WHERE id=?1",
                params![self.id, self.terms],
            )
            .map_err(err)?;
        self.connection.execute_batch("COMMIT").map_err(err)?;
        self.published = true;
        Ok(())
    }
}
impl Drop for Sink {
    fn drop(&mut self) {
        if !self.published {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub running: bool,
    pub progress: ImportProgress,
    pub error: Option<String>,
    pub report: Option<ImportReport>,
}
#[derive(Default)]
pub struct Jobs {
    cancel: AtomicBool,
    state: Mutex<JobStatus>,
}
impl Jobs {
    pub fn begin(&self) -> Result<(), String> {
        let mut state = self.state.lock().map_err(err)?;
        if state.running {
            return Err("Another dictionary import is already running.".into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        *state = JobStatus {
            running: true,
            progress: ImportProgress {
                phase: "Choose a local ZIP".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        Ok(())
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
    pub fn status(&self) -> JobStatus {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn finish(&self, result: &Result<ImportReport, String>) {
        if let Ok(mut state) = self.state.lock() {
            state.running = false;
            match result {
                Ok(report) => {
                    state.progress.phase = "Import complete".into();
                    state.report = Some(report.clone());
                }
                Err(error) => state.error = Some(error.clone()),
            }
        }
    }
    pub fn import<R: Read + Seek>(
        &self,
        root: &Path,
        source: R,
        replace: Option<i64>,
    ) -> Result<ImportReport, String> {
        yomitan::canceled(&self.cancel)?;
        Storage::open(root)?.import(source, replace, &self.cancel, |progress| {
            if let Ok(mut state) = self.state.lock() {
                state.progress = progress.clone();
            }
        })
    }
}
