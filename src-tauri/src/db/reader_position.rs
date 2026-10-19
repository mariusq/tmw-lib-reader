use rusqlite::{params, Connection, TransactionBehavior};
use serde::Serialize;
use super::{Database, unix_timestamp};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderPosition {
    pub location_cfi: Option<String>,
    pub token: String,
    pub accepted: bool,
}

fn snapshot(c: &Connection, id: i64) -> rusqlite::Result<ReaderPosition> {
    c.query_row("SELECT p.location_cfi,p.content_version,b.file_size,b.modified_time,r.epoch,
        CASE WHEN p.content_version IS NULL OR EXISTS(SELECT 1 FROM companion_content_versions v WHERE v.book_id=b.id AND v.version=p.content_version AND v.source_size=b.file_size AND v.source_modified=b.modified_time) THEN p.location_cfi ELSE NULL END
        FROM books b LEFT JOIN reading_progress p ON p.book_id=b.id CROSS JOIN companion_revision r WHERE b.id=?1", [id], |r| {
        let raw: Option<String> = r.get(0)?;
        let version: Option<String> = r.get(1)?;
        let size: i64 = r.get(2)?;
        let modified: i64 = r.get(3)?;
        let epoch: String = r.get(4)?;
        Ok(ReaderPosition { location_cfi: r.get(5)?, token: serde_json::to_string(&(raw,version,size,modified,epoch)).expect("position token"), accepted: true })
    })
}

impl Database {
    pub fn reader_position(&self, id: i64) -> rusqlite::Result<ReaderPosition> {
        let c = self.connection.lock().expect("database mutex poisoned");
        snapshot(&c,id)
    }

    pub fn save_reader_position(&self, id: i64, cfi: &str, expected: &str) -> rusqlite::Result<ReaderPosition> {
        let mut c = self.connection.lock().expect("database mutex poisoned");
        // Other API connections cannot commit between comparison and update.
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = snapshot(&tx,id)?;
        if current.token != expected {
            current.accepted = false;
        } else if current.location_cfi.as_deref() != Some(cfi) {
            tx.execute("INSERT INTO reading_progress(book_id,location_cfi,updated_at,content_version) VALUES(?1,?2,?3,(SELECT v.version FROM companion_content_versions v JOIN books b ON b.id=v.book_id WHERE b.id=?1 AND v.source_size=b.file_size AND v.source_modified=b.modified_time)) ON CONFLICT(book_id) DO UPDATE SET location_cfi=excluded.location_cfi,updated_at=excluded.updated_at,content_version=excluded.content_version",params![id,cfi,unix_timestamp()])?;
            current = snapshot(&tx,id)?;
        }
        tx.commit()?;
        Ok(current)
    }
}
