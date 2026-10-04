-- Additive journal. Coalesces each public ID to its latest revision; bounded by
-- current books plus retained tombstones. Old cursors restart after compaction.
CREATE TABLE IF NOT EXISTS companion_revision(singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL, floor INTEGER NOT NULL, epoch TEXT NOT NULL);
INSERT OR IGNORE INTO companion_revision VALUES(1,0,0,lower(hex(randomblob(16))));
CREATE TABLE IF NOT EXISTS companion_changes(public_id TEXT PRIMARY KEY, revision INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS companion_changes_revision ON companion_changes(revision,public_id);
CREATE TRIGGER IF NOT EXISTS companion_identity_added AFTER INSERT ON companion_books BEGIN
 UPDATE companion_revision SET revision=revision+1;
 INSERT INTO companion_changes VALUES(new.public_id,(SELECT revision FROM companion_revision)) ON CONFLICT(public_id) DO UPDATE SET revision=excluded.revision;
END;
CREATE TRIGGER IF NOT EXISTS companion_identity_removed BEFORE DELETE ON companion_books BEGIN
 UPDATE companion_revision SET revision=revision+1;
 INSERT INTO companion_changes VALUES(old.public_id,(SELECT revision FROM companion_revision)) ON CONFLICT(public_id) DO UPDATE SET revision=excluded.revision;
 UPDATE companion_revision SET floor=max(floor,revision-100000);
 DELETE FROM companion_changes WHERE revision<=(SELECT floor FROM companion_revision)
 AND public_id NOT IN (SELECT public_id FROM companion_books);
END;
