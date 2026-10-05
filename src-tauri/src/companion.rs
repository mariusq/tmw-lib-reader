//! Opt-in, loopback-only private companion transport. No source path is accepted over HTTP.
use crate::{db::Database, models::book::BrowseBooksRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use uuid::Uuid;

const PORT: u16 = 47831;
const MAX_REQUEST: usize = 16_384;
const MAX_BODY: usize = 65_536;
const MAX_FILE: u64 = 512_000_000;
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing)]
    token_hash: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    pub address: String,
    pub devices: Vec<Device>,
}
struct PairCode {
    code: String,
    expires: Instant,
    attempts: usize,
}
struct Running {
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}
#[derive(Default)]
pub struct Service {
    running: Mutex<Option<Running>>,
    auth: Arc<Mutex<Auth>>,
}
#[derive(Default)]
struct Auth {
    devices: Vec<Device>,
    code: Option<PairCode>,
    store: PathBuf,
}
impl Auth {
    fn save(&self) -> Result<(), String> {
        // Only hashed bearer credentials are persisted. SQLite provides atomic durability.
        let db = rusqlite::Connection::open(&self.store)
            .map_err(|_| "Credential storage unavailable")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY, name TEXT NOT NULL, token_hash TEXT NOT NULL);").map_err(|_| "Credential storage unavailable")?;
        let tx = db
            .unchecked_transaction()
            .map_err(|_| "Credential storage unavailable")?;
        tx.execute("DELETE FROM devices", [])
            .map_err(|_| "Credential storage unavailable")?;
        for d in &self.devices {
            tx.execute(
                "INSERT INTO devices VALUES(?1,?2,?3)",
                rusqlite::params![d.id, d.name, d.token_hash],
            )
            .map_err(|_| "Credential storage unavailable")?;
        }
        tx.commit()
            .map_err(|_| "Credential storage unavailable".into())
    }
    fn authorized(&self, token: &str) -> bool {
        if token.len() != 64 {
            return false;
        }
        let digest = hash(token.as_bytes());
        self.devices.iter().any(|d| {
            if d.token_hash.len() != digest.len() {
                return false;
            }
            d.token_hash
                .as_bytes()
                .iter()
                .zip(digest.as_bytes())
                .fold(0u8, |a, (x, y)| a | (x ^ y))
                == 0
        })
    }
}
impl Service {
    pub fn status(&self) -> Status {
        Status {
            enabled: self.running.lock().unwrap().is_some(),
            address: format!("127.0.0.1:{PORT}"),
            devices: self.auth.lock().unwrap().devices.clone(),
        }
    }
    pub fn enable(&self, catalog: PathBuf, store: PathBuf) -> Result<Status, String> {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            drop(running);
            return Ok(self.status());
        }
        let listener = TcpListener::bind(("127.0.0.1", PORT))
            .map_err(|_| "Loopback service port unavailable")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "Service unavailable")?;
        let db =
            rusqlite::Connection::open(&store).map_err(|_| "Credential storage unavailable")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS devices(id TEXT PRIMARY KEY,name TEXT NOT NULL,token_hash TEXT NOT NULL);").map_err(|_| "Credential storage unavailable")?;
        let devices = db
            .prepare("SELECT id,name,token_hash FROM devices")
            .map_err(|_| "Credential storage unavailable")?
            .query_map([], |r| {
                Ok(Device {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    token_hash: r.get(2)?,
                })
            })
            .map_err(|_| "Credential storage unavailable")?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "Credential storage unavailable")?;
        *self.auth.lock().unwrap() = Auth {
            devices,
            code: None,
            store,
        };
        let stop = Arc::new(AtomicBool::new(false));
        let halt = stop.clone();
        let auth = self.auth.clone();
        let join = std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            while !halt.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        if prepare_accepted_socket(&socket).is_err() {
                            continue;
                        }
                        if active.load(Ordering::Relaxed) >= 4 {
                            let _ = socket.set_write_timeout(Some(Duration::from_secs(1)));
                            let _ = reply(&mut socket, 503, json!({"error":"busy"}));
                            continue;
                        }
                        active.fetch_add(1, Ordering::Relaxed);
                        let active = active.clone();
                        let auth = auth.clone();
                        let catalog = catalog.clone();
                        let halt = halt.clone();
                        std::thread::spawn(move || {
                            let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
                            let _ = socket.set_write_timeout(Some(Duration::from_secs(5)));
                            if let Err((code, message)) =
                                handle(&mut socket, &catalog, &auth, &halt)
                            {
                                let _ = reply(&mut socket, code, json!({"error":message}));
                            }
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(25))
                    }
                    Err(_) => break,
                }
            }
        });
        *running = Some(Running { stop, join });
        drop(running);
        Ok(self.status())
    }
    pub fn disable(&self) {
        if let Some(r) = self.running.lock().unwrap().take() {
            r.stop.store(true, Ordering::Relaxed);
            let _ = r.join.join();
        }
        self.auth.lock().unwrap().code = None;
    }
    pub fn pairing_code(&self) -> Result<String, String> {
        if self.running.lock().unwrap().is_none() {
            return Err("Enable the service first".into());
        }
        let code = Uuid::new_v4().simple().to_string()[..12].to_owned();
        self.auth.lock().unwrap().code = Some(PairCode {
            code: code.clone(),
            expires: Instant::now() + Duration::from_secs(120),
            attempts: 0,
        });
        Ok(code)
    }
    pub fn revoke(&self, id: &str) -> Result<(), String> {
        let mut auth = self.auth.lock().unwrap();
        let old = auth.devices.clone();
        auth.devices.retain(|d| d.id != id);
        if let Err(e) = auth.save() {
            auth.devices = old;
            return Err(e);
        }
        Ok(())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.disable();
    }
}
type Error = (u16, &'static str);
fn prepare_accepted_socket(socket: &TcpStream) -> std::io::Result<()> {
    // Windows accept inherits the listener's nonblocking mode. Workers use
    // blocking read_exact/write_all with bounded timeouts, including EPUB bodies.
    socket.set_nonblocking(false)
}
fn reply(s: &mut TcpStream, status: u16, value: Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(&value)?;
    header(s, status, "application/json", body.len() as u64, None)?;
    s.write_all(&body)
}
fn header(
    s: &mut TcpStream,
    status: u16,
    mime: &str,
    len: u64,
    etag: Option<&str>,
) -> std::io::Result<()> {
    write!(s,"HTTP/1.1 {status} Response\r\nContent-Type: {mime}\r\nContent-Length: {len}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n")?;
    if let Some(v) = etag {
        write!(s, "ETag: \"{v}\"\r\n")?;
    }
    write!(s, "\r\n")
}
struct Request {
    method: String,
    path: String,
    token: String,
    body: Vec<u8>,
    version: Option<String>,
}
fn request(s: &mut TcpStream) -> Result<Request, Error> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut bytes = Vec::new();
    let mut b = [0u8; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        s.set_read_timeout(Some(
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1)),
        ))
        .map_err(|_| (400, "invalid_request"))?;
        if Instant::now() >= deadline {
            return Err((408, "request_timeout"));
        }
        if bytes.len() >= MAX_REQUEST {
            return Err((413, "headers_too_large"));
        }
        s.read_exact(&mut b).map_err(|_| (400, "invalid_request"))?;
        bytes.push(b[0]);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| (400, "invalid_headers"))?;
    let mut lines = text.split("\r\n");
    let first: Vec<_> = lines.next().unwrap_or("").split(' ').collect();
    if first.len() != 3
        || first[2] != "HTTP/1.1"
        || first[1].len() > 1024
        || !first[1].starts_with('/')
    {
        return Err((400, "invalid_request"));
    }
    let mut len = 0;
    let mut token = String::new();
    let mut version = None;
    let mut seen = std::collections::HashSet::new();
    for l in lines.filter(|l| !l.is_empty()) {
        let (k, v) = l.split_once(':').ok_or((400, "invalid_headers"))?;
        let k = k.to_ascii_lowercase();
        if !seen.insert(k.clone()) {
            return Err((400, "duplicate_header"));
        }
        let v = v.trim();
        match k.as_str() {
            "content-length" => {
                len = v.parse::<usize>().map_err(|_| (400, "invalid_length"))?;
            }
            "transfer-encoding" | "origin" => return Err((400, "unsupported_header")),
            "authorization" => token = v.strip_prefix("Bearer ").unwrap_or("").to_owned(),
            "if-match" => version = Some(v.trim_matches('"').to_owned()),
            _ => {}
        }
    }
    if len > MAX_BODY {
        return Err((413, "body_too_large"));
    }
    let mut body = vec![0; len];
    let mut read = 0;
    while read < len {
        if Instant::now() >= deadline {
            return Err((408, "request_timeout"));
        }
        s.set_read_timeout(Some(
            deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(1)),
        ))
        .map_err(|_| (400, "invalid_body"))?;
        let n = s
            .read(&mut body[read..])
            .map_err(|_| (400, "invalid_body"))?;
        if n == 0 {
            return Err((400, "invalid_body"));
        }
        read += n;
    }
    Ok(Request {
        method: first[0].into(),
        path: first[1].into(),
        token,
        body,
        version,
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Pair {
    code: String,
    name: String,
}
fn handle(
    s: &mut TcpStream,
    catalog: &Path,
    auth: &Arc<Mutex<Auth>>,
    stop: &AtomicBool,
) -> Result<(), Error> {
    let r = request(s)?;
    let request_started = Instant::now();
    if stop.load(Ordering::Relaxed) {
        return Err((503, "disabled"));
    }
    if r.path == "/v1/pair" && r.method == "POST" {
        let p: Pair = serde_json::from_slice(&r.body).map_err(|_| (400, "invalid_pairing"))?;
        if p.code.len() != 12
            || p.name.trim().is_empty()
            || p.name.len() > 80
            || p.name.chars().any(char::is_control)
        {
            return Err((400, "invalid_pairing"));
        }
        let mut a = auth.lock().unwrap();
        let code = a.code.as_mut().ok_or((403, "pairing_closed"))?;
        code.attempts += 1;
        if code.expires < Instant::now() || code.attempts > 5 || code.code != p.code {
            if code.attempts >= 5 {
                a.code = None;
            }
            return Err((403, "pairing_denied"));
        }
        if a.devices.len() >= 16 {
            return Err((409, "device_limit"));
        }
        let token = secret();
        let id = Uuid::new_v4().simple().to_string();
        a.devices.push(Device {
            id: id.clone(),
            name: p.name,
            token_hash: hash(token.as_bytes()),
        });
        if a.save().is_err() {
            a.devices.pop();
            return Err((503, "storage_unavailable"));
        }
        a.code = None;
        return reply(
            s,
            201,
            json!({"protocolVersion":1,"deviceId":id,"token":token}),
        )
        .map_err(|_| (500, "response_failed"));
    }
    if !auth.lock().unwrap().authorized(&r.token) {
        return Err((401, "unauthorized"));
    }
    if r.path == "/v1/device" && r.method == "DELETE" {
        let mut a = auth.lock().unwrap();
        let old = a.devices.clone();
        let digest = hash(r.token.as_bytes());
        a.devices.retain(|d| d.token_hash != digest);
        if a.save().is_err() {
            a.devices = old;
            return Err((503, "storage_unavailable"));
        }
        return reply(s, 200, json!({"protocolVersion":1,"revoked":true}))
            .map_err(|_| (500, "response_failed"));
    }
    if r.path == "/v3/user-sync" && r.method == "POST" {
        let q:crate::db::user_sync::SyncRequest=serde_json::from_slice(&r.body).map_err(|_|(400,"invalid_sync"))?;
        if q.operations.len()>16 {return Err((400,"sync_bounds"))}
        let device=auth.lock().unwrap().devices.iter().find(|d|d.token_hash==hash(r.token.as_bytes())).map(|d|d.id.clone()).ok_or((401,"unauthorized"))?;
        let db=Database::open_api_writer(catalog).map_err(|_|(503,"catalog_busy"))?;
        // Strong versions are verified from read-only source handles outside the write transaction.
        let mut verified=std::collections::HashSet::new();
        for op in &q.operations {
            if op.kind=="history" {continue}
            if op.kind=="passage" && (op.deleted || op.fields.keys().all(|key|matches!(key.as_str(),"sentence"|"note"))) {continue}
            if !verified.insert(&op.book_id) {continue}
            let Some(id)=db.private_id(&op.book_id).map_err(|_|(503,"catalog_busy"))? else{continue};
            db.forget_content_version(id).map_err(|_|(503,"catalog_busy"))?;
            let Some(d)=db.book_details(id).map_err(|_|(503,"catalog_busy"))? else{continue};
            if d.book.extraction_status=="unavailable" {continue}
            let Some(root)=db.library_root(d.book.library_root_id).map_err(|_|(503,"catalog_busy"))? else{continue};
            let Ok(path)=isolated_source(Path::new(&d.book.file_path),Path::new(&root.path)) else{continue};
            let Ok(mut file)=source_file(&path) else{continue};
            let before=file.metadata().map_err(|_|(409,"source_changed"))?;
            if before.len()>MAX_FILE{return Err((413,"file_bounds"))}
            let mut digest=Sha256::new();let mut buffer=[0u8;65536];let mut total=0u64;
            loop {check_client(auth,&r.token,stop)?;if request_started.elapsed()>Duration::from_secs(120){return Err((408,"request_timeout"))}let n=file.read(&mut buffer).map_err(|_|(409,"source_changed"))?;if n==0{break}total+=n as u64;if total>MAX_FILE{return Err((413,"file_bounds"))}digest.update(&buffer[..n]);}
            if total!=before.len() || file.metadata().ok().and_then(|m|m.modified().ok())!=before.modified().ok(){return Err((409,"source_changed"))}
            let modified=before.modified().ok().and_then(|m|m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_secs() as i64).ok_or((409,"source_changed"))?;
            db.remember_content_version(id,&format!("sha256-{:x}",digest.finalize()),total as i64,modified).map_err(|_|(503,"catalog_busy"))?;
        }
        check_client(auth,&r.token,stop)?;
        let result=db.user_sync(&device,q).map_err(|e|(if e=="catalog_busy"{503}else{409},e))?;
        return reply(s,200,result).map_err(|_|(500,"response_failed"));
    }
    let db = Database::open_api_reader(catalog).map_err(|_| (503, "catalog_busy"))?;
    if r.path == "/v2/catalog" && r.method == "POST" {
        let q = serde_json::from_slice(&r.body).map_err(|_| (400, "invalid_cursor"))?;
        let page = db
            .catalog_page(q)
            .map_err(|e| (if e == "restart_snapshot" { 409 } else { 503 }, e))?;
        return reply(s, 200, page).map_err(|_| (500, "response_failed"));
    }
    if !r.path.starts_with("/v1/") && !r.path.starts_with("/v2/books/") {
        return Err((426, "unsupported_protocol"));
    }
    if r.path == "/v1/status" && r.method == "GET" {
        return reply(
            s,
            200,
            json!({"protocolVersion":1,"connected":true,"maxPageSize":100,"catalogId":db.setting("companion_catalog_id").map_err(|_|(503,"catalog_busy"))?}),
        )
        .map_err(|_| (500, "response_failed"));
    }
    if r.path == "/v1/catalog" && r.method == "POST" {
        let q: BrowseBooksRequest =
            serde_json::from_slice(&r.body).map_err(|_| (400, "invalid_query"))?;
        if !(1..=100).contains(&q.limit)
            || !(0..=1_000_000).contains(&q.offset)
            || q.query.len() > 512
            || ![
                "title",
                "author",
                "series",
                "dateAdded",
                "modified",
                "folder",
            ]
            .contains(&q.sort.as_str())
        {
            return Err((400, "query_bounds"));
        }
        let books = db.browse_books(&q).map_err(|_| (503, "catalog_busy"))?;
        let mut result = Vec::new();
        for b in books {
            let id = db.public_id(b.id).map_err(|_| (503, "catalog_busy"))?;
            result.push(json!({"id":id,"title":bounded(&b.effective_title),"creator":bounded(&b.effective_creator),"series":bounded(&b.effective_series),"volume":bounded(&b.effective_volume),"available":b.is_available,"hasCover":b.effective_cover_path.is_some(),"finished":b.is_finished}));
        }
        let next = if result.len() == q.limit as usize {
            Some(q.offset + q.limit)
        } else {
            None
        };
        return reply(
            s,
            200,
            json!({"protocolVersion":1,"items":result,"nextOffset":next,"consistency":"live"}),
        )
        .map_err(|_| (500, "response_failed"));
    }
    let parts: Vec<_> = r.path.split('/').collect();
    if r.method != "GET"
        || parts.len() != 5
        || !(parts[1] == "v1" || (parts[1] == "v2" && parts[4] == "cover"))
        || parts[2] != "books"
        || parts[3].len() != 32
        || !parts[3].bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err((404, "not_found"));
    }
    let id = db
        .private_id(parts[3])
        .map_err(|_| (503, "catalog_busy"))?
        .ok_or((404, "not_found"))?;
    let d = db
        .book_details(id)
        .map_err(|_| (503, "catalog_busy"))?
        .ok_or((404, "not_found"))?;
    match parts[4] {
        "metadata" => reply(s,200,json!({"protocolVersion":1,"id":parts[3],"title":bounded(&d.effective_title),"creator":bounded(&d.effective_creator),"series":bounded(&d.effective_series),"volume":bounded(&d.effective_volume),"tags":d.tags.into_iter().take(100).map(|(id,name)|json!({"id":id,"name":bounded(&name)})).collect::<Vec<_>>(),"available":d.book.extraction_status!="unavailable","bytes":d.book.file_size,"versionEndpoint":format!("/v1/books/{}/content",parts[3])})).map_err(|_|(500,"response_failed")),
        "epub" | "content" => {
            if d.book.extraction_status == "unavailable" {
                return Err((404, "unavailable"));
            }
            let root = db
                .library_root(d.book.library_root_id)
                .map_err(|_| (503, "catalog_busy"))?
                .ok_or((404, "unavailable"))?;
            let path = isolated_source(Path::new(&d.book.file_path), Path::new(&root.path))?;
            let mut file = source_file(&path).map_err(|_| (404, "unavailable"))?;
            let metadata = file.metadata().map_err(|_| (404, "unavailable"))?;
            if !metadata.is_file() || metadata.len() > MAX_FILE {
                return Err((413, "file_bounds"));
            }
            // Hash read-only on a network worker, outside SQLite locks. Never use size/mtime as identity.
            let mut digest = Sha256::new();
            let mut buf = [0u8; 65536];
            let mut total = 0u64;
            loop {
                if request_started.elapsed()>Duration::from_secs(120) {return Err((408,"request_timeout"));}
                check_client(auth, &r.token, stop)?;
                let n = file.read(&mut buf).map_err(|_| (409, "source_changed"))?;
                if n == 0 {
                    break;
                }
                total += n as u64;
                if total > MAX_FILE {
                    return Err((413, "file_bounds"));
                }
                digest.update(&buf[..n]);
            }
            let version = format!("sha256-{:x}", digest.finalize());
            if total != metadata.len()
                || file.metadata().ok().and_then(|m| m.modified().ok()) != metadata.modified().ok()
            {
                return Err((409, "source_changed"));
            }
            if parts[4] == "content" {
                let modified=metadata.modified().ok().and_then(|m|m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_secs() as i64).ok_or((409,"source_changed"))?;
                Database::open_api_writer(catalog).and_then(|writer|writer.remember_content_version(id,&version,total as i64,modified)).map_err(|_|(503,"catalog_busy"))?;
                return reply(
                    s,
                    200,
                    json!({"protocolVersion":1,"id":parts[3],"contentVersion":version,"bytes":total}),
                )
                .map_err(|_| (500, "response_failed"));
            }
            if r.version.as_deref() != Some(&version) {
                return Err((412, "content_version_required"));
            }
            use std::io::{Seek, SeekFrom};
            file.seek(SeekFrom::Start(0))
                .map_err(|_| (409, "source_changed"))?;
            // Allow bounded backpressure from the private HTTPS proxy/phone.
            s.set_write_timeout(Some(Duration::from_secs(30)))
                .map_err(|_| (500, "response_failed"))?;
            header(s, 200, "application/epub+zip", total, Some(&version))
                .map_err(|_| (500, "response_failed"))?;
            let mut transferred = 0;
            loop {
                if request_started.elapsed()>Duration::from_secs(120) || check_client(auth, &r.token, stop).is_err() {
                    let _ = s.shutdown(std::net::Shutdown::Both);
                    return Ok(());
                }
                let n = match file.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => {
                        let _ = s.shutdown(std::net::Shutdown::Both);
                        return Ok(());
                    }
                };
                if n == 0 {
                    break;
                }
                transferred += n as u64;
                if transferred > total {
                    let _ = s.shutdown(std::net::Shutdown::Both);
                    return Ok(());
                }
                if s.write_all(&buf[..n]).is_err() {
                    // Headers have already been sent. Close the truncated response;
                    // never append a second HTTP response to the EPUB bytes.
                    let _ = s.shutdown(std::net::Shutdown::Both);
                    return Ok(());
                }
            }
            Ok(())
        }
        "cover" => {
            let path = d.effective_cover_path.ok_or((404, "no_cover"))?;
            if parts[1]=="v2" && r.version != crate::db::companion::cover_key(&path) {
                return Err((412,"cover_version_changed"));
            }
            let file = File::open(&path).map_err(|_| (404, "no_cover"))?;
            if file.metadata().map_err(|_| (404, "no_cover"))?.len() > 16_000_000 {
                return Err((413, "cover_bounds"));
            }
            let mut reader = image::ImageReader::new(std::io::BufReader::new(file))
                .with_guessed_format()
                .map_err(|_| (422, "invalid_cover"))?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            limits.max_alloc = Some(64_000_000);
            reader.limits(limits);
            let image = reader
                .decode()
                .map_err(|_| (422, "invalid_cover"))?
                .thumbnail(240, 360)
                .to_rgb8();
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 75)
                .encode_image(&image)
                .map_err(|_| (422, "invalid_cover"))?;
            if parts[1]=="v2" && r.version != crate::db::companion::cover_key(&path) {return Err((412,"cover_version_changed"));}
            check_client(auth, &r.token, stop)?;
            header(
                s,
                200,
                "image/jpeg",
                bytes.len() as u64,
                Some(&format!("cover-v1-{}", hash(&bytes))),
            )
            .map_err(|_| (500, "response_failed"))?;
            s.write_all(&bytes).map_err(|_| (500, "response_failed"))
        }
        _ => Err((404, "not_found")),
    }
}
fn check_client(auth: &Mutex<Auth>, token: &str, stop: &AtomicBool) -> Result<(), Error> {
    if stop.load(Ordering::Relaxed) || !auth.lock().unwrap().authorized(token) {
        Err((401, "unauthorized"))
    } else {
        Ok(())
    }
}
fn isolated_source(path: &Path, root: &Path) -> Result<PathBuf, Error> {
    let root = root.canonicalize().map_err(|_| (404, "unavailable"))?;
    let path = path.canonicalize().map_err(|_| (404, "unavailable"))?;
    if !path.starts_with(root)
        || path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| !s.eq_ignore_ascii_case("epub"))
            .unwrap_or(true)
    {
        return Err((403, "source_isolation"));
    }
    Ok(path)
}

