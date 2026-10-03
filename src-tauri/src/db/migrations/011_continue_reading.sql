-- Additive user state, independent of extracted metadata and saved CFI.
CREATE TABLE reader_resume (
    book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    last_read_at INTEGER NOT NULL,
    finished INTEGER NOT NULL DEFAULT 0 CHECK (finished IN (0, 1))
);
INSERT INTO reader_resume(book_id,last_read_at)
SELECT book_id,updated_at FROM reading_progress;
CREATE INDEX idx_reader_resume_recent ON reader_resume(finished,last_read_at DESC,book_id DESC);
