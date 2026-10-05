use rusqlite::{params, Connection};
use serde::Deserialize;

#[derive(Deserialize)]
struct CandidateGroup {
    group: i64,
    high: bool,
    paths: Vec<String>,
}

// A connection-local index: no catalog migration or source-library writes.
pub(super) fn initialize(connection: &mut Connection) -> rusqlite::Result<()> {
    let groups: Vec<CandidateGroup> = serde_json::from_str(include_str!("duplicate_candidates.json"))
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let transaction = connection.transaction()?;
    transaction.execute_batch("CREATE TEMP TABLE duplicate_candidates (path TEXT NOT NULL, group_id INTEGER NOT NULL, high INTEGER NOT NULL, PRIMARY KEY(path, high)); CREATE INDEX temp.duplicate_candidate_groups ON duplicate_candidates(group_id, path);")?;
    {
        let mut statement = transaction.prepare("INSERT INTO duplicate_candidates VALUES (?1, ?2, ?3)")?;
        for group in groups {
            for path in group.paths {
                statement.execute(params![path, group.group, group.high])?;
            }
        }
    }
    transaction.commit()
}

// Prefer available copies, then the earliest catalog ID. Missing/deleted members
// cannot suppress the remaining member. Apply before LIMIT/OFFSET.
pub(super) const ALL_PREDICATE: &str = "AND NOT EXISTS (SELECT 1 FROM duplicate_candidates candidate JOIN duplicate_candidates peer ON peer.group_id=candidate.group_id JOIN books earlier ON earlier.file_path=peer.path WHERE candidate.path=b.file_path AND candidate.high=0 AND ((earlier.extraction_status<>'unavailable' AND b.extraction_status='unavailable') OR ((earlier.extraction_status='unavailable')=(b.extraction_status='unavailable') AND earlier.id<b.id)))";
pub(super) const HIGH_PREDICATE: &str = "AND NOT EXISTS (SELECT 1 FROM duplicate_candidates candidate JOIN duplicate_candidates peer ON peer.group_id=candidate.group_id JOIN books earlier ON earlier.file_path=peer.path WHERE candidate.path=b.file_path AND candidate.high=1 AND ((earlier.extraction_status<>'unavailable' AND b.extraction_status='unavailable') OR ((earlier.extraction_status='unavailable')=(b.extraction_status='unavailable') AND earlier.id<b.id)))";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_collapse_by_confidence_and_recover_missing_copies() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        connection.execute_batch("CREATE TABLE books(id INTEGER PRIMARY KEY, file_path TEXT, extraction_status TEXT); INSERT INTO books SELECT row_number() OVER (ORDER BY group_id,path),path,'complete' FROM duplicate_candidates WHERE high=0;").unwrap();
        let count = |predicate: &str| -> i64 {
            connection.query_row(&format!("SELECT count(*) FROM books b WHERE 1=1 {predicate}"), [], |row| row.get(0)).unwrap()
        };
        assert_eq!(count(ALL_PREDICATE), 721);
        assert!(count(HIGH_PREDICATE) > count(ALL_PREDICATE));
        assert!(count("") > count(HIGH_PREDICATE));
        connection.execute("UPDATE books SET extraction_status='unavailable' WHERE id=1", []).unwrap();
        assert_eq!(count(ALL_PREDICATE), 721);
        let first: i64 = connection.query_row(&format!("SELECT min(id) FROM books b WHERE 1=1 {ALL_PREDICATE}"), [], |row| row.get(0)).unwrap();
        assert_eq!(first, 2);
        connection.execute("DELETE FROM books WHERE id=2", []).unwrap();
        assert_eq!(count(ALL_PREDICATE), 721);
    }
}
