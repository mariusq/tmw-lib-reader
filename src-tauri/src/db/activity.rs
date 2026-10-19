use super::Database;
use rusqlite::params;
use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityCheckpoint {
    #[serde(default)] pub generation: Option<String>,
    pub book_id: i64,
    pub day: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub milliseconds: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDay {
    pub lookups: i64,
    pub day: String,
    pub milliseconds: i64,
    pub characters: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityBook {
    pub lookups: i64,
    pub id: i64,
    pub title: String,
    pub characters: i64,
    pub milliseconds: i64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySummary {
    pub enabled: bool,
    pub days: Vec<ActivityDay>,
    pub books: Vec<ActivityBook>,
    pub finished: i64,
    pub streak: i64,
    pub longest_streak: i64,
}
pub(crate) fn valid_day(day: &str) -> bool {
    let bytes = day.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year = day[..4].parse::<u32>().unwrap_or(0);
    let month = day[5..7].parse::<u32>().unwrap_or(0);
    let date = day[8..10].parse::<u32>().unwrap_or(0);
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    year > 0 && date > 0 && date <= max
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CharacterCheckpoint {
    #[serde(default)] pub generation: Option<String>,
    pub book_id: i64,
    pub day: String,
    pub content_key: String,
    pub spine: i64,
    pub start_offset: i64,
    pub end_offset: i64,
}
pub(crate) fn content_key(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<String> {
    connection.query_row("SELECT COALESCE((SELECT version FROM companion_content_versions WHERE book_id=b.id AND source_size=b.file_size AND source_modified=b.modified_time),CAST(b.file_size AS TEXT)||':'||CAST(b.modified_time AS TEXT)) FROM books b WHERE b.id=?1",[id],|r|r.get(0))
}
impl Database {
    pub fn record_characters(&self, request: CharacterCheckpoint) -> rusqlite::Result<i64> {
        if !valid_day(&request.day)
            || request.spine < 0
            || request.spine > 100000
            || request.start_offset < 0
            || request.end_offset <= request.start_offset
            || request.end_offset > 100000000
            || request.end_offset - request.start_offset > 100000
            || request.content_key.len() > 128
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "Invalid character interval".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.transaction()?;
        if request.generation.as_ref().is_some_and(|g|super::activity_sync::epoch(&tx).ok().as_ref()!=Some(g)) {return Ok(0)}
        let enabled: bool = tx.query_row(
            "SELECT value='true' FROM app_settings WHERE key='reading_activity_enabled'",
            [],
            |r| r.get(0),
        )?;
        if !enabled {
            return Ok(0);
        }
        if content_key(&tx, request.book_id)? != request.content_key {
            return Err(rusqlite::Error::InvalidParameterName(
                "Book content changed; character estimate discarded".into(),
            ));
        }
        let before:i64=tx.query_row("SELECT COALESCE(SUM(characters),0) FROM reading_activity WHERE book_id=?1",[request.book_id],|r|r.get(0))?;
        super::activity_sync::local(&tx,Some(request.book_id),serde_json::json!({"id":uuid::Uuid::new_v4().to_string(),"kind":"characters","day":request.day,"version":request.content_key,"spine":request.spine,"startOffset":request.start_offset,"endOffset":request.end_offset}))?;
        let after:i64=tx.query_row("SELECT COALESCE(SUM(characters),0) FROM reading_activity WHERE book_id=?1",[request.book_id],|r|r.get(0))?;
        let added=after-before;
        tx.commit()?;
        Ok(added)
    }

    pub fn record_activity(&self, request: ActivityCheckpoint) -> rusqlite::Result<()> {
        if !valid_day(&request.day)
            || request.start_ms < 0
            || request.end_ms <= request.start_ms
            || request.end_ms - request.start_ms > 15_000
            || !(1..=15_000).contains(&request.milliseconds)
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "Invalid reading interval".into(),
            ));
        }
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.transaction()?;
        if request.generation.as_ref().is_some_and(|g|super::activity_sync::epoch(&tx).ok().as_ref()!=Some(g)) {return Ok(())}
        let enabled: String = tx.query_row(
            "SELECT value FROM app_settings WHERE key='reading_activity_enabled'",
            [],
            |r| r.get(0),
        )?;
        if enabled != "true" {
            return Ok(());
        }
        super::activity_sync::local(&tx,Some(request.book_id),serde_json::json!({"id":format!("desktop:{}:{}",request.start_ms,request.end_ms),"kind":"time","day":request.day,"startMs":request.start_ms,"endMs":request.end_ms,"milliseconds":request.milliseconds}))?;
        tx.commit()
    }
    pub fn reading_activity(
        &self,
        start_day: &str,
        end_day: &str,
        selected_day: &str,
        week_start: i64,
    ) -> rusqlite::Result<ActivitySummary> {
        if !valid_day(start_day) || !valid_day(end_day) || !valid_day(selected_day) {
            return Err(rusqlite::Error::InvalidParameterName(
                "Invalid calendar day".into(),
            ));
        }
        let connection = self.connection.lock().expect("database mutex poisoned");
        let enabled = connection.query_row(
            "SELECT value='true' FROM app_settings WHERE key='reading_activity_enabled'",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = connection.prepare("SELECT day,SUM(milliseconds),SUM(characters),SUM(lookups) FROM (SELECT day,milliseconds,characters,0 AS lookups FROM reading_activity UNION ALL SELECT day,0,0,lookups FROM reading_lookup_activity) WHERE day BETWEEN ?1 AND ?2 GROUP BY day ORDER BY day LIMIT 366")?;
        let days = stmt
            .query_map(params![start_day, end_day], |r| {
                Ok(ActivityDay {
                    day: r.get(0)?,
                    milliseconds: r.get(1)?,
                    characters: r.get(2)?,
                    lookups: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = connection.prepare("SELECT b.id,COALESCE(NULLIF(o.title,''),NULLIF(b.discovered_title,''),b.file_name),SUM(a.milliseconds),SUM(a.characters),(SELECT COUNT(*) FROM activity_events e WHERE json_extract(e.json,'$.bookId')=c.public_id AND json_extract(e.json,'$.day')=?1 AND json_extract(e.json,'$.kind')='lookup') FROM reading_activity a JOIN books b ON b.id=a.book_id LEFT JOIN book_overrides o ON o.book_id=b.id LEFT JOIN companion_books c ON c.book_id=b.id WHERE a.day=?1 GROUP BY b.id ORDER BY SUM(a.milliseconds) DESC,b.id LIMIT 200")?;
        let books = stmt
            .query_map([selected_day], |r| {
                Ok(ActivityBook {
                    lookups: r.get(4)?,
                    id: r.get(0)?,
                    title: r.get(1)?,
                    milliseconds: r.get(2)?,
                    characters: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let finished = connection.query_row("SELECT COUNT(*) FROM books WHERE reading_status='finished' AND completed_at>=?1 AND completed_at<?2",params![week_start,super::unix_timestamp()+1],|r|r.get(0))?;
        // Streaks use all recorded days, independent of the heatmap window.
        let today: i64 =
            connection.query_row("SELECT CAST(julianday(?1) AS INTEGER)", [end_day], |r| {
                r.get(0)
            })?;
        let mut stmt = connection.prepare("SELECT CAST(julianday(day) AS INTEGER) FROM reading_activity WHERE day<=?1 GROUP BY day HAVING SUM(milliseconds)>=60000 ORDER BY day")?;
        let mut last = None;
        let mut run = 0;
        let mut longest_streak = 0;
        for day in stmt.query_map([end_day], |r| r.get::<_, i64>(0))? {
            let day = day?;
            run = if last == Some(day - 1) { run + 1 } else { 1 };
            longest_streak = longest_streak.max(run);
            last = Some(day);
        }
        let streak = if last == Some(today) || last == Some(today - 1) {
            run
        } else {
            0
        };
        Ok(ActivitySummary {
            enabled,
            days,
            books,
            finished,
            streak,
            longest_streak,
        })
    }
    pub fn clear_reading_activity(&self) -> rusqlite::Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.transaction()?;
        super::activity_sync::clear(&tx,&uuid::Uuid::new_v4().simple().to_string())?;
        tx.commit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{book::NewBook, library_root::NewLibraryRoot};
    #[test]
    fn activity_deduplicates_pauses_and_survives_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog.sqlite3");
        let db = Database::open(&path).unwrap();
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "absent",
                display_name: "Books",
            })
            .unwrap();
        let id = db
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: "absent/a.epub",
                parent_folder_path: "absent",
                file_name: "Test.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap()
            .id;
        let record = |start, end| ActivityCheckpoint {
            generation: None,
            book_id: id,
            day: "2026-10-13".into(),
            start_ms: start,
            end_ms: end,
            milliseconds: end - start,
        };
        db.record_activity(record(10000, 20000)).unwrap();
        db.record_activity(record(10000, 20000)).unwrap();
        db.record_activity(record(15000, 25000)).unwrap();
        assert_eq!(
            db.reading_activity("2026-10-01", "2026-10-31", "2026-10-13", 0)
                .unwrap()
                .days[0]
                .milliseconds,
            15000
        );
        db.set_setting("reading_activity_enabled", "false").unwrap();
        db.record_activity(record(25000, 30000)).unwrap();
        assert!(
            !db.reading_activity("2026-10-01", "2026-10-31", "2026-10-13", 0)
                .unwrap()
                .enabled
        );
        assert!(db.record_activity(record(30000, 60000)).is_err());
        let backup = dir.path().join("backup.sqlite3");
        db.connection
            .lock()
            .unwrap()
            .backup(rusqlite::DatabaseName::Main, &backup, None)
            .unwrap();
        let restored = Database::open(&backup).unwrap();
        assert_eq!(
            restored
                .reading_activity("2026-10-01", "2026-10-31", "2026-10-13", 0)
                .unwrap()
                .books[0]
                .milliseconds,
            15000
        );
        restored.clear_reading_activity().unwrap();
        assert!(restored
            .reading_activity("2026-10-01", "2026-10-31", "2026-10-13", 0)
            .unwrap()
            .days
            .is_empty());
        assert!(restored.book_details(id).unwrap().is_some());
        drop(db);
        let reopened = Database::open(&path).unwrap();
        assert_eq!(
            reopened
                .reading_activity("2026-10-01", "2026-10-31", "2026-10-13", 0)
                .unwrap()
                .days[0]
                .milliseconds,
            15000
        );
    }
    #[test]
    fn character_coverage_lookup_counts_and_streaks_are_durable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metrics.sqlite3");
        let db = Database::open(&path).unwrap();
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "absent",
                display_name: "Books",
            })
            .unwrap();
        let id = db
            .add_book(NewBook {
                library_root_id: root.id,
                file_path: "absent/book.epub",
                parent_folder_path: "absent",
                file_name: "Test.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap()
            .id;
        let key = db.reader_book(id).unwrap().unwrap().content_key;
        let chars = |start, end| CharacterCheckpoint {
            generation: None,
            book_id: id,
            day: "2026-10-13".into(),
            content_key: key.clone(),
            spine: 0,
            start_offset: start,
            end_offset: end,
        };
        assert_eq!(db.record_characters(chars(0, 100)).unwrap(), 100);
        assert_eq!(db.record_characters(chars(40, 140)).unwrap(), 40);
        assert_eq!(db.record_characters(chars(0, 100)).unwrap(), 0);
        let mut stale = chars(140, 150);
        stale.content_key = "changed".into();
        assert!(db.record_characters(stale).is_err());
        db.set_setting("lookup_history_enabled", "false").unwrap();
        let lookup = super::super::history::LookupRecord {
            activity_generation: None,
            activity_day: Some("2026-10-13".into()),
            surface: "本".into(),
            entry_id: None,
            headword: None,
            reading: None,
            dictionary_id: None,
            book_id: None,
            location_cfi: String::new(),
            sentence: String::new(),
        };
        assert_eq!(db.record_lookup(&lookup).unwrap(), None);
        db.record_lookup(&lookup).unwrap();
        db.set_setting("reading_activity_enabled", "false").unwrap();
        assert_eq!(db.record_characters(chars(140, 160)).unwrap(), 0);
        db.record_lookup(&lookup).unwrap();
        db.clear_lookup_history().unwrap();
        {
            let conn = db.connection.lock().unwrap();
            for day in ["2026-10-09", "2026-10-10", "2026-10-11"] {
                conn.execute(
                    "INSERT INTO reading_activity(day,book_id,milliseconds) VALUES(?1,?2,60000)",
                    params![day, id],
                )
                .unwrap();
            }
            conn.execute("INSERT INTO reading_activity(day,book_id,milliseconds) VALUES('2026-10-12',?1,59000)",[id]).unwrap();
        }
        let summary = db
            .reading_activity("2026-10-01", "2026-10-13", "2026-10-13", 0)
            .unwrap();
        assert_eq!(summary.streak, 0);
        assert_eq!(summary.longest_streak, 3);
        assert_eq!(summary.books[0].characters, 140);
        assert_eq!(summary.days.last().unwrap().lookups, 2);
        assert_eq!(
            db.reading_activity("2026-10-01", "2026-10-12", "2026-10-12", 0)
                .unwrap()
                .streak,
            3
        );
        db.connection
            .lock()
            .unwrap()
            .execute(
                "UPDATE reading_activity SET milliseconds=60000 WHERE day='2026-10-12'",
                [],
            )
            .unwrap();
        assert_eq!(
            db.reading_activity("2026-10-01", "2026-10-13", "2026-10-13", 0)
                .unwrap()
                .streak,
            4
        );
        let backup = dir.path().join("backup.sqlite3");
        db.backup_to(&backup).unwrap();
        let restored = Database::open(&backup).unwrap();
        assert_eq!(
            restored
                .reading_activity("2026-10-01", "2026-10-13", "2026-10-13", 0)
                .unwrap()
                .books[0]
                .characters,
            140
        );
        restored.clear_reading_activity().unwrap();
        let cleared = restored
            .reading_activity("2026-10-01", "2026-10-13", "2026-10-13", 0)
            .unwrap();
        assert!(cleared.days.is_empty());
        assert_eq!(cleared.longest_streak, 0);
        assert!(restored.book_details(id).unwrap().is_some());
        assert!(!valid_day("2026-02-29"));
        assert!(valid_day("2024-02-29"));
    }
}
