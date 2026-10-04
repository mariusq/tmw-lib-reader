CREATE TABLE IF NOT EXISTS companion_books (
 book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
 public_id TEXT NOT NULL UNIQUE
);
CREATE TRIGGER IF NOT EXISTS companion_book_insert AFTER INSERT ON books BEGIN
 INSERT INTO companion_books VALUES(new.id, lower(hex(randomblob(16))));
END;
INSERT OR IGNORE INTO companion_books SELECT id,lower(hex(randomblob(16))) FROM books;
INSERT OR IGNORE INTO app_settings(key,value) VALUES('companion_catalog_id',lower(hex(randomblob(16))));

