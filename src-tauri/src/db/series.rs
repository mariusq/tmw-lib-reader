use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(super) fn revision() -> String {
    static REVISION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    REVISION.get_or_init(||format!("series-v1-{:x}", Sha256::digest(include_bytes!("series_candidates.json")))).clone()
}

pub(super) fn portable_membership(connection: &Connection, id: i64, title: &str, author: &str, series: &str, volume: &str) -> rusqlite::Result<serde_json::Value> {
    if !series.is_empty() {
        return Ok(serde_json::json!({"version":1,"key":format!("metadata:{:x}",Sha256::digest(series.as_bytes())),"name":series,"confidence":1,"order":volume.parse::<f64>().ok(),"volume":volume}));
    }
    use rusqlite::OptionalExtension;
    connection.query_row("SELECT c.group_key,c.name,c.confidence,c.volume_order FROM series_candidates c JOIN books b ON b.file_path=c.path WHERE b.id=?1 AND c.title=?2 AND c.author=?3", rusqlite::params![id,title,author], |row| Ok(serde_json::json!({"version":1,"key":format!("proposal:{}",row.get::<_,String>(0)?),"name":row.get::<_,String>(1)?,"confidence":row.get::<_,i64>(2)?,"order":row.get::<_,i64>(3)?,"volume":volume}))).optional().map(|value|value.unwrap_or(serde_json::Value::Null))
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesBrowseOptions {
    #[serde(default)]
    pub consolidate: bool,
    pub confidence: Option<String>,
    pub selected_key: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Candidate {
    path: String,
    title: String,
    author: String,
    group_key: String,
    name: String,
    confidence: i64,
    volume_order: i64,
}

pub(super) fn initialize(connection: &mut Connection) -> rusqlite::Result<()> {
    let candidates: Vec<Candidate> = serde_json::from_str(include_str!("series_candidates.json"))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let transaction = connection.transaction()?;
    transaction.execute_batch("CREATE TEMP TABLE series_candidates(path TEXT PRIMARY KEY, title TEXT NOT NULL, author TEXT NOT NULL, group_key TEXT NOT NULL, name TEXT NOT NULL, confidence INTEGER NOT NULL, volume_order INTEGER NOT NULL DEFAULT 0, normalized_name TEXT NOT NULL DEFAULT '');")?;
    {
        let mut insert = transaction.prepare("INSERT INTO series_candidates VALUES(?1,?2,?3,?4,?5,?6,?7,?8)")?;
        for candidate in candidates {
            insert.execute(params![candidate.path, candidate.title, candidate.author, candidate.group_key, candidate.name, candidate.confidence, candidate.volume_order, crate::services::metadata::normalize_for_search(&candidate.name)])?;
        }
    }
    transaction.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::Database, models::{book::{BrowseBooksRequest, ExtractedBookMetadata, NewBook, BookOverride}, library_root::NewLibraryRoot}};

    #[test]
    fn portable_membership_has_stable_identity_and_preserves_explicit_overrides() {
        let mut c=Connection::open_in_memory().unwrap();
        initialize(&mut c).unwrap();
        c.execute_batch("CREATE TABLE books(id INTEGER PRIMARY KEY,file_path TEXT); INSERT INTO books VALUES(7,'fixture/series.epub'); INSERT INTO series_candidates VALUES('fixture/series.epub','Story 2','Author','stable-fixture','Story',2,2,'Story');").unwrap();
        let value=portable_membership(&c,7,"Story 2","Author","","").unwrap();
        assert_eq!(value["key"],"proposal:stable-fixture");
        assert_eq!(value["confidence"],2);
        assert!(portable_membership(&c,7,"Edited","Author","","").unwrap().is_null());
        let manual=portable_membership(&c,7,"Edited","Author","My series","3").unwrap();
        assert_eq!(manual["confidence"],1);
        assert_eq!(manual["name"],"My series");
        assert_eq!(manual["key"],portable_membership(&c,999,"Unrelated","Other","My series","4").unwrap()["key"]);
        assert!(revision().starts_with("series-v1-"));
    }

    #[test]
    #[ignore = "opt-in read-only catalog diagnosis; set TMW_SERIES_DIAGNOSTIC_CATALOG"]
    fn inspect_catalog_series_search_read_only() {
        let path = std::env::var("TMW_SERIES_DIAGNOSTIC_CATALOG").unwrap();
        let mut connection = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        super::super::duplicates::initialize(&mut connection).unwrap();
        initialize(&mut connection).unwrap();
        let db = Database { connection: std::sync::Mutex::new(connection) };
        let request: BrowseBooksRequest = serde_json::from_value(serde_json::json!({"readingStatus":null,"libraryRootId":null,"tagId":null,"collectionId":null,"needsMetadata":false,"hideDuplicateTitles":true,"duplicateFiltering":"high","query":"tokidoki","sort":"title","offset":0,"limit":80})).unwrap();
        let plain = db.browse_books(&request).unwrap();
        let options=SeriesBrowseOptions { consolidate:true, confidence:Some("medium".into()), selected_key:None };
        let grouped=db.browse_series_books(&request,Some(&options)).unwrap();
        println!("Ungrouped rows: {}; grouped rows: {}; series cards: {}",plain.len(),grouped.len(),grouped.iter().filter(|row| row.series_count>1).count());
        for row in &grouped {
            println!("id={} count={} key={:?}",row.id,row.series_count,row.series_key);
        }
        assert!(grouped.len()<plain.len());
        assert!(grouped.iter().any(|row| row.series_title.as_deref()==Some("時々ボソッとロシア語でデレる隣のアーリャさん") && row.series_count>1));
    }

    #[test]
    fn duplicate_copies_do_not_lower_embedded_series_confidence() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let (copies, minimum, maximum): (i64,i64,i64) = connection.query_row(
            "SELECT count(*),min(confidence),max(confidence) FROM series_candidates WHERE name=?1",
            ["時々ボソッとロシア語でデレる隣のアーリャさん"],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        ).unwrap();
        assert!(copies>11, "Multiple copies must remain represented");
        assert_eq!(minimum,1, "Copy ambiguity is independent of core series confidence");
        assert!(maximum<=2, "Related same-author members may be medium confidence");
    }

    #[test]
    fn series_pagination_confidence_filters_duplicates_and_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("catalog.sqlite3")).unwrap();
        let root = db.add_library_root(NewLibraryRoot { path: "fixture", display_name: "Fixture" }).unwrap();
        let mut ids = Vec::new();
        for (index, title) in ["Story 1", "Story 2", "Story 2", "Other"].iter().enumerate() {
            let path = format!("fixture/{index}.epub");
            let book = db.add_book(NewBook { library_root_id: root.id, file_path: &path, parent_folder_path: "fixture", file_name: &path, file_size: 1, modified_time: 1 }).unwrap();
            db.save_extracted_metadata(book.id, &ExtractedBookMetadata { title: Some((*title).into()), creator: Some("Author".into()), ..Default::default() }).unwrap();
            ids.push(book.id);
            if index < 3 {
                db.connection.lock().unwrap().execute("INSERT INTO series_candidates VALUES(?1,?2,'Author','fixture','Family label',2,?3,'family label')", params![path,title,index as i64]).unwrap();
            }
        }
        let mut request: BrowseBooksRequest = serde_json::from_value(serde_json::json!({"readingStatus":null,"libraryRootId":null,"tagId":null,"collectionId":null,"needsMetadata":false,"hideDuplicateTitles":false,"query":"","sort":"title","offset":0,"limit":1})).unwrap();
        let mut options = SeriesBrowseOptions { consolidate: true, confidence: Some("medium".into()), selected_key: None };
        let first = db.browse_series_books(&request,Some(&options)).unwrap();
        request.offset = 1;
        let second = db.browse_series_books(&request,Some(&options)).unwrap();
        assert_eq!(first.len(),1);
        assert_eq!(second.len(),1);
        assert_eq!(first[0].series_count,3);
        request.offset=2;
        assert!(db.browse_series_books(&request,Some(&options)).unwrap().is_empty());
        request.offset=0; request.limit=80;
        options.confidence=Some("high".into());
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),4);
        options.confidence=Some("medium".into());
        request.query="Family label".into();
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap()[0].series_count,3);
        options.consolidate=false;
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),3);
        options.confidence=Some("high".into());
        assert!(db.browse_series_books(&request,Some(&options)).unwrap().is_empty());
        options.confidence=Some("medium".into()); options.consolidate=true;
        request.query="Fa".into();
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap()[0].series_count,3);
        request.query="Story".into();
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap()[0].series_count,3);
        request.hide_duplicate_titles=true;
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap()[0].series_count,2);
        options.selected_key=Some("proposal:fixture".into());
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),2);
        request.hide_duplicate_titles=false;
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),3);
        db.save_book_overrides(ids[0], &BookOverride { title:Some("Changed".into()),..Default::default() }).unwrap();
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),2);
        for id in &ids[..2] {
            db.save_book_overrides(*id, &BookOverride { series_name:Some("Manual series".into()),..Default::default() }).unwrap();
        }
        request.query.clear(); options.selected_key=Some("metadata:Manual series".into());
        assert_eq!(db.browse_series_books(&request,Some(&options)).unwrap().len(),2);
        assert_eq!(db.browse_books(&request).unwrap().len(),4);
    }
}
