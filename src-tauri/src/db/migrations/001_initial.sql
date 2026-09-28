CREATE TABLE library_roots (
  id INTEGER PRIMARY KEY,
  path TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  added_at INTEGER NOT NULL,
  last_scanned_at INTEGER
);

CREATE TABLE books (
  id INTEGER PRIMARY KEY,
  library_root_id INTEGER NOT NULL REFERENCES library_roots(id) ON DELETE CASCADE,
  file_path TEXT NOT NULL UNIQUE,
  parent_folder_path TEXT NOT NULL,
  file_name TEXT NOT NULL,
  file_size INTEGER NOT NULL,
  modified_time INTEGER NOT NULL,
  content_hash TEXT,
  discovered_title TEXT,
  discovered_creator TEXT,
  discovered_language TEXT,
  discovered_identifier TEXT,
  discovered_series TEXT,
  discovered_series_index TEXT,
  discovered_cover_path TEXT,
  extraction_status TEXT NOT NULL DEFAULT 'pending',
  extraction_error TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE TABLE book_overrides (
  book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
  title TEXT,
  creator TEXT,
  series_name TEXT,
  volume_label TEXT,
  cover_path TEXT,
  notes TEXT,
  updated_at INTEGER NOT NULL
);

CREATE TABLE tags (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE book_tags (
  book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (book_id, tag_id)
);

CREATE TABLE collections (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

CREATE TABLE collection_books (
  collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  sort_order INTEGER,
  PRIMARY KEY (collection_id, book_id)
);

CREATE TABLE app_settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX idx_books_library_root_id ON books(library_root_id);
CREATE INDEX idx_books_parent_folder_path ON books(parent_folder_path);
CREATE INDEX idx_books_discovered_title ON books(discovered_title);
CREATE INDEX idx_books_discovered_creator ON books(discovered_creator);
CREATE INDEX idx_tags_name ON tags(name);
