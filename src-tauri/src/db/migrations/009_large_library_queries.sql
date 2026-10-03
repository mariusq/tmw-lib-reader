-- Phase 6 optimization: support the common non-search browse orders and
-- filtered pagination without building a full result set in application code.
CREATE INDEX IF NOT EXISTS idx_books_created_at_id ON books(created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_books_modified_time_id ON books(modified_time DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_books_folder_file ON books(parent_folder_path COLLATE NOCASE, file_name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_books_root_created_at_id ON books(library_root_id, created_at DESC, id DESC);
