use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

static LOG: OnceLock<Mutex<std::fs::File>> = OnceLock::new();

pub fn initialize(directory: &Path) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("tmw-library.jsonl"))?;
    let _ = LOG.set(Mutex::new(file));
    Ok(())
}

pub fn event(level: &str, name: &str, fields: &[(&str, String)]) {
    let Some(log) = LOG.get() else {
        return;
    };
    let mut record = BTreeMap::new();
    record.insert(
        "timestamp",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    );
    record.insert("level", level.to_owned());
    record.insert("event", name.to_owned());
    for (key, value) in fields {
        record.insert(key, value.clone());
    }
    if let Ok(line) = serde_json::to_string(&record) {
        if let Ok(mut file) = log.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
}
