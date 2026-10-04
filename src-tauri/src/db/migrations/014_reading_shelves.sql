-- Additive: keeps progress, resume history, and every previous user correction.
ALTER TABLE books ADD COLUMN reading_status TEXT NOT NULL DEFAULT 'unset'
    CHECK(reading_status IN ('unset','want','reading','paused','finished'));
ALTER TABLE books ADD COLUMN completed_at INTEGER;
UPDATE books SET reading_status=CASE WHEN EXISTS(SELECT 1 FROM reader_resume r WHERE r.book_id=books.id AND r.finished=1) THEN 'finished' ELSE 'reading' END
WHERE EXISTS(SELECT 1 FROM reader_resume r WHERE r.book_id=books.id);
-- Historical completion dates were never recorded: leave those unknown.
CREATE INDEX idx_books_reading_status ON books(reading_status,id);
CREATE TABLE smart_shelves (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK(length(trim(name)) BETWEEN 1 AND 120),
    version INTEGER NOT NULL DEFAULT 1 CHECK(version=1),
    filter_json TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);
