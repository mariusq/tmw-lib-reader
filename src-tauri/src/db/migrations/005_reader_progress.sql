-- Reader state is catalog-only and may be safely retained when a source file is unavailable.
CREATE TABLE reading_progress (
  book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
  location_cfi TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX idx_reading_progress_updated_at ON reading_progress(updated_at DESC);
