-- Make duplicate-title collapsing an indexed lookup instead of a correlated
-- full scan for every candidate book. The expression matches browse_books'
-- case-insensitive comparison and book_id supports the "earliest row" range.
CREATE INDEX IF NOT EXISTS idx_book_search_duplicate_title
ON book_search_documents(lower(title_normalized), book_id);
