use rusqlite::{params, Connection};
use std::io::Write;
use tmw_japanese_core::dictionary::{import_jmdict, normalize_query};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let entries = import_jmdict(std::path::Path::new(&args[1]))?;
    let temporary = format!("{}.tmp", args[2]);
    let mut db = Connection::open(&temporary)?;
    db.execute_batch("DROP TABLE IF EXISTS entries; CREATE TABLE entries(id INTEGER PRIMARY KEY,term TEXT,reading TEXT,definitions TEXT,part_of_speech TEXT,term_normalized TEXT,reading_normalized TEXT); PRAGMA user_version=1;")?;
    let tx = db.transaction()?;
    {
        let mut insert = tx.prepare("INSERT INTO entries(term,reading,definitions,part_of_speech,term_normalized,reading_normalized) VALUES(?1,?2,?3,?4,?5,?6)")?;
        for e in &entries {
            insert.execute(params![
                e.term,
                e.reading,
                serde_json::to_string(&e.definitions)?,
                serde_json::to_string(&e.part_of_speech)?,
                normalize_query(&e.term),
                normalize_query(e.reading.as_deref().unwrap_or(""))
            ])?;
        }
    }
    tx.commit()?;
    db.execute_batch("CREATE INDEX term_lookup ON entries(term_normalized); CREATE INDEX reading_lookup ON entries(reading_normalized); CREATE INDEX prefix_lookup ON entries(term_normalized COLLATE NOCASE); VACUUM;")?;
    db.close().map_err(|(_, e)| e)?;
    std::fs::rename(temporary, &args[2])?;
    let compressed_path = format!("{}.gz", args[2]);
    let compressed_temp = format!("{compressed_path}.tmp");
    let mut encoder = flate2::write::GzEncoder::new(
        std::fs::File::create(&compressed_temp)?,
        flate2::Compression::best(),
    );
    std::io::copy(&mut std::fs::File::open(&args[2])?, &mut encoder)?;
    encoder.flush()?;
    encoder.finish()?.sync_all()?;
    std::fs::rename(compressed_temp, compressed_path)?;
    println!("Built {} forms", entries.len());
    Ok(())
}
