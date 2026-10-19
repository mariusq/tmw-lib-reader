CREATE TABLE IF NOT EXISTS reading_activity (
 day TEXT NOT NULL, book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
 device TEXT NOT NULL DEFAULT 'desktop', milliseconds INTEGER NOT NULL CHECK(milliseconds >= 0),
 PRIMARY KEY(day,book_id,device));
CREATE TABLE IF NOT EXISTS reading_activity_clock(singleton INTEGER PRIMARY KEY CHECK(singleton=1), end_ms INTEGER NOT NULL);
INSERT OR IGNORE INTO reading_activity_clock VALUES(1,0);
INSERT OR IGNORE INTO app_settings(key,value) VALUES('reading_activity_enabled','true');
