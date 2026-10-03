-- Imported JMdict data is app-managed lookup data, never source-library data.
CREATE TABLE dictionaries (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  source_path TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  imported_at INTEGER NOT NULL
);
CREATE TABLE dictionary_entries (
  id INTEGER PRIMARY KEY,
  dictionary_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
  term TEXT NOT NULL,
  reading TEXT,
  definitions TEXT NOT NULL,
  part_of_speech TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_dictionary_entries_term ON dictionary_entries(term COLLATE NOCASE);
