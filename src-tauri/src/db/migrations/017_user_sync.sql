CREATE TABLE companion_content_versions(book_id INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE, version TEXT NOT NULL, source_size INTEGER NOT NULL, source_modified INTEGER NOT NULL);
ALTER TABLE reading_progress ADD COLUMN content_version TEXT;
ALTER TABLE saved_passages ADD COLUMN sync_id TEXT;
ALTER TABLE saved_passages ADD COLUMN content_version TEXT;
UPDATE saved_passages SET sync_id=lower(hex(randomblob(16)));
CREATE UNIQUE INDEX saved_passages_sync_id ON saved_passages(sync_id);
CREATE TABLE companion_user_changes(sequence INTEGER PRIMARY KEY AUTOINCREMENT, book_id TEXT NOT NULL, kind TEXT NOT NULL, entity_id TEXT NOT NULL, deleted INTEGER NOT NULL, content_version TEXT, fields TEXT NOT NULL);
CREATE TABLE companion_user_receipts(device_id TEXT NOT NULL, operation_id TEXT NOT NULL, device_sequence INTEGER NOT NULL, reason TEXT, PRIMARY KEY(device_id,operation_id), UNIQUE(device_id,device_sequence));
CREATE TABLE companion_user_devices(device_id TEXT PRIMARY KEY,last_sequence INTEGER NOT NULL);
CREATE TABLE companion_passage_tombstones(entity_id TEXT PRIMARY KEY);
CREATE TRIGGER companion_progress_insert AFTER INSERT ON reading_progress BEGIN
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT public_id,'progress',public_id,0,new.content_version,json_object('locationCfi',new.location_cfi) FROM companion_books WHERE book_id=new.book_id;
END;
CREATE TRIGGER companion_progress_update AFTER UPDATE ON reading_progress BEGIN
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT public_id,'progress',public_id,0,new.content_version,json_object('locationCfi',new.location_cfi) FROM companion_books WHERE book_id=new.book_id;
END;
CREATE TRIGGER companion_passage_insert AFTER INSERT ON saved_passages BEGIN
 UPDATE saved_passages SET sync_id=COALESCE(new.sync_id,lower(hex(randomblob(16)))) WHERE id=new.id;
END;
CREATE TRIGGER companion_passage_update AFTER UPDATE ON saved_passages BEGIN
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT public_id,'passage',new.sync_id,0,new.content_version,json_object('surface',new.surface,'headword',new.headword,'reading',new.reading,'sentence',new.sentence,'note',new.note,'locationCfi',new.location_cfi) FROM companion_books WHERE book_id=new.book_id;
END;
CREATE TRIGGER companion_passage_delete BEFORE DELETE ON saved_passages BEGIN
 INSERT OR IGNORE INTO companion_passage_tombstones VALUES(old.sync_id);
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT public_id,'passage',old.sync_id,1,old.content_version,'{}' FROM companion_books WHERE book_id=old.book_id;
END;
INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT c.public_id,'progress',c.public_id,0,p.content_version,json_object('locationCfi',p.location_cfi) FROM reading_progress p JOIN companion_books c ON c.book_id=p.book_id;
INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT c.public_id,'passage',p.sync_id,0,p.content_version,json_object('surface',p.surface,'headword',p.headword,'reading',p.reading,'sentence',p.sentence,'note',p.note,'locationCfi',p.location_cfi) FROM saved_passages p JOIN companion_books c ON c.book_id=p.book_id;
