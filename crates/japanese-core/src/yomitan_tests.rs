use crate::{
    dictionary::{DictionaryImportSink, DictionaryManifest, TagRecord, TermRecord},
    yomitan,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    io::{Cursor, Read, Write},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

fn archive(files: Vec<(&str, Vec<u8>)>) -> Cursor<Vec<u8>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    writer.finish().unwrap()
}
fn fixture(glossary: Value) -> Cursor<Vec<u8>> {
    archive(vec![
        (
            "index.json",
            serde_json::to_vec(
                &json!({"title":"Generated fixture", "revision":"1", "format":3, "sequenced":true}),
            )
            .unwrap(),
        ),
        (
            "term_bank_1.json",
            serde_json::to_vec(&json!([[
                "猫", "ねこ", null, "v1", 1, glossary, 42, "common"
            ]]))
            .unwrap(),
        ),
        (
            "tag_bank_1.json",
            serde_json::to_vec(&json!([["common", "frequency", 0, "Common word", 1]])).unwrap(),
        ),
    ])
}
#[derive(Default)]
struct Collected {
    terms: Vec<TermRecord>,
    tags: Vec<TagRecord>,
    published: bool,
}
struct Sink(Rc<RefCell<Collected>>);
impl DictionaryImportSink for Sink {
    fn write_terms(&mut self, terms: &[TermRecord]) -> Result<(), String> {
        self.0.borrow_mut().terms.extend_from_slice(terms);
        Ok(())
    }
    fn write_tags(&mut self, tags: &[TagRecord]) -> Result<(), String> {
        self.0.borrow_mut().tags.extend_from_slice(tags);
        Ok(())
    }
    fn stage_asset(&mut self, _: &str, _: &mut dyn Read) -> Result<(), String> {
        Ok(())
    }
    fn publish(self) -> Result<(), String> {
        self.0.borrow_mut().published = true;
        Ok(())
    }
}
#[test]
fn imports_tuple_fields_and_retains_untrusted_content_as_data() {
    let glossary = json!([{"type":"structured-content", "content":{"tag":"span","content":"猫", "style":{"color":"red"},"onclick":"evil()"}}, ["to eat",["v1"]]]);
    let state = Rc::new(RefCell::new(Collected::default()));
    let report = yomitan::import(
        fixture(glossary.clone()),
        |_: &DictionaryManifest| Ok(Sink(state.clone())),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let state = state.borrow();
    assert!(state.published);
    assert_eq!(report.terms, 1);
    assert_eq!(report.tags, 1);
    assert_eq!(
        state.terms[0].glossary,
        glossary.as_array().unwrap().clone()
    );
    assert_eq!(state.terms[0].rules, ["v1"]);
    assert_eq!(state.terms[0].sequence, 42);
    assert_eq!(state.tags[0].notes, "Common word");
    let safe = yomitan::safe_glossary(&glossary);
    assert!(safe[0]["content"].get("style").is_none());
    assert!(safe[0]["content"].get("onclick").is_none());
}
#[test]
fn unsafe_archive_paths_are_rejected_before_sink_creation() {
    for path in [
        "../escaped.png",
        "/absolute.png",
        "C:/drive.png",
        "folder\\file.png",
        "NUL.png",
        "folder/trailing. ",
    ] {
        let mut created = false;
        let result = yomitan::import(
            archive(vec![(path, vec![])]),
            |_| {
                created = true;
                Ok(Sink(Rc::default()))
            },
            &AtomicBool::new(false),
            |_| {},
        );
        assert!(result.is_err(), "Accepted {path}");
        assert!(!created);
    }
}
#[test]
fn malformed_deinflection_and_missing_asset_never_publish() {
    for glossary in [
        json!([["eat", 12]]),
        json!([{"type":"image", "path":"missing.png"}]),
    ] {
        let state = Rc::new(RefCell::new(Collected::default()));
        assert!(yomitan::import(
            fixture(glossary),
            |_| Ok(Sink(state.clone())),
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err());
        assert!(!state.borrow().published);
    }
}
#[test]
fn cancellation_at_publication_does_not_publish() {
    let state = Rc::new(RefCell::new(Collected::default()));
    let cancel = AtomicBool::new(false);
    let result = yomitan::import(
        fixture(json!(["cat"])),
        |_| Ok(Sink(state.clone())),
        &cancel,
        |p| {
            if p.phase == "Publishing" {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    );
    assert!(result.is_err());
    assert!(!state.borrow().published);
}
#[test]
fn safe_content_preserves_text_but_cannot_load_remote_images_or_links() {
    let value = json!({"type":"structured-content","content":[{"tag":"a","href":"https://example.invalid","content":"text"},{"tag":"img","path":"https://example.invalid/a.png"},{"tag":"script","content":"bad"}]});
    let safe = yomitan::safe_glossary(&value);
    assert_eq!(safe["content"][0]["content"], "text");
    assert!(safe["content"][0].get("href").is_none());
    assert!(safe["content"][1].get("path").is_none());
    assert!(safe["content"][2].get("unsupported").is_some());
}

#[test]
#[ignore = "Opt-in, read-only user ZIP compatibility check via TMW_DICTIONARY_FIXTURE"]
fn supplied_dictionary_imports_without_changing_source() {
    let path = std::env::var_os("TMW_DICTIONARY_FIXTURE")
        .expect("Set TMW_DICTIONARY_FIXTURE to the local ZIP path");
    let file = std::fs::File::open(&path).unwrap();
    let before = file.metadata().unwrap();
    let root = tempfile::tempdir().unwrap();
    let storage = crate::dictionary_storage::Storage::open(root.path()).unwrap();
    let report = storage
        .import(file, None, &AtomicBool::new(false), |_| {})
        .unwrap();
    assert!(report.terms > 0);
    let after = std::fs::metadata(path).unwrap();
    assert_eq!(before.len(), after.len());
    assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    assert_eq!(
        crate::dictionary_storage::Storage::open(root.path())
            .unwrap()
            .list()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn replacement_failure_and_cancel_preserve_existing_dictionary_and_settings() {
    use crate::dictionary_storage::Storage;
    let root = tempfile::tempdir().unwrap();
    Storage::open(root.path())
        .unwrap()
        .import(
            fixture(json!(["original"])),
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let original = Storage::open(root.path())
        .unwrap()
        .list()
        .unwrap()
        .remove(0);
    Storage::open(root.path())
        .unwrap()
        .update(original.id, false, 17)
        .unwrap();
    for glossary in [
        json!([{"type":"image","path":"absent.png"}]),
        json!(["replacement"]),
    ] {
        let cancel = AtomicBool::new(false);
        let result = Storage::open(root.path()).unwrap().import(
            fixture(glossary),
            Some(original.id),
            &cancel,
            |p| {
                if p.phase == "Publishing" {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(result.is_err());
        let current = Storage::open(root.path())
            .unwrap()
            .list()
            .unwrap()
            .remove(0);
        assert_eq!(current.id, original.id);
        assert_eq!(current.term_count, 1);
        assert!(!current.enabled);
        assert_eq!(current.priority, 17);
        let database =
            rusqlite::Connection::open(root.path().join("dictionaries.sqlite3")).unwrap();
        let glossary: String = database
            .query_row("SELECT glossary FROM terms", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&glossary).unwrap(),
            json!(["original"])
        );
    }
    Storage::open(root.path())
        .unwrap()
        .import(
            fixture(json!(["replacement"])),
            Some(original.id),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let current = Storage::open(root.path())
        .unwrap()
        .list()
        .unwrap()
        .remove(0);
    assert_eq!(current.id, original.id);
    assert!(!current.enabled);
    assert_eq!(current.priority, 17);
    Storage::open(root.path())
        .unwrap()
        .remove(original.id)
        .unwrap();
    let database = rusqlite::Connection::open(root.path().join("dictionaries.sqlite3")).unwrap();
    for table in ["dictionaries", "terms", "tags", "assets"] {
        let count: i64 = database
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn local_image_is_preserved_in_database_with_original_and_safe_json() {
    use crate::dictionary_storage::Storage;
    let root = tempfile::tempdir().unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1, 1)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    let glossary = json!([{"type":"structured-content","content":{"tag":"img","path":"gaiji/猫.png","height":1,"sizeUnits":"em"}}]);
    let files = vec![
        (
            "index.json",
            serde_json::to_vec(&json!({"title":"Image fixture","revision":"1","format":3}))
                .unwrap(),
        ),
        (
            "term_bank_1.json",
            serde_json::to_vec(&json!([["猫", "", null, "", 0, glossary, 1, ""]])).unwrap(),
        ),
        ("gaiji/猫.png", bytes.clone()),
    ];
    let report = Storage::open(root.path())
        .unwrap()
        .import(archive(files), None, &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(report.assets, 1);
    #[cfg(feature = "tokenizer")]
    {
        let mut storage = Storage::open(root.path()).unwrap();
        let mut response = crate::chunk_lookup::lookup(
            &crate::lookup::LookupRequest {
                text: "猫".into(),
                offset: 0,
            },
            &mut storage,
        )
        .unwrap();
        storage.decorate_response(&mut response).unwrap();
        assert!(response.groups[0].matches[0].entry.assets["gaiji/猫.png"]
            .starts_with("data:image/png;base64,"));
    }
    let database = rusqlite::Connection::open(root.path().join("dictionaries.sqlite3")).unwrap();
    let stored: Vec<u8> = database
        .query_row(
            "SELECT bytes FROM assets WHERE path='gaiji/猫.png'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, bytes);
    let (raw, safe): (String, String) = database
        .query_row("SELECT glossary,safe_glossary FROM terms", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), glossary);
    assert_eq!(
        serde_json::from_str::<Value>(&safe).unwrap()[0]["content"]["path"],
        "gaiji/猫.png"
    );
}

#[cfg(feature = "tokenizer")]
#[test]
fn metadata_only_source_reopens_and_enriches_matching_reading() {
    use crate::{dictionary_storage::Storage, lookup::LookupRequest};
    let root = tempfile::tempdir().unwrap();
    Storage::open(root.path())
        .unwrap()
        .import(
            fixture(json!(["cat"])),
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let metadata=archive(vec![("index.json",serde_json::to_vec(&json!({"title":"Frequency and pitch","revision":"1","format":3})).unwrap()),("term_meta_bank_1.json",serde_json::to_vec(&json!([["猫","freq",{"reading":"ねこ","frequency":{"value":3,"displayValue":"top 3"}}],["猫","pitch",{"reading":"ねこ","pitches":[{"position":1,"tags":["n"]}]}],["猫","freq",{"reading":"びょう","frequency":99}]])).unwrap())]);
    let report = Storage::open(root.path())
        .unwrap()
        .import(metadata, None, &AtomicBool::new(false), |_| {})
        .unwrap();
    assert_eq!(report.metadata, 3);
    let mut store = Storage::open(root.path()).unwrap();
    let mut response = crate::chunk_lookup::lookup(
        &LookupRequest {
            text: "猫".into(),
            offset: 0,
        },
        &mut store,
    )
    .unwrap();
    store.decorate_response(&mut response).unwrap();
    let entries = response
        .groups
        .iter()
        .flat_map(|g| g.matches.iter())
        .collect::<Vec<_>>();
    assert!(!entries.is_empty());
    assert_eq!(entries[0].entry.metadata.len(), 2);
    let id = store
        .list()
        .unwrap()
        .into_iter()
        .find(|s| s.title == "Frequency and pitch")
        .unwrap()
        .id;
    store.update(id, false, 0).unwrap();
    let mut response = crate::chunk_lookup::lookup(
        &LookupRequest {
            text: "猫".into(),
            offset: 0,
        },
        &mut store,
    )
    .unwrap();
    store.decorate_response(&mut response).unwrap();
    assert!(response.groups[0].matches[0].entry.metadata.is_empty());
}

#[test]
#[ignore = "opt-in large generated multi-dictionary import observation"]
fn large_multi_dictionary_import_reopens_offline() {
    use crate::dictionary_storage::Storage;
    let dir = tempfile::tempdir().unwrap();
    let start = std::time::Instant::now();
    let storage = Storage::open(dir.path()).unwrap();
    for source in 0..3 {
        let rows: Vec<Value> = (0..20000)
            .map(|i| {
                json!([
                    format!("語{i}"),
                    format!("ご{i}"),
                    "",
                    "",
                    i % 10,
                    ["Generated definition"],
                    i,
                    ""
                ])
            })
            .collect();
        let zip = archive(vec![
            (
                "index.json",
                serde_json::to_vec(
                    &json!({"title":format!("Large fixture {source}"),"revision":"1","format":3}),
                )
                .unwrap(),
            ),
            ("term_bank_1.json", serde_json::to_vec(&rows).unwrap()),
        ]);
        Storage::open(dir.path())
            .unwrap()
            .import(zip, None, &AtomicBool::new(false), |_| {})
            .unwrap();
    }
    let elapsed = start.elapsed();
    drop(storage);
    let storage = Storage::open(dir.path()).unwrap();
    assert_eq!(
        storage
            .list()
            .unwrap()
            .iter()
            .map(|d| d.term_count)
            .sum::<i64>(),
        60000
    );
    let rows = storage.query_batch(&["語19999".into()]).unwrap();
    assert_eq!(rows.len(), 3);
    println!("Generated 3-source/60000-term import including fixture creation: {:?}; {:.0} terms/s; database bytes {}", elapsed, 60000.0 / elapsed.as_secs_f64(), std::fs::metadata(dir.path().join("dictionaries.sqlite3")).unwrap().len());
}

#[test]
fn existing_version_one_store_gains_ranked_indexes_without_losing_settings() {
    use crate::dictionary_storage::Storage;
    let dir = tempfile::tempdir().unwrap();
    Storage::open(dir.path())
        .unwrap()
        .import(
            fixture(json!(["cat"])),
            None,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
    let storage = Storage::open(dir.path()).unwrap();
    let id = storage.list().unwrap()[0].id;
    storage.update(id, false, 7).unwrap();
    drop(storage);
    let db = rusqlite::Connection::open(dir.path().join("dictionaries.sqlite3")).unwrap();
    db.execute_batch("DROP INDEX terms_term_ranked; DROP INDEX terms_reading_ranked;")
        .unwrap();
    drop(db);
    let storage = Storage::open(dir.path()).unwrap();
    let sources = storage.list().unwrap();
    assert_eq!(sources[0].id, id);
    assert_eq!(sources[0].priority, 7);
    assert!(!sources[0].enabled);
    assert!(storage.query_batch(&["猫".into()]).unwrap().is_empty());
    storage.update(id, true, 7).unwrap();
    assert_eq!(storage.query_batch(&["猫".into()]).unwrap().len(), 1);
}

#[test]
fn declared_expansion_and_corrupt_archives_fail_before_publication() {
    let mut oversized = archive(vec![("term_bank_1.json", b"[]".to_vec())]).into_inner();
    let central = oversized
        .windows(4)
        .position(|w| w == b"PK\x01\x02")
        .unwrap();
    oversized[central + 24..central + 28].copy_from_slice(&64_000_001u32.to_le_bytes());
    for bytes in [oversized, b"broken ZIP".to_vec()] {
        let mut created = false;
        assert!(yomitan::import(
            Cursor::new(bytes),
            |_| {
                created = true;
                Ok(Sink(Rc::default()))
            },
            &AtomicBool::new(false),
            |_| {}
        )
        .is_err());
        assert!(!created);
    }
}

#[cfg(feature = "tokenizer")]
#[test]
fn result_limit_persists_and_caps_across_groups() {
    use crate::{dictionary_storage::Storage, lookup::LookupRequest};
    let root = tempfile::tempdir().unwrap();
    let mut store = Storage::open(root.path()).unwrap();
    store.import(archive(vec![
        ("index.json", serde_json::to_vec(&json!({"title":"Limit fixture","revision":"1","format":3})).unwrap()),
        ("term_bank_1.json", serde_json::to_vec(&json!([
            ["猫","ねこ","","",1,["cat"],1,""],
            ["猫","ねこ","","",2,["feline"],2,""],
            ["猫","びょう","","",0,["alternate"],3,""]
        ])).unwrap())
    ]), None, &AtomicBool::new(false), |_| {}).unwrap();
    let id = store.list().unwrap()[0].id;
    store.set_result_limit(id, 1).unwrap();
    assert!(store.set_result_limit(id, -1).is_err());
    drop(store);
    let mut store = Storage::open(root.path()).unwrap();
    assert_eq!(store.list().unwrap()[0].result_limit, 1);
    let request = LookupRequest { text: "猫".into(), offset: 0 };
    let mut response = crate::chunk_lookup::lookup(&request, &mut store).unwrap();
    store.limit_response(&mut response).unwrap();
    assert_eq!(response.groups.iter().map(|g| g.matches.len()).sum::<usize>(), 1);
    store.set_result_limit(id, 0).unwrap();
    let mut response = crate::chunk_lookup::lookup(&request, &mut store).unwrap();
    store.limit_response(&mut response).unwrap();
    assert_eq!(response.groups.iter().map(|g| g.matches.len()).sum::<usize>(), 3);
}
