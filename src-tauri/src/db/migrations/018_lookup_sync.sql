-- Additive archive identities. Existing events and dictionary-independent text survive.
ALTER TABLE lookup_history ADD COLUMN sync_id TEXT;
ALTER TABLE lookup_history ADD COLUMN public_book_id TEXT NOT NULL DEFAULT '';
ALTER TABLE lookup_history ADD COLUMN content_version TEXT;
ALTER TABLE lookup_history ADD COLUMN dictionary_id TEXT NOT NULL DEFAULT 'legacy-desktop';
ALTER TABLE lookup_history ADD COLUMN dictionary_entry_id TEXT;
ALTER TABLE lookup_history ADD COLUMN dictionary_label TEXT;
UPDATE lookup_history SET sync_id=lower(hex(randomblob(16))), public_book_id=COALESCE((SELECT public_id FROM companion_books WHERE book_id=lookup_history.book_id),'');
CREATE UNIQUE INDEX lookup_history_sync_id ON lookup_history(sync_id);
CREATE TABLE companion_history_tombstones(entity_id TEXT PRIMARY KEY);
CREATE TRIGGER companion_history_insert AFTER INSERT ON lookup_history WHEN new.sync_id IS NULL BEGIN
 UPDATE lookup_history SET sync_id=lower(hex(randomblob(16))),public_book_id=COALESCE((SELECT public_id FROM companion_books WHERE book_id=new.book_id),'') WHERE id=new.id;
END;
CREATE TRIGGER companion_history_update AFTER UPDATE OF sync_id ON lookup_history WHEN new.sync_id IS NOT NULL BEGIN
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) VALUES(new.public_book_id,'history',new.sync_id,0,new.content_version,json_object('surface',new.surface,'headword',new.headword,'reading',new.reading,'sentence',new.sentence,'locationCfi',new.location_cfi,'lookedUpAt',CAST(new.looked_up_at AS TEXT),'dictionaryId',new.dictionary_id,'dictionaryEntryId',new.dictionary_entry_id,'dictionaryLabel',new.dictionary_label));
END;
CREATE TRIGGER companion_history_synced_insert AFTER INSERT ON lookup_history WHEN new.sync_id IS NOT NULL BEGIN
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) VALUES(new.public_book_id,'history',new.sync_id,0,new.content_version,json_object('surface',new.surface,'headword',new.headword,'reading',new.reading,'sentence',new.sentence,'locationCfi',new.location_cfi,'lookedUpAt',CAST(new.looked_up_at AS TEXT),'dictionaryId',new.dictionary_id,'dictionaryEntryId',new.dictionary_entry_id,'dictionaryLabel',new.dictionary_label));
END;
CREATE TRIGGER companion_history_delete BEFORE DELETE ON lookup_history WHEN old.sync_id IS NOT NULL BEGIN
 INSERT OR IGNORE INTO companion_history_tombstones VALUES(old.sync_id);
 INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) VALUES(old.public_book_id,'history',old.sync_id,1,old.content_version,'{}');
END;
INSERT INTO companion_user_changes(book_id,kind,entity_id,deleted,content_version,fields) SELECT public_book_id,'history',sync_id,0,content_version,json_object('surface',surface,'headword',headword,'reading',reading,'sentence',sentence,'locationCfi',location_cfi,'lookedUpAt',CAST(looked_up_at AS TEXT),'dictionaryId',dictionary_id,'dictionaryEntryId',dictionary_entry_id,'dictionaryLabel',dictionary_label) FROM lookup_history ORDER BY id;
