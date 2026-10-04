-- Independent of dictionary rows: rebuilding JMdict must not erase history.
CREATE TABLE IF NOT EXISTS lookup_history (
    id INTEGER PRIMARY KEY,
    identity TEXT NOT NULL,
    surface TEXT NOT NULL,
    headword TEXT,
    reading TEXT,
    search_text TEXT NOT NULL,
    book_id INTEGER REFERENCES books(id) ON DELETE SET NULL,
    location_cfi TEXT NOT NULL,
    sentence TEXT NOT NULL,
    source_size INTEGER,
    source_modified INTEGER,
    looked_up_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_lookup_history_identity ON lookup_history(identity);
