CREATE TABLE IF NOT EXISTS reading_character_coverage (
 book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
 content_key TEXT NOT NULL, spine INTEGER NOT NULL,
 start_offset INTEGER NOT NULL, end_offset INTEGER NOT NULL CHECK(end_offset>start_offset),
 PRIMARY KEY(book_id,content_key,spine,start_offset));
CREATE TABLE IF NOT EXISTS reading_lookup_activity (
 day TEXT PRIMARY KEY, lookups INTEGER NOT NULL DEFAULT 0 CHECK(lookups>=0));
