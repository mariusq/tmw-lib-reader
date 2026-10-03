CREATE TABLE saved_passages (
    id INTEGER PRIMARY KEY,
    book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    surface TEXT NOT NULL,
    headword TEXT,
    reading TEXT,
    sentence TEXT NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    location_cfi TEXT NOT NULL DEFAULT '',
    source_size INTEGER NOT NULL,
    source_modified INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(book_id, location_cfi, surface)
);
CREATE INDEX saved_passages_book ON saved_passages(book_id, id DESC);
