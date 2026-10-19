CREATE TABLE activity_sync_state(singleton INTEGER PRIMARY KEY CHECK(singleton=1), epoch TEXT NOT NULL,cutoff_ms INTEGER NOT NULL DEFAULT 0);
INSERT INTO activity_sync_state(singleton,epoch) VALUES(1,lower(hex(randomblob(16))));
CREATE TABLE activity_events(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT NOT NULL UNIQUE,json TEXT NOT NULL);
CREATE INDEX activity_event_book_day ON activity_events(json_extract(json,'$.bookId'),json_extract(json,'$.day'),json_extract(json,'$.kind'));
CREATE TABLE activity_time_coverage(start_ms INTEGER PRIMARY KEY,end_ms INTEGER NOT NULL CHECK(end_ms>start_ms));
CREATE TABLE activity_reset_receipts(id TEXT PRIMARY KEY);
-- Preserve measured historical aggregates. Never fabricate historical intervals.
INSERT INTO activity_events(id,json) SELECT 'baseline:'||a.day||':'||c.public_id||':'||a.device,
 json_object('id','baseline:'||a.day||':'||c.public_id||':'||a.device,'kind','baseline','day',a.day,'bookId',c.public_id,'title',substr(COALESCE(NULLIF(o.title,''),b.discovered_title,b.file_name),1,1000),'milliseconds',a.milliseconds,'characters',a.characters)
 FROM reading_activity a JOIN companion_books c ON c.book_id=a.book_id JOIN books b ON b.id=a.book_id LEFT JOIN book_overrides o ON o.book_id=b.id;
INSERT INTO activity_events(id,json) SELECT 'lookup-baseline:'||day,json_object('id','lookup-baseline:'||day,'kind','baseline','day',day,'bookId','','title','','lookups',lookups) FROM reading_lookup_activity;
INSERT INTO activity_events(id,json) SELECT 'coverage:'||c.public_id||':'||r.content_key||':'||r.spine||':'||r.start_offset,
 json_object('id','coverage:'||c.public_id||':'||r.content_key||':'||r.spine||':'||r.start_offset,'kind','coverage','day','2000-01-01','bookId',c.public_id,'version',r.content_key,'spine',r.spine,'startOffset',r.start_offset,'endOffset',r.end_offset)
 FROM reading_character_coverage r JOIN companion_books c ON c.book_id=r.book_id;
