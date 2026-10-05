//! Indexed, bounded exact-query adapters for the three existing SQLite layouts.
use crate::chunk_lookup::{Entry, Provenance, Store, MAX_CANDIDATES};
use rusqlite::{params_from_iter, Connection};
use std::collections::BTreeSet;

#[derive(Clone, Copy)]
pub enum Schema {
    Desktop,
    Bundled,
    Imported,
}
pub struct SqliteStore<'a> {
    pub connection: &'a Connection,
    pub schema: Schema,
}

fn sql(schema: Schema, count: usize) -> String {
    let (table, term_index, reading_index, _scope) = match schema {
        Schema::Desktop => (
            "dictionary_entries",
            "idx_dictionary_entries_term_normalized",
            "idx_dictionary_entries_reading_normalized",
            "AND dictionary_id IN (SELECT id FROM dictionaries WHERE enabled=1)",
        ),
        Schema::Bundled => ("entries", "term_lookup", "reading_lookup", ""),
        Schema::Imported => (
            "terms",
            "terms_term_ranked",
            "terms_reading_ranked",
            "AND dictionary_id IN (SELECT id FROM dictionaries WHERE enabled=1)",
        ),
    };
    // Bound source scope and every key/source index seek before ranking.
    let values = vec!["(?)"; count].join(",");
    let scopes = match schema {
        Schema::Imported => "SELECT id,priority,title FROM dictionaries WHERE enabled=1 ORDER BY priority DESC,title,id LIMIT 32",
        Schema::Desktop => "SELECT id,0 AS priority,name AS title FROM dictionaries WHERE enabled=1 ORDER BY name,id LIMIT 32",
        Schema::Bundled => "SELECT 0 AS id,0 AS priority,'' AS title",
    };
    let seek = |column: &str, index: &str| {
        if matches!(schema, Schema::Bundled) {
            format!("SELECT id FROM {table} INDEXED BY {index} WHERE {column}=k.value LIMIT 12")
        } else {
            let score = if matches!(schema, Schema::Imported) {
                "candidate.score DESC,"
            } else {
                ""
            };
            let seek_order = if matches!(schema, Schema::Imported) {
                "ORDER BY score DESC,id"
            } else {
                "ORDER BY id"
            };
            format!("SELECT candidate.id FROM scopes d CROSS JOIN {table} candidate ON candidate.id IN
                (SELECT id FROM {table} INDEXED BY {index} WHERE {column}=k.value AND dictionary_id=d.id {seek_order} LIMIT 12)
                ORDER BY d.priority DESC,{score}d.title,candidate.id LIMIT 12")
        }
    };
    let term = seek("term_normalized", term_index);
    let reading = seek("reading_normalized", reading_index);
    let ids = format!(
        "WITH keys(value) AS (VALUES {values}), scopes AS ({scopes}), hits(id) AS MATERIALIZED (
        SELECT e.id FROM keys k CROSS JOIN {table} e ON e.id IN
          ({term})
        UNION
        SELECT e.id FROM keys k CROSS JOIN {table} e ON e.id IN
          ({reading}) LIMIT 4097)"
    );
    match schema {
        Schema::Desktop => format!("{ids} SELECT e.id,e.term,COALESCE(e.reading,''),e.definitions,e.part_of_speech,d.name,CAST(d.imported_at AS TEXT) FROM hits CROSS JOIN dictionary_entries e ON e.id=hits.id JOIN dictionaries d ON d.id=e.dictionary_id ORDER BY e.id"),
        Schema::Bundled => format!("{ids} SELECT e.id,e.term,COALESCE(e.reading,''),e.definitions,e.part_of_speech,'JMdict','jmdict-20260928-v2' FROM hits CROSS JOIN entries e ON e.id=hits.id ORDER BY e.id"),
        Schema::Imported => format!("{ids} SELECT e.id,e.term,e.reading,e.safe_glossary,e.rules,d.title,d.revision,e.definition_tags,e.term_tags,e.sequence,e.score,d.priority FROM hits CROSS JOIN terms e ON e.id=hits.id JOIN dictionaries d ON d.id=e.dictionary_id ORDER BY d.priority DESC,e.score DESC,e.id"),
    }
}

