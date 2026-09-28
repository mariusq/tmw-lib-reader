-- Phase 6: local, regenerable matching fields; catalog display text is unchanged.
CREATE TABLE book_search_documents (
  book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
  title_normalized TEXT NOT NULL DEFAULT '', creator_normalized TEXT NOT NULL DEFAULT '',
  series_normalized TEXT NOT NULL DEFAULT '', file_name_normalized TEXT NOT NULL DEFAULT '',
  parent_folder_normalized TEXT NOT NULL DEFAULT '', tags_normalized TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_book_search_title ON book_search_documents(title_normalized);
CREATE INDEX idx_book_search_creator ON book_search_documents(creator_normalized);
CREATE VIRTUAL TABLE book_search_fts USING fts5(
  book_id UNINDEXED, title, creator, series, file_name, parent_folder, tags,
  tokenize='trigram case_sensitive 0'
);
