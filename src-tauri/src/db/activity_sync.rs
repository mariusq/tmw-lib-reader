use super::*;
use serde::Deserialize;
use serde_json::{json,Value};

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct ActivitySyncRequest {
    pub catalog_id:String, pub epoch:Option<String>,
    #[serde(default)] pub cursor:i64,
    #[serde(default)] pub reset:Option<String>,
    #[serde(default)] pub operations:Vec<Value>,
}
pub(crate) fn epoch(c:&Connection)->rusqlite::Result<String> {
    c.query_row("SELECT epoch FROM activity_sync_state WHERE singleton=1",[],|r|r.get(0))
}
pub(crate) fn clear(c:&Connection,new_epoch:&str)->rusqlite::Result<()> {
    c.execute_batch("DELETE FROM reading_activity; DELETE FROM reading_lookup_activity; DELETE FROM reading_character_coverage; DELETE FROM activity_time_coverage; DELETE FROM activity_events;")?;
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as i64;
    c.execute("UPDATE activity_sync_state SET epoch=?1,cutoff_ms=?2 WHERE singleton=1",params![new_epoch,now])?;
    Ok(())
}
pub(crate) fn event(c:&Connection,book:Option<i64>,mut op:Value)->rusqlite::Result<bool> {
    let id=op["id"].as_str().filter(|s|!s.is_empty()&&s.len()<=256).ok_or(rusqlite::Error::InvalidQuery)?.to_owned();
    if c.query_row("SELECT EXISTS(SELECT 1 FROM activity_events WHERE id=?1)",[&id],|r|r.get::<_,bool>(0))? {return Ok(false)}
    let day=op["day"].as_str().filter(|s|super::activity::valid_day(s)).ok_or(rusqlite::Error::InvalidQuery)?.to_owned();
    let kind=op["kind"].as_str().unwrap_or("").to_owned();
    let mut time=0;let mut chars=0;let mut lookups=0;
    if kind=="time" {
        if book.is_none(){return Err(rusqlite::Error::InvalidQuery)}
        let a=op["startMs"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        let b=op["endMs"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        let ms=op["milliseconds"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        if a<0||b<=a||b-a>15000||ms<=0||ms>b-a {return Err(rusqlite::Error::InvalidQuery)}
        let mut stmt=c.prepare("SELECT start_ms,end_ms FROM activity_time_coverage WHERE end_ms>=?1 AND start_ms<=?2")?;
        let rows=stmt.query_map(params![a,b],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let (mut start,mut end,mut overlap)=(a,b,0);
        for (x,y) in rows {overlap+=(b.min(y)-a.max(x)).max(0);start=start.min(x);end=end.max(y)}
        time=ms.min((b-a-overlap).max(0));
        c.execute("DELETE FROM activity_time_coverage WHERE end_ms>=?1 AND start_ms<=?2",params![a,b])?;
        c.execute("INSERT INTO activity_time_coverage VALUES(?1,?2)",params![start,end])?;
    } else if kind=="characters" {
        let book=book.ok_or(rusqlite::Error::InvalidQuery)?;
        let version=op["version"].as_str().filter(|s|!s.is_empty()&&s.len()<=128).ok_or(rusqlite::Error::InvalidQuery)?;
        let spine=op["spine"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        let a=op["startOffset"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        let b=op["endOffset"].as_i64().ok_or(rusqlite::Error::InvalidQuery)?;
        if !(0..=100000).contains(&spine)||a<0||b<=a||b>100000000||b-a>100000 {return Err(rusqlite::Error::InvalidQuery)}
        let rows=c.prepare("SELECT start_offset,end_offset FROM reading_character_coverage WHERE book_id=?1 AND content_key=?2 AND spine=?3 AND end_offset>=?4 AND start_offset<=?5")?.query_map(params![book,version,spine,a,b],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let count:i64=c.query_row("SELECT COUNT(*) FROM reading_character_coverage WHERE book_id=?1 AND content_key=?2",params![book,version],|r|r.get(0))?;
        if count>=4096&&rows.is_empty(){return Err(rusqlite::Error::InvalidQuery)}
        let (mut start,mut end,mut overlap)=(a,b,0);
        for (x,y) in rows {overlap+=(b.min(y)-a.max(x)).max(0);start=start.min(x);end=end.max(y)}
        chars=b-a-overlap;
        c.execute("DELETE FROM reading_character_coverage WHERE book_id=?1 AND content_key=?2 AND spine=?3 AND end_offset>=?4 AND start_offset<=?5",params![book,version,spine,a,b])?;
        c.execute("INSERT INTO reading_character_coverage VALUES(?1,?2,?3,?4,?5)",params![book,version,spine,start,end])?;
    } else if kind=="lookup" {lookups=1} else {return Err(rusqlite::Error::InvalidQuery)}
    if let Some(book)=book {
        c.execute("INSERT INTO reading_activity(day,book_id,device,milliseconds,characters) VALUES(?1,?2,'combined',?3,?4) ON CONFLICT(day,book_id,device) DO UPDATE SET milliseconds=milliseconds+excluded.milliseconds,characters=characters+excluded.characters",params![day,book,time,chars])?;
        let (public,title):(String,String)=c.query_row("SELECT c.public_id,COALESCE(NULLIF(o.title,''),b.discovered_title,b.file_name) FROM books b JOIN companion_books c ON c.book_id=b.id LEFT JOIN book_overrides o ON o.book_id=b.id WHERE b.id=?1",[book],|r|Ok((r.get(0)?,r.get(1)?)))?;
        op["bookId"]=json!(public);op["title"]=json!(title.chars().take(1000).collect::<String>());
    }
    if lookups>0 {c.execute("INSERT INTO reading_lookup_activity VALUES(?1,1) ON CONFLICT(day) DO UPDATE SET lookups=lookups+1",[day])?;}
    c.execute("INSERT INTO activity_events(id,json) VALUES(?1,?2)",params![id,op.to_string()])?;
    Ok(true)
}
pub(crate) fn local(c:&Connection,book:Option<i64>,op:Value)->rusqlite::Result<bool> {
    event(c,book,op)
}
/// A freshly verified hash can safely identify existing metadata-guarded coverage.
/// Counts are preserved; only future overlap comparison gains the stronger key.
pub(crate) fn promote_version(c:&Connection,book:i64,old:&str,new:&str)->rusqlite::Result<()> {
    let rows=c.prepare("SELECT spine,start_offset,end_offset FROM reading_character_coverage WHERE book_id=?1 AND content_key IN (?2,?3) ORDER BY spine,start_offset")?.query_map(params![book,old,new],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let old_exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM reading_character_coverage WHERE book_id=?1 AND content_key=?2)",params![book,old],|r|r.get(0))?;
    if !old_exists{return Ok(())}
    let mut merged:Vec<(i64,i64,i64)>=Vec::new();
    for (spine,a,b) in rows {if let Some(last)=merged.last_mut(){if last.0==spine&&last.2>=a {last.2=last.2.max(b);continue}}merged.push((spine,a,b));}
    c.execute("DELETE FROM reading_character_coverage WHERE book_id=?1 AND content_key IN (?2,?3)",params![book,old,new])?;
    for (spine,a,b) in merged {c.execute("INSERT INTO reading_character_coverage VALUES(?1,?2,?3,?4,?5)",params![book,new,spine,a,b])?;}
    let public:String=c.query_row("SELECT public_id FROM companion_books WHERE book_id=?1",[book],|r|r.get(0))?;
    let id=uuid::Uuid::new_v4().to_string();let op=json!({"id":id,"kind":"versionAlias","day":"2000-01-01","bookId":public,"oldVersion":old,"version":new});
    c.execute("INSERT INTO activity_events(id,json) VALUES(?1,?2)",params![id,op.to_string()])?;
    Ok(())
}
impl Database {
    pub fn activity_generation(&self)->rusqlite::Result<String> {epoch(&self.connection.lock().unwrap())}
    pub fn activity_sync(&self,q:ActivitySyncRequest)->rusqlite::Result<Value> {
        if q.cursor<0||q.operations.len()>64 {return Err(rusqlite::Error::InvalidQuery)}
        let mut c=self.connection.lock().unwrap();let tx=c.transaction()?;
        let ns:String=tx.query_row("SELECT value FROM app_settings WHERE key='companion_catalog_id'",[],|r|r.get(0))?;
        if ns!=q.catalog_id {return Err(rusqlite::Error::InvalidQuery)}
        let mut generation=epoch(&tx)?;
        if let Some(reset)=&q.reset {
            if reset.len()!=32||!reset.bytes().all(|b|b.is_ascii_hexdigit()){return Err(rusqlite::Error::InvalidQuery)}
            let seen:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM activity_reset_receipts WHERE id=?1)",[reset],|r|r.get(0))?;
            if !seen && (q.epoch.is_none()||q.epoch.as_deref()==Some(&generation)) {
                clear(&tx,reset)?;generation=reset.clone();
                tx.execute("INSERT INTO activity_reset_receipts VALUES(?1)",[reset])?;
            }
        }
        // First contact can submit offline activity. A known stale generation cannot.
        let same=q.epoch.is_none()||q.epoch.as_deref()==Some(&generation)||q.reset.as_ref()==Some(&generation);
        let mut ack=Vec::new();let mut rejected=Vec::new();
        if same {
            let cutoff:i64=tx.query_row("SELECT cutoff_ms FROM activity_sync_state WHERE singleton=1",[],|r|r.get(0))?;
            for op in q.operations {
                let id=op["id"].as_str().unwrap_or("").to_owned();
                let reset_origin=op["resetId"].as_str()==Some(&generation)||q.reset.as_ref()==Some(&generation);
                if !reset_origin {
                    if op["generation"].as_str().is_some_and(|g|g!=generation) {rejected.push(id);continue}
                    let unknown_origin=q.epoch.is_none()||(op.get("generation").is_some()&&op["generation"].is_null());
                    if unknown_origin && cutoff>0 && (op["occurredAt"].as_i64().unwrap_or(0)<=cutoff || (op["kind"]=="time"&&op["startMs"].as_i64().unwrap_or(0)<=cutoff)) {rejected.push(id);continue}
                }
                let public=op["bookId"].as_str().unwrap_or("");
                if op["kind"]=="characters" {
                    let v=op["version"].as_str().unwrap_or("");
                    if v.len()!=71||!v.starts_with("sha256-")||!v[7..].bytes().all(|b|b.is_ascii_hexdigit()) {rejected.push(id);continue}
                }
                let book=if public.is_empty(){None}else{tx.query_row("SELECT book_id FROM companion_books WHERE public_id=?1",[public],|r|r.get::<_,i64>(0)).optional()?};
                if !public.is_empty()&&book.is_none(){rejected.push(id);continue}
                // Atomic per-operation rollback, while allowing valid neighbours through.
                tx.execute_batch("SAVEPOINT activity_op")?;
                match event(&tx,book,op) {Ok(_)=>{tx.execute_batch("RELEASE activity_op")?;ack.push(id)},Err(_)=>{tx.execute_batch("ROLLBACK TO activity_op; RELEASE activity_op")?;rejected.push(id)}}
            }
        }
        let cursor=if q.epoch.as_deref()==Some(&generation){q.cursor}else{0};
        let rows=tx.prepare("SELECT seq,json FROM activity_events WHERE seq>?1 ORDER BY seq LIMIT 256")?.query_map([cursor],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let high:i64=tx.query_row("SELECT COALESCE(MAX(seq),0) FROM activity_events",[],|r|r.get(0))?;
        let mut next=cursor;let mut events=Vec::new();let mut bytes=0;
        for (seq,s) in rows {
            let mut value:Value=serde_json::from_str(&s).map_err(|_|rusqlite::Error::InvalidQuery)?;
            value["seq"]=json!(seq);
            let size=serde_json::to_vec(&value).map_err(|_|rusqlite::Error::InvalidQuery)?.len()+1;
            if !events.is_empty()&&bytes+size>500000 {break}
            bytes+=size;next=seq;events.push(value);
        }
        tx.commit()?;
        Ok(json!({"protocolVersion":1,"catalogId":ns,"epoch":generation,"cursor":next,"hasMore":next<high,"acknowledged":ack,"rejected":rejected,"events":events}))
    }
}