/// Map both JMdict abbreviations and expanded jmdict-eng POS descriptions.
pub fn jmdict_rules(tags: &[String]) -> Vec<String> {
    let mut rules = BTreeSet::new();
    for tag in tags {
        let lower = tag.to_lowercase();
        if lower.starts_with("v1") || lower.contains("ichidan") {
            rules.insert("v1".to_owned());
        }
        if lower.starts_with("v5") || lower.contains("godan") {
            let subtype = if lower.starts_with("v5") {
                lower.as_str()
            } else if lower.contains("iku/yuku") {
                "v5k-s"
            } else if lower.contains("u ending (special") {
                "v5u-s"
            } else if lower.contains("ku ending") {
                "v5k"
            } else if lower.contains("gu ending") {
                "v5g"
            } else if lower.contains("tsu ending") {
                "v5t"
            } else if lower.contains("su ending") {
                "v5s"
            } else if lower.contains("nu ending") {
                "v5n"
            } else if lower.contains("bu ending") {
                "v5b"
            } else if lower.contains("mu ending") {
                "v5m"
            } else if lower.contains("ru ending") {
                "v5r"
            } else if lower.contains("u ending") {
                "v5u"
            } else {
                "v5"
            };
            rules.insert(subtype.to_owned());
        }
        if lower.starts_with("vs") || lower.contains("suru") {
            rules.insert("vs".to_owned());
        }
        if lower == "vk" || lower.contains("kuru") {
            rules.insert("vk".to_owned());
        }
        if lower.starts_with("adj-i") || lower.contains("adjective (keiyoushi)") {
            rules.insert("adj-i".to_owned());
        }
    }
    rules.into_iter().collect()
}
fn json<T: serde::de::DeserializeOwned>(
    row: &rusqlite::Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(index, rusqlite::types::Type::Text, Box::new(e))
    })
}
impl Store for SqliteStore<'_> {
    fn query_batch(&mut self, keys: &[String]) -> Result<Vec<Entry>, String> {
        if keys.len() > MAX_CANDIDATES {
            return Err("Dictionary batch exceeds 512 keys".into());
        }
        if keys.iter().any(|key| key.len() > 512)
            || keys.iter().map(String::len).sum::<usize>() > 128 * 1024
        {
            return Err("Dictionary batch exceeds its bounded key bytes".into());
        }
        if keys.is_empty() {
            return Ok(vec![]);
        }
        if !matches!(self.schema, Schema::Bundled) {
            let count: i64 = self
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM dictionaries WHERE enabled=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if count > 32 {
                return Err("Dictionary lookup supports at most 32 enabled sources; disable dictionaries and retry".into());
            }
        }
        let imported = matches!(self.schema, Schema::Imported);
        let mut statement = self
            .connection
            .prepare(&sql(self.schema, keys.len()))
            .map_err(|e| e.to_string())?;
        let rows = statement
            .query_map(params_from_iter(keys), |r| {
                let title: String = r.get(5)?;
                let revision: String = r.get(6)?;
                let tags: Vec<String> = json(r, 4)?;
                Ok(Entry {
                    assets: Default::default(),
                    metadata: vec![],
                    id: r.get(0)?,
                    term: r.get(1)?,
                    reading: r.get(2)?,
                    provenance: Provenance {
                        source: format!(
                            "{}:{}",
                            if imported { "yomitan" } else { "builtin" },
                            title
                        ),
                        title,
                        revision,
                    },
                    rules: if imported {
                        tags.clone()
                    } else {
                        jmdict_rules(&tags)
                    },
                    glossary: json(r, 3)?,
                    definition_tags: if imported { json(r, 7)? } else { tags },
                    term_tags: if imported { json(r, 8)? } else { vec![] },
                    sequence: if imported { r.get(9)? } else { -1 },
                    score: if imported { r.get(10)? } else { 0.0 },
                    priority: if imported { r.get(11)? } else { 0 },
                })
            })
            .map_err(|e| e.to_string())?;
        let mut output = Vec::new();
        let mut bytes = 0usize;
        for row in rows {
            let entry = row.map_err(|e| e.to_string())?;
            bytes += serde_json::to_vec(&entry).map_err(|e| e.to_string())?.len();
            if bytes > 8 * 1024 * 1024 {
                return Err(
                    "Dictionary result exceeds 8 MiB; disable an oversized dictionary and retry"
                        .into(),
                );
            }
            output.push(entry);
            if output.len() > 4096 {
                return Err("Dictionary result exceeds 4096 rows; narrow the lookup or disable dictionaries".into());
            }
        }
        Ok(output)
    }
}
impl Store for crate::dictionary_storage::Storage {
    fn query_batch(&mut self, keys: &[String]) -> Result<Vec<Entry>, String> {
        crate::dictionary_storage::Storage::query_batch(self, keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn expanded_pos_preserves_godan_subclasses() {
        assert_eq!(
            jmdict_rules(&["Godan verb with tsu ending".into()]),
            ["v5t"]
        );
        assert_eq!(jmdict_rules(&["Godan verb with su ending".into()]), ["v5s"]);
        assert_eq!(jmdict_rules(&["v5g".into()]), ["v5g"]);
    }
    #[test]
    fn imported_zip_reopens_for_inflected_lookup_and_disabling() {
        use crate::{dictionary_storage::Storage, lookup::LookupRequest};
        use std::io::{Cursor, Write};
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let glossary = serde_json::json!([{"type":"structured-content","content":{"tag":"span","content":"eat"}}]);
        for (name, value) in [
            (
                "index.json",
                serde_json::json!({"title":"Generated","revision":"r3","format":3,"sequenced":true}),
            ),
            (
                "term_bank_1.json",
                serde_json::json!([["食べる", "たべる", "", "v1", 1, glossary, 10, ""]]),
            ),
        ] {
            writer
                .start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer
                .write_all(&serde_json::to_vec(&value).unwrap())
                .unwrap();
        }
        let root = tempfile::tempdir().unwrap();
        Storage::open(root.path())
            .unwrap()
            .import(
                writer.finish().unwrap(),
                None,
                &std::sync::atomic::AtomicBool::new(false),
                |_| {},
            )
            .unwrap();
        let mut store = Storage::open(root.path()).unwrap();
        let request = LookupRequest {
            text: "食べました".into(),
            offset: 0,
        };
        let response = crate::chunk_lookup::lookup(&request, &mut store).unwrap();
        assert_eq!(
            (response.matched_span.start, response.matched_span.end),
            (0, 5)
        );
        let entry = &response.groups[0].matches[0].entry;
        assert_eq!(entry.provenance.source, "yomitan:Generated");
        assert_eq!(entry.provenance.revision, "r3");
        assert_eq!(entry.glossary, crate::yomitan::safe_glossary(&glossary));
        store.update(store.list().unwrap()[0].id, false, 0).unwrap();
        assert!(crate::chunk_lookup::lookup(&request, &mut store)
            .unwrap()
            .groups
            .is_empty());
    }
    fn fixture(schema: Schema) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        match schema {
            Schema::Desktop => db.execute_batch(r#"CREATE TABLE dictionaries(id INTEGER PRIMARY KEY,name TEXT,imported_at INTEGER,enabled INTEGER); INSERT INTO dictionaries VALUES(1,'JMdict',123,1),(2,'Disabled',123,0); CREATE TABLE dictionary_entries(id INTEGER PRIMARY KEY,dictionary_id INTEGER,term TEXT,reading TEXT,definitions TEXT,part_of_speech TEXT,term_normalized TEXT,reading_normalized TEXT); CREATE INDEX idx_dictionary_entries_term_normalized ON dictionary_entries(term_normalized); CREATE INDEX idx_dictionary_entries_reading_normalized ON dictionary_entries(reading_normalized); INSERT INTO dictionary_entries VALUES(1,1,'食べる','たべる','["eat"]','["Ichidan verb"]','食べる','たべる'),(2,2,'食べる','たべる','[]','[]','食べる','たべる');"#).unwrap(),
            Schema::Bundled => db.execute_batch(r#"CREATE TABLE entries(id INTEGER PRIMARY KEY,term TEXT,reading TEXT,definitions TEXT,part_of_speech TEXT,term_normalized TEXT,reading_normalized TEXT); CREATE INDEX term_lookup ON entries(term_normalized); CREATE INDEX reading_lookup ON entries(reading_normalized); INSERT INTO entries VALUES(1,'食べる','たべる','["eat"]','["v1"]','食べる','たべる');"#).unwrap(),
            Schema::Imported => db.execute_batch(r#"CREATE TABLE dictionaries(id INTEGER PRIMARY KEY,title TEXT,revision TEXT,enabled INTEGER,priority INTEGER); INSERT INTO dictionaries VALUES(1,'Local','rev',1,9),(2,'Disabled','rev',0,100); CREATE TABLE terms(id INTEGER PRIMARY KEY,dictionary_id INTEGER,term TEXT,reading TEXT,term_normalized TEXT,reading_normalized TEXT,safe_glossary TEXT,rules TEXT,definition_tags TEXT,term_tags TEXT,sequence INTEGER,score REAL); CREATE INDEX terms_term_ranked ON terms(term_normalized,dictionary_id,score DESC,id); CREATE INDEX terms_reading_ranked ON terms(reading_normalized,dictionary_id,score DESC,id); INSERT INTO terms VALUES(1,1,'食べる','たべる','食べる','たべる','["eat"]','["v1"]','["verb"]','[]',4,5.0),(2,2,'食べる','たべる','食べる','たべる','[]','[]','[]','[]',0,0);"#).unwrap(),
        }
        db
    }
    #[test]
    fn adapters_use_exact_indexed_batches_and_enabled_sources() {
        for schema in [Schema::Desktop, Schema::Bundled, Schema::Imported] {
            let db = fixture(schema);
            let mut store = SqliteStore {
                connection: &db,
                schema,
            };
            let keys = vec!["たべる".into(), "食べる".into(), "食べ".into()];
            let rows = store.query_batch(&keys).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].rules, vec!["v1"]);
            assert_eq!(rows[0].glossary, serde_json::json!(["eat"]));
            assert!(store.query_batch(&["食べ".into()]).unwrap().is_empty());
            if matches!(schema, Schema::Imported) {
                assert_eq!(rows[0].priority, 9);
                assert_eq!(rows[0].sequence, 4);
            }
            let mut plan = db
                .prepare(&format!("EXPLAIN QUERY PLAN {}", sql(schema, keys.len())))
                .unwrap();
            let details = plan
                .query_map(params_from_iter(&keys), |r| r.get::<_, String>(3))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
                .join("\n");
            assert!(
                (details.contains("USING INDEX") || details.contains("USING COVERING INDEX")),
                "{details}"
            );
            assert!(details.contains("term_normalized=?"), "{details}");
            assert!(details.contains("reading_normalized=?"), "{details}");
            assert!(!details.contains("SCAN e"), "{details}");
        }
    }
    #[test]
    fn homophones_and_key_count_are_bounded() {
        let db = fixture(Schema::Bundled);
        for id in 2..200 {
            db.execute(
                "INSERT INTO entries VALUES(?1,'別','たべる','[]','[]','別','たべる')",
                [id],
            )
            .unwrap();
        }
        let mut store = SqliteStore {
            connection: &db,
            schema: Schema::Bundled,
        };
        assert_eq!(store.query_batch(&["たべる".into()]).unwrap().len(), 12);
        assert!(store.query_batch(&vec!["x".into(); 513]).is_err());
    }
    #[test]
    fn high_priority_source_survives_large_homophone_bucket() {
        let db = fixture(Schema::Imported);
        for id in 3..80 {
            db.execute("INSERT INTO terms VALUES(?1,1,'食べる','たべる','食べる','たべる','[]','[]','[]','[]',0,0)",[id]).unwrap();
        }
        db.execute_batch("UPDATE dictionaries SET enabled=1 WHERE id=2")
            .unwrap();
        let mut store = SqliteStore {
            connection: &db,
            schema: Schema::Imported,
        };
        let rows = store.query_batch(&["たべる".into()]).unwrap();
        assert_eq!(rows.len(), 12);
        assert_eq!(rows[0].provenance.title, "Disabled");
        assert_eq!(rows[0].priority, 100);
    }
    #[test]
    fn late_high_score_sense_survives_bounded_seek() {
        let db = fixture(Schema::Imported);
        for id in 3..100 {
            db.execute("INSERT INTO terms VALUES(?1,1,'食べる','たべる','食べる','たべる','[]','[]','[]','[]',0,0)",[id]).unwrap();
        }
        db.execute("UPDATE terms SET score=999 WHERE id=99", [])
            .unwrap();
        let mut store = SqliteStore {
            connection: &db,
            schema: Schema::Imported,
        };
        for key in ["食べる", "たべる"] {
            let rows = store.query_batch(&[key.into()]).unwrap();
            assert_eq!(rows.len(), 12);
            assert_eq!(rows[0].score, 999.0);
        }
    }
    #[test]
    fn total_rows_and_payload_bytes_fail_explicitly() {
        let db = fixture(Schema::Bundled);
        let keys = (0..342).map(|i| format!("key{i}")).collect::<Vec<_>>();
        let mut id = 100;
        for key in &keys {
            for _ in 0..12 {
                db.execute(
                    "INSERT INTO entries VALUES(?1,?2,'','[]','[]',?2,'')",
                    rusqlite::params![id, key],
                )
                .unwrap();
                id += 1;
            }
        }
        let mut store = SqliteStore {
            connection: &db,
            schema: Schema::Bundled,
        };
        assert!(store.query_batch(&keys).unwrap_err().contains("4096 rows"));
        let glossary = serde_json::to_string(&vec!["x".repeat(750_000)]).unwrap();
        db.execute(
            "UPDATE entries SET definitions=?1 WHERE term_normalized='key0'",
            [glossary],
        )
        .unwrap();
        assert!(store
            .query_batch(&["key0".into()])
            .unwrap_err()
            .contains("8 MiB"));
        assert!(store
            .query_batch(&["x".repeat(513)])
            .unwrap_err()
            .contains("key bytes"));
    }
}
