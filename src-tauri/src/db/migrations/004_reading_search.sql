-- Phase 7: derived readings are regenerable assistance data, never display metadata.
CREATE TABLE book_reading_overrides (
  book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
  reading TEXT,
  aliases TEXT,
  updated_at INTEGER NOT NULL
);

ALTER TABLE book_search_documents ADD COLUMN title_reading TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN creator_reading TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN series_reading TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN file_name_reading TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN aliases_normalized TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN title_romaji TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN creator_romaji TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN series_romaji TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN file_name_romaji TEXT NOT NULL DEFAULT '';
ALTER TABLE book_search_documents ADD COLUMN aliases_romaji TEXT NOT NULL DEFAULT '';
CREATE INDEX idx_book_search_title_romaji ON book_search_documents(title_romaji);

-- This is intentionally safe to replace: FTS is a fully regenerable derived index.
DROP TABLE book_search_fts;
CREATE VIRTUAL TABLE book_search_fts USING fts5(
  book_id UNINDEXED, title, creator, series, file_name, parent_folder, tags,
  title_reading, creator_reading, series_reading, file_name_reading, aliases,
  title_romaji, creator_romaji, series_romaji, file_name_romaji, aliases_romaji,
  tokenize='trigram case_sensitive 0'
);