fn bounded(value: &str) -> String {
    value.chars().take(512).collect()
}
fn source_file(path: &Path) -> std::io::Result<File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(1);
    }
    options.open(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn accepted_socket_streams_beyond_send_buffer_to_slow_reader() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            // Simulate Windows inheritance on every test platform.
            socket.set_nonblocking(true).unwrap();
            prepare_accepted_socket(&socket).unwrap();
            socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            let chunk = [0x5au8; 65536];
            for _ in 0..64 {
                socket.write_all(&chunk).unwrap();
            }
        });
        let mut reader = TcpStream::connect(address).unwrap();
        reader.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let mut chunk = [0u8; 16384];
        let mut received = 0;
        loop {
            let n = reader.read(&mut chunk).unwrap();
            if n == 0 { break; }
            assert!(chunk[..n].iter().all(|byte| *byte == 0x5a));
            received += n;
            std::thread::sleep(Duration::from_millis(1));
        }
        writer.join().unwrap();
        assert_eq!(received, 4 * 1024 * 1024);
    }
    use super::*;
    use crate::models::{
        book::{BookOverride, NewBook},
        library_root::NewLibraryRoot,
    };
    fn http(method: &str, path: &str, token: &str, body: Value, extra: &str) -> (u16, Vec<u8>) {
        let mut socket = TcpStream::connect(("127.0.0.1", PORT)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let bytes = if method == "POST" {
            serde_json::to_vec(&body).unwrap()
        } else {
            vec![]
        };
        write!(socket,"{method} {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\n{extra}\r\n",bytes.len()).unwrap();
        socket.write_all(&bytes).unwrap();
        let mut response = vec![];
        socket.read_to_end(&mut response).unwrap();
        let end = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let status = std::str::from_utf8(&response[..end])
            .unwrap()
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .unwrap();
        (status, response[end..].to_vec())
    }
    fn json_response(method: &str, path: &str, token: &str, body: Value) -> (u16, Value) {
        let (s, b) = http(method, path, token, body, "");
        (s, serde_json::from_slice(&b).unwrap())
    }
    fn query(offset: i64, limit: i64, term: &str) -> Value {
        json!({"libraryRootId":null,"tagId":null,"collectionId":null,"needsMetadata":false,"query":term,"sort":"title","offset":offset,"limit":limit})
    }
    fn fixture(path: &Path) {
        use zip::write::SimpleFileOptions;
        let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data) in [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                r#"<container><rootfiles><rootfile full-path="content.opf"/></rootfiles></container>"#,
            ),
            (
                "content.opf",
                r#"<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata><dc:title>猫の本</dc:title><dc:creator>テスト</dc:creator></metadata><manifest><item id="text" href="text.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="text"/></spine></package>"#,
            ),
            ("text.xhtml", "<html><body><p>猫。</p></body></html>"),
        ] {
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(data.as_bytes()).unwrap();
        }
        archive.finish().unwrap();
    }
    #[test]
    fn private_api_pairing_isolation_import_and_versions() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        std::fs::create_dir(&source).unwrap();
        for n in 0..512 {
            fixture(&source.join(format!("book-{n:04}.epub")));
        }
        let bytes_before = std::fs::read(source.join("book-0000.epub")).unwrap();
        let catalog = tmp.path().join("catalog.sqlite3");
        let database = Arc::new(Database::open(&catalog).unwrap());
        let root = database
            .add_library_root(NewLibraryRoot {
                path: source.to_str().unwrap(),
                display_name: "test",
            })
            .unwrap();
        let service = Service::default();
        assert!(!service.status().enabled);
        assert!(service.pairing_code().is_err());
        service
            .enable(catalog.clone(), tmp.path().join("devices.sqlite3"))
            .unwrap();
        assert_eq!(json_response("GET", "/v1/status", "", Value::Null).0, 401);
        let code = service.pairing_code().unwrap();
        assert_eq!(
            json_response(
                "POST",
                "/v1/pair",
                "",
                json!({"code":"000000000000","name":"test"})
            )
            .0,
            403
        );
        let (status, paired) = json_response(
            "POST",
            "/v1/pair",
            "",
            json!({"code":code,"name":"test phone"}),
        );
        assert_eq!(status, 201);
        let token = paired["token"].as_str().unwrap();
        let device = paired["deviceId"].as_str().unwrap();
        assert_eq!(
            json_response("POST", "/v1/pair", "", json!({"code":code,"name":"repeat"})).0,
            403
        );
        assert_eq!(
            json_response("GET", "/v1/status", token, Value::Null).0,
            200
        );
        assert_eq!(
            json_response("GET", "/v2/status", token, Value::Null).0,
            426
        );
        let anchor = source.join("book-0000.epub");
        let seed = database
            .upsert_scanned_book(NewBook {
                library_root_id: root.id,
                file_path: anchor.to_str().unwrap(),
                parent_folder_path: source.to_str().unwrap(),
                file_name: "book-0000.epub",
                file_size: bytes_before.len() as i64,
                modified_time: 1,
            })
            .unwrap();
        let seed_id = database.public_id(seed.book_id).unwrap();
        let seed_cover = tmp.path().join("seed-cover.png");
        image::RgbImage::new(240, 360).save(&seed_cover).unwrap();
        database
            .save_book_overrides(
                seed.book_id,
                &BookOverride {
                    cover_path: Some(seed_cover.to_str().unwrap().into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let done = Arc::new(AtomicBool::new(false));
        let done_worker = done.clone();
        let writer = database.clone();
        let root_path = source.clone();
        let cache = tmp.path().join("cache");
        let import = std::thread::spawn(move || {
            let start = Instant::now();
            crate::services::scanner::test_import(&writer, root.id, &root_path, &cache).unwrap();
            done_worker.store(true, Ordering::Release);
            start.elapsed()
        });
        let (status, version) = json_response(
            "GET",
            &format!("/v1/books/{seed_id}/content"),
            token,
            Value::Null,
        );
        assert_eq!(status, 200);
        assert!(
            !done.load(Ordering::Acquire),
            "generated import should overlap representative requests"
        );
        assert_eq!(
            http(
                "GET",
                &format!("/v1/books/{seed_id}/epub"),
                token,
                Value::Null,
                &format!(
                    "If-Match: \"{}\"\r\n",
                    version["contentVersion"].as_str().unwrap()
                )
            )
            .0,
            200
        );
        assert_eq!(
            http(
                "GET",
                &format!("/v1/books/{seed_id}/cover"),
                token,
                Value::Null,
                ""
            )
            .0,
            200
        );
        let mut overlapped = 0;
        let mut max = Duration::ZERO;
        while !done.load(Ordering::Acquire) {
            let start = Instant::now();
            let (status, _) = if overlapped % 2 == 0 {
                json_response(
                    "POST",
                    "/v2/catalog",
                    token,
                    json!({"after":"","since":0,"delta":false}),
                )
            } else {
                json_response("POST", "/v1/catalog", token, query(0, 10, "猫"))
            };
            assert_eq!(status, 200);
            max = max.max(start.elapsed());
            overlapped += 1;
            if overlapped > 200 {
                break;
            }
        }
        let elapsed = import.join().unwrap();
        assert!(overlapped > 0);
        assert_eq!(json_response("POST", "/v2/catalog", "", json!({})).0, 401);
        assert_eq!(
            json_response("POST", "/v2/catalog", token, json!({"path":"secret"})).0,
            400
        );
        let (_, snapshot) = json_response("POST", "/v2/catalog", token, json!({}));
        assert_eq!(snapshot["protocolVersion"], 2);
        assert!(snapshot["items"][0].get("collections").is_some());
        assert!(snapshot["items"][0].get("contentVersionEndpoint").is_some());
        eprintln!("phase3 import: {} ms; concurrent API requests: {overlapped}; max catalog request: {} ms",elapsed.as_millis(),max.as_millis());
        let (status, page) = json_response("POST", "/v1/catalog", token, query(0, 2, "猫"));
        assert_eq!(status, 200);
        assert_eq!(page["items"].as_array().unwrap().len(), 2);
        assert_eq!(page["nextOffset"], 2);
        assert!(!page.to_string().contains(source.to_str().unwrap()));
        let id = page["items"][0]["id"].as_str().unwrap();
        let local = database.private_id(id).unwrap().unwrap();
        database
            .save_book_overrides(
                local,
                &BookOverride {
                    title: Some("Manual override".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            json_response(
                "POST",
                "/v1/catalog",
                token,
                query(0, 10, "manual override")
            )
            .1["items"][0]["id"],
            id
        );
        assert_eq!(
            json_response("POST", "/v1/catalog", token, query(0, 101, "")).0,
            400
        );
        let mut malicious = query(0, 10, "");
        malicious["path"] = json!(source.to_str().unwrap());
        assert_eq!(
            json_response("POST", "/v1/catalog", token, malicious).0,
            400
        );
        for path in [
            "/v1/books/../../secret/epub",
            "/v1/books/%2e%2e/epub",
            "/v1/books/1/epub",
            "/v1/books/00000000000000000000000000000000/epub",
        ] {
            assert_eq!(json_response("GET", path, token, Value::Null).0, 404);
        }
        let base = format!("/v1/books/{id}");
        assert_eq!(
            json_response("GET", &format!("{base}/metadata"), token, Value::Null).0,
            200
        );
        let (status, content) =
            json_response("GET", &format!("{base}/content"), token, Value::Null);
        assert_eq!(status, 200);
        let version = content["contentVersion"].as_str().unwrap();
        assert!(version.starts_with("sha256-"));
        let namespace=database.setting("companion_catalog_id").unwrap().unwrap();
        let sync=json!({"catalogId":namespace,"cursor":0,"operations":[{"id":"http-progress","sequence":1,"bookId":id,"kind":"progress","contentVersion":version,"fields":{"locationCfi":"epubcfi(/6/2)"}}]});
        assert_eq!(json_response("POST","/v3/user-sync","",sync.clone()).0,401);
        let (status,first)=json_response("POST","/v3/user-sync",token,sync.clone());
        assert_eq!(status,200,"{first}");assert_eq!(first["acknowledged"],json!(["http-progress"]));
        let (status,retry)=json_response("POST","/v3/user-sync",token,sync);
        assert_eq!(status,200);assert_eq!(retry["cursor"],first["cursor"]);
        assert_eq!(database.reading_location(local).unwrap().unwrap(),"epubcfi(/6/2)");
        let (status,reset)=json_response("POST","/v3/user-sync",token,json!({"catalogId":namespace,"epoch":"old-backup","cursor":0}));
        assert_eq!(status,409);assert_eq!(reset["error"],"cursor_reset");
        // Generated oversized source: existing notes/deletions need no source read.
        let oversized_dir=tmp.path().join("oversized-source");std::fs::create_dir(&oversized_dir).unwrap();
        let oversized_path=oversized_dir.join("oversized.epub");File::create(&oversized_path).unwrap().set_len(MAX_FILE+1).unwrap();
        let oversized_root=database.add_library_root(NewLibraryRoot{path:oversized_dir.to_str().unwrap(),display_name:"Generated oversized fixture"}).unwrap();
        let oversized_book=database.upsert_scanned_book(NewBook{library_root_id:oversized_root.id,file_path:oversized_path.to_str().unwrap(),parent_folder_path:oversized_dir.to_str().unwrap(),file_name:"oversized.epub",file_size:(MAX_FILE+1) as i64,modified_time:1}).unwrap();
        let pid=database.save_passage(&crate::models::book::SavePassageRequest{book_id:oversized_book.book_id,surface:"本".into(),headword:None,reading:None,sentence:"context".into(),note:"original".into(),location_cfi:String::new()}).unwrap();
        let oversized_public=database.public_id(oversized_book.book_id).unwrap();
        let entity:String=rusqlite::Connection::open(&catalog).unwrap().query_row("SELECT sync_id FROM saved_passages WHERE id=?1",[pid],|r|r.get(0)).unwrap();
        let (status,edited)=json_response("POST","/v3/user-sync",token,json!({"catalogId":namespace,"operations":[{"id":"source-independent-note","sequence":2,"bookId":oversized_public,"kind":"passage","entityId":entity,"contentVersion":null,"fields":{"note":"phone note"}}]}));
        assert_eq!(status,200,"{edited}");assert_eq!(edited["acknowledged"],json!(["source-independent-note"]));
        assert_eq!(database.saved_passages(Some(oversized_book.book_id),0).unwrap()[0].note,"phone note");
        let (status,deleted)=json_response("POST","/v3/user-sync",token,json!({"catalogId":namespace,"operations":[{"id":"source-independent-delete","sequence":3,"bookId":oversized_public,"kind":"passage","entityId":entity,"contentVersion":null,"deleted":true}]}));
        assert_eq!(status,200,"{deleted}");assert_eq!(deleted["acknowledged"],json!(["source-independent-delete"]));
        assert_eq!(
            json_response("GET", &format!("{base}/epub"), token, Value::Null).0,
            412
        );
        assert_eq!(
            http(
                "GET",
                &format!("{base}/epub"),
                token,
                Value::Null,
                "If-Match: \"wrong\"\r\n"
            )
            .0,
            412
        );
        let (status, download) = http(
            "GET",
            &format!("{base}/epub"),
            token,
            Value::Null,
            &format!("If-Match: \"{version}\"\r\n"),
        );
        assert_eq!(status, 200);
        assert_eq!(download, bytes_before);
        let cover = tmp.path().join("test-cover.png");
        image::RgbImage::new(800, 1200).save(&cover).unwrap();
        database
            .save_book_overrides(
                local,
                &BookOverride {
                    cover_path: Some(cover.to_str().unwrap().into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let (status, cover_bytes) = http("GET", &format!("{base}/cover"), token, Value::Null, "");
        assert_eq!(status, 200);
        let decoded = image::load_from_memory(&cover_bytes).unwrap();
        assert!(decoded.width() <= 240 && decoded.height() <= 360);
        assert_eq!(
            http("GET", &format!("{base}/cover"), "", Value::Null, "").0,
            401
        );
        let unrelated = tmp.path().join("unrelated.epub");
        fixture(&unrelated);
        let evil = database
            .upsert_scanned_book(NewBook {
                library_root_id: root.id,
                file_path: unrelated.to_str().unwrap(),
                parent_folder_path: tmp.path().to_str().unwrap(),
                file_name: "unrelated.epub",
                file_size: 1,
                modified_time: 1,
            })
            .unwrap();
        let evil_id = database.public_id(evil.book_id).unwrap();
        assert_eq!(
            json_response(
                "GET",
                &format!("/v1/books/{evil_id}/content"),
                token,
                Value::Null
            )
            .0,
            403
        );
        let backup = tmp.path().join("backup.sqlite3");
        database.backup_to(&backup).unwrap();
        database.restore_from(&backup).unwrap();
        assert_eq!(database.public_id(local).unwrap(), id);
        assert_eq!(
            std::fs::read(source.join("book-0000.epub")).unwrap(),
            bytes_before
        );
        assert_eq!(std::fs::read_dir(&source).unwrap().count(), 512);
        service.disable();
        assert!(!service.status().enabled);
        service
            .enable(catalog, tmp.path().join("devices.sqlite3"))
            .unwrap();
        assert_eq!(
            json_response("GET", "/v1/status", token, Value::Null).0,
            200
        );
        service.revoke(device).unwrap();
        for path in [
            "/v1/status",
            &format!("{base}/epub"),
            &format!("{base}/cover"),
        ] {
            assert_eq!(json_response("GET", path, token, Value::Null).0, 401);
        }
        let code = service.pairing_code().unwrap();
        let (_, paired) = json_response(
            "POST",
            "/v1/pair",
            "",
            json!({"code":code,"name":"self revoke"}),
        );
        let token = paired["token"].as_str().unwrap();
        assert_eq!(
            json_response("DELETE", "/v1/device", token, Value::Null).0,
            200
        );
        assert_eq!(
            json_response("GET", "/v1/status", token, Value::Null).0,
            401
        );
        service.disable();
        service
            .enable(
                tmp.path().join("catalog.sqlite3"),
                tmp.path().join("devices.sqlite3"),
            )
            .unwrap();
        assert!(service.status().devices.is_empty());
        assert_eq!(
            json_response("GET", "/v1/status", token, Value::Null).0,
            401
        );
        service.disable();
    }
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use crate::models::{book::NewBook, library_root::NewLibraryRoot};
    #[test]
    fn old_backups_rescans_and_numeric_id_reuse_keep_identity_safe() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("catalog.sqlite3");
        let db = Database::open(&path).unwrap();
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "test-only-root",
                display_name: "test",
            })
            .unwrap();
        let input = NewBook {
            library_root_id: root.id,
            file_path: "test-only-root/book.epub",
            parent_folder_path: "test-only-root",
            file_name: "book.epub",
            file_size: 10,
            modified_time: 1,
        };
        let row = db.upsert_scanned_book(input.clone()).unwrap();
        let first = db.public_id(row.book_id).unwrap();
        let mut changed = input.clone();
        changed.file_size = 20;
        changed.modified_time = 2;
        assert_eq!(
            db.upsert_scanned_book(changed).unwrap().book_id,
            row.book_id
        );
        assert_eq!(db.public_id(row.book_id).unwrap(), first);
        db.remove_library_root(root.id).unwrap();
        assert!(db.private_id(&first).unwrap().is_none());
        let root = db
            .add_library_root(NewLibraryRoot {
                path: "test-only-root",
                display_name: "test",
            })
            .unwrap();
        let mut again = input;
        again.library_root_id = root.id;
        let row = db.upsert_scanned_book(again).unwrap();
        assert_ne!(db.public_id(row.book_id).unwrap(), first);
        drop(db);
        // Only this generated catalog simulates a genuine pre-15 backup, not a user database.
        let old = rusqlite::Connection::open(&path).unwrap();
        crate::db::user_sync::remove_test_schema(&old);
        old.execute_batch("DROP TRIGGER companion_book_insert; DROP TABLE companion_books; DELETE FROM app_settings WHERE key='companion_catalog_id'; PRAGMA user_version=14;").unwrap();
        drop(old);
        let legacy = tmp.path().join("legacy.sqlite3");
        std::fs::copy(&path, &legacy).unwrap();
        let db = Database::open(&path).unwrap();
        assert_eq!(db.public_id(row.book_id).unwrap().len(), 32);
        db.restore_from(&legacy).unwrap();
        assert_eq!(db.public_id(row.book_id).unwrap().len(), 32);
        assert!(db.setting("companion_catalog_id").unwrap().is_some());
    }
    #[test]
    fn malformed_stored_hash_never_authorizes_a_token() {
        let mut auth = Auth::default();
        auth.devices.push(Device {
            id: "test".into(),
            name: "test".into(),
            token_hash: "".into(),
        });
        assert!(!auth.authorized(&"a".repeat(64)));
        auth.devices[0].token_hash = hash("not a credential".as_bytes());
        assert!(!auth.authorized(&"a".repeat(64)));
    }
}
