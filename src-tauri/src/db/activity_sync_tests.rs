use super::activity_sync::*;
use super::*;
use serde_json::{json,Value};
fn fixture() -> (tempfile::TempDir,Database,i64,String,String) {
    let dir=tempfile::tempdir().unwrap();let db=Database::open(&dir.path().join("catalog.sqlite3")).unwrap();
    let root=db.add_library_root(NewLibraryRoot{path:"generated-only",display_name:"fixture"}).unwrap();
    let book=db.add_book(NewBook{library_root_id:root.id,file_path:"generated-only/fixture.epub",parent_folder_path:"generated-only",file_name:"Fixture.epub",file_size:1,modified_time:1}).unwrap().id;
    let ns=db.setting("companion_catalog_id").unwrap().unwrap();let public=db.public_id(book).unwrap();
    (dir,db,book,ns,public)
}
fn request(ns:&str,epoch:Option<&str>,ops:Value)->ActivitySyncRequest {
    serde_json::from_value(json!({"catalogId":ns,"epoch":epoch,"operations":ops})).unwrap()
}
fn time(id:&str,book:&str,a:i64,b:i64)->Value {json!({"id":id,"kind":"time","day":"2026-10-14","bookId":book,"startMs":a,"endMs":b,"milliseconds":b-a})}
#[test]
fn early_schema_22_upgrade_preserves_activity_and_delivers_phone_queue() {
    let (dir,db,book,ns,public)=fixture();let generation=db.activity_generation().unwrap();
    db.activity_sync(request(&ns,Some(&generation),json!([time("existing",&public,10000,20000)]))).unwrap();
    db.connection.lock().unwrap().execute_batch("DROP TABLE activity_reset_receipts; ALTER TABLE activity_sync_state DROP COLUMN cutoff_ms; PRAGMA user_version=22;").unwrap();
    drop(db);
    let db=Database::open(&dir.path().join("catalog.sqlite3")).unwrap();
    assert_eq!(db.activity_generation().unwrap(),generation);
    assert_eq!(db.connection.lock().unwrap().pragma_query_value(None,"user_version",|r|r.get::<_,i64>(0)).unwrap(),23);
    assert_eq!(db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap().days[0].milliseconds,10000);
    let mut cursor=0;
    for first in (0..319).step_by(64) {
        let ops:Vec<Value>=(first..(first+64).min(319)).map(|i|time(&format!("phone-{i}"),&public,20000+i*10000,30000+i*10000)).collect();
        let q=serde_json::from_value(json!({"catalogId":ns,"epoch":generation,"cursor":cursor,"reset":null,"operations":ops})).unwrap();
        let result=Database::open_api_writer(&dir.path().join("catalog.sqlite3")).unwrap().activity_sync(q).unwrap();
        assert_eq!(result["acknowledged"].as_array().unwrap().len(),ops.len());
        assert_eq!(result["rejected"],json!([]));cursor=result["cursor"].as_i64().unwrap();
    }
    assert_eq!(db.connection.lock().unwrap().query_row("SELECT count(*) FROM activity_events",[],|r|r.get::<_,i64>(0)).unwrap(),320);
    assert_eq!(db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap().days[0].milliseconds,3200000);
    drop(db);let reopened=Database::open(&dir.path().join("catalog.sqlite3")).unwrap();
    assert_eq!(reopened.activity_generation().unwrap(),generation);
    let _=book;
}
#[test]
fn cross_device_time_coverage_delivery_and_backup_are_idempotent() {
    let (dir,db,book,ns,public)=fixture();let generation=db.activity_generation().unwrap();
    db.record_activity(activity::ActivityCheckpoint{generation:Some(generation.clone()),book_id:book,day:"2026-10-14".into(),start_ms:10000,end_ms:20000,milliseconds:10000}).unwrap();
    let version=format!("sha256-{}","a".repeat(64));
    let chars=|id:&str,a,b|json!({"id":id,"kind":"characters","day":"2026-10-14","bookId":public,"version":version,"spine":0,"startOffset":a,"endOffset":b});
    let ops=json!([time("phone-time",&public,15000,25000),chars("pc-page",0,100),chars("phone-page",40,140),json!({"id":"lookup-1","kind":"lookup","day":"2026-10-14","bookId":public})]);
    let first=db.activity_sync(request(&ns,Some(&generation),ops.clone())).unwrap();
    let retry=db.activity_sync(request(&ns,Some(&generation),ops)).unwrap();
    assert_eq!(first["events"],retry["events"]);assert_eq!(first["acknowledged"].as_array().unwrap().len(),4);
    let summary=db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap();
    assert_eq!(summary.days[0].milliseconds,15000);assert_eq!(summary.days[0].characters,140);assert_eq!(summary.days[0].lookups,1);
    let backup=dir.path().join("backup.sqlite3");db.connection.lock().unwrap().backup(DatabaseName::Main,&backup,None).unwrap();
    let restored=Database::open(&backup).unwrap();assert_eq!(restored.activity_generation().unwrap(),generation);
    assert_eq!(restored.activity_sync(request(&ns,Some(&generation),json!([time("phone-time",&public,15000,25000)]))).unwrap()["events"],first["events"]);
    db.restore_from(&backup).unwrap();assert_ne!(db.activity_generation().unwrap(),generation);
    let replay=db.activity_sync(request(&ns,Some(&generation),json!([time("stale-after-restore",&public,25000,35000)]))).unwrap();
    assert!(replay["acknowledged"].as_array().unwrap().is_empty());assert!(!replay["events"].as_array().unwrap().is_empty());
}
#[test]
fn reset_discards_stale_queues_and_is_retry_safe_even_on_first_contact() {
    let (_dir,db,book,ns,public)=fixture();let old=db.activity_generation().unwrap();
    db.activity_sync(request(&ns,Some(&old),json!([time("before-clear",&public,10000,20000)]))).unwrap();
    let reset="c".repeat(32);
    let mut clear=request(&ns,None,json!([]));clear.reset=Some(reset.clone());
    assert_eq!(db.activity_sync(clear).unwrap()["epoch"],reset);
    let stale=db.activity_sync(request(&ns,Some(&old),json!([time("old-queue",&public,20000,30000)]))).unwrap();
    assert!(stale["acknowledged"].as_array().unwrap().is_empty());
    let unknown=db.activity_sync(request(&ns,None,json!([time("never-synced-old-queue",&public,20000,30000)]))).unwrap();
    assert_eq!(unknown["rejected"],json!(["never-synced-old-queue"]));
    let mut second_batch=time("second-old-batch",&public,30000,40000);second_batch["generation"]=Value::Null;
    let later=db.activity_sync(request(&ns,Some(&reset),json!([second_batch]))).unwrap();
    assert_eq!(later["rejected"],json!(["second-old-batch"]));
    let mut post_local_reset=time("post-local-reset",&public,60000,70000);post_local_reset["generation"]=json!(old);post_local_reset["resetId"]=json!(reset);
    assert_eq!(db.activity_sync(request(&ns,Some(&reset),json!([post_local_reset]))).unwrap()["acknowledged"],json!(["post-local-reset"]));
    db.record_activity(activity::ActivityCheckpoint{generation:Some(old),book_id:book,day:"2026-10-14".into(),start_ms:30000,end_ms:40000,milliseconds:10000}).unwrap();
    assert_eq!(db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap().days[0].milliseconds,10000);
    db.activity_sync(request(&ns,Some(&reset),json!([time("after-clear",&public,40000,50000)]))).unwrap();
    let mut retry=request(&ns,None,json!([]));retry.reset=Some(reset);db.activity_sync(retry).unwrap();
    assert_eq!(db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap().days[0].milliseconds,20000);
}
#[test]
fn pages_preserve_acknowledgements_and_reject_malformed_neighbours_atomically() {
    let (_dir,db,_book,ns,public)=fixture();let generation=db.activity_generation().unwrap();
    for batch in 0..5 {let ops:Vec<Value>=(0..64).map(|i|time(&format!("op-{}",batch*64+i),&public,(batch*64+i)*10000,(batch*64+i+1)*10000)).collect();db.activity_sync(request(&ns,Some(&generation),json!(ops))).unwrap();}
    let page=db.activity_sync(request(&ns,Some(&generation),json!([]))).unwrap();assert_eq!(page["hasMore"],true);assert_eq!(page["events"].as_array().unwrap().len(),256);
    let mut next=request(&ns,Some(&generation),json!([time("bad",&public,10,30000),time("good",&public,5000000,5010000)]));next.cursor=page["cursor"].as_i64().unwrap();
    let next=db.activity_sync(next).unwrap();assert_eq!(next["hasMore"],false);assert_eq!(next["rejected"],json!(["bad"]));assert_eq!(next["acknowledged"],json!(["good"]));
}
#[test]
fn measured_legacy_baselines_and_coverage_migrate_without_inferred_intervals() {
    let (dir,db,book,ns,_public)=fixture();
    {let c=db.connection.lock().unwrap();c.execute("INSERT INTO reading_activity(day,book_id,milliseconds,characters) VALUES('2026-10-13',?1,60000,100)",[book]).unwrap();c.execute("INSERT INTO reading_character_coverage VALUES(?1,'1:1',0,0,100)",[book]).unwrap();c.execute_batch("DROP TABLE activity_events;DROP TABLE activity_sync_state;DROP TABLE activity_time_coverage;DROP TABLE activity_reset_receipts;PRAGMA user_version=21;").unwrap();}
    drop(db);let migrated=Database::open(&dir.path().join("catalog.sqlite3")).unwrap();let events=migrated.activity_sync(request(&ns,None,json!([]))).unwrap();
    assert_eq!(events["events"].as_array().unwrap().len(),2);
    assert_eq!(events["events"][0]["milliseconds"],60000);
    assert_eq!(migrated.connection.lock().unwrap().query_row("SELECT count(*) FROM activity_time_coverage",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
#[test]
fn verified_hash_promotes_metadata_coverage_without_double_credit() {
    let (_dir,db,book,ns,public)=fixture();let key=db.reader_book(book).unwrap().unwrap().content_key;
    db.record_characters(activity::CharacterCheckpoint{generation:None,book_id:book,day:"2026-10-14".into(),content_key:key,spine:0,start_offset:0,end_offset:100}).unwrap();
    let version=format!("sha256-{}","a".repeat(64));db.remember_content_version(book,&version,1,1).unwrap();
    let result=db.activity_sync(request(&ns,None,json!([{"id":"phone","kind":"characters","day":"2026-10-14","bookId":public,"version":version,"spine":0,"startOffset":40,"endOffset":140}]))).unwrap();
    assert!(result["events"].as_array().unwrap().iter().any(|e|e["kind"]=="versionAlias"));
    assert_eq!(db.reading_activity("2026-10-01","2026-10-14","2026-10-14",0).unwrap().days[0].characters,140);
}
