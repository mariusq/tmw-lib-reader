//! Independently implemented format-3 ZIP importer. No upstream code/rules reused.
//! JSON banks stream one row at a time; the sink owns atomic publication.
use crate::dictionary::{DictionaryImportSink, DictionaryManifest, TagRecord, TermRecord};
use serde::de::{DeserializeSeed, Error, SeqAccess, Visitor};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashSet},
    fmt,
    io::{BufReader, Cursor, Read, Seek},
    sync::atomic::{AtomicBool, Ordering},
};

pub const MAX_ARCHIVE_BYTES: u64 = 512_000_000;
const MAX_EXPANDED: u64 = 2_000_000_000;
const MAX_BANK: u64 = 64_000_000;
const MAX_ASSET: u64 = 16_000_000;
const MAX_ROWS: u64 = 5_000_000;
const MAX_ENTRIES: usize = 100_000;
const BATCH: usize = 128;

#[derive(Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub phase: String,
    pub files_done: usize,
    pub files_total: usize,
    pub terms: u64,
    pub tags: u64,
    pub metadata: u64,
}
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub manifest: DictionaryManifest,
    pub terms: u64,
    pub tags: u64,
    pub metadata: u64,
    pub assets: usize,
    pub warnings: Vec<String>,
}
struct CheckedRead<'a, R> {
    reader: R,
    remaining: u64,
    cancel: &'a AtomicBool,
}
impl<R: Read> Read for CheckedRead<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("Dictionary import canceled"));
        }
        let limit = bytes
            .len()
            .min((self.remaining.saturating_add(1)).min(usize::MAX as u64) as usize);
        let size = self.reader.read(&mut bytes[..limit])?;
        if size as u64 > self.remaining {
            return Err(std::io::Error::other("ZIP entry exceeds its declared size"));
        }
        self.remaining -= size as u64;
        if size == 0 && self.remaining != 0 {
            return Err(std::io::Error::other("ZIP entry is truncated"));
        }
        Ok(size)
    }
}
pub fn canceled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Import canceled; existing dictionaries retained.".into())
    } else {
        Ok(())
    }
}

/// Portable path validation also rejects Windows drive/ADS/reserved-name aliases.
pub fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.ends_with(['.', ' '])
                && !part.chars().any(char::is_control)
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(&stem.as_str())
        })
}
fn bank(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(".json"))
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn image_path(name: &str) -> bool {
    [".png", ".jpg", ".jpeg", ".webp"]
        .iter()
        .any(|ext| name.to_ascii_lowercase().ends_with(ext))
}

/// Sanitized data has no CSS, event handlers, executable elements, or external URLs.
/// Original JSON is stored separately for later matching/rendering compatibility.
pub fn safe_glossary(value: &Value) -> Value {
    match value {
        Value::String(_) => value.clone(),
        Value::Array(values) => Value::Array(values.iter().map(safe_glossary).collect()),
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) == Some("text") {
                return serde_json::json!({"type":"text", "text":obj.get("text").and_then(Value::as_str).unwrap_or("")});
            }
            if obj.get("type").and_then(Value::as_str) == Some("structured-content") {
                return serde_json::json!({"type":"structured-content", "content":safe_glossary(&obj["content"])});
            }
            let tag = obj.get("tag").and_then(Value::as_str).or_else(|| {
                (obj.get("type").and_then(Value::as_str) == Some("image")).then_some("img")
            });
            if !tag.is_some_and(|s| {
                [
                    "br", "ruby", "rt", "rp", "table", "thead", "tbody", "tfoot", "tr", "td", "th",
                    "span", "div", "ol", "ul", "li", "details", "summary", "img", "a",
                ]
                .contains(&s)
            }) {
                return serde_json::json!({"unsupported": "Unsupported definition element", "content":obj.get("content").map(safe_glossary)});
            }
            let mut result = serde_json::Map::new();
            result.insert("tag".into(), Value::String(tag.unwrap().into()));
            for key in ["content", "title", "alt", "description", "lang"] {
                if let Some(v) = obj.get(key) {
                    if key == "content" {
                        result.insert(key.into(), safe_glossary(v));
                    } else if v.is_string() {
                        result.insert(key.into(), v.clone());
                    }
                }
            }
            if tag == Some("img") {
                if let Some(path) = obj
                    .get("path")
                    .and_then(Value::as_str)
                    .filter(|s| safe_path(s) && image_path(s))
                {
                    result.insert("path".into(), Value::String(path.into()));
                } else {
                    result.insert(
                        "unsupported".into(),
                        Value::String("Unsupported image path".into()),
                    );
                }
            }
            // Dictionary links remain plain containers. No remote loading/navigation.
            Value::Object(result)
        }
        _ => serde_json::json!({"unsupported":"Unsupported definition value"}),
    }
}

fn glossary_check(
    value: &Value,
    depth: usize,
    assets: &mut BTreeSet<String>,
    warnings: &mut BTreeSet<String>,
) -> Result<(), String> {
    if depth > 32 {
        return Err("Definition nesting exceeds 32 levels.".into());
    }
    match value {
        Value::String(s) if s.len() > 1_000_000 => {
            return Err("Definition text exceeds 1 MB.".into())
        }
        Value::Array(items) => {
            for item in items {
                glossary_check(item, depth + 1, assets, warnings)?;
            }
        }
        Value::Object(obj) => {
            if let Some(path) = obj.get("path").filter(|_| {
                obj.get("tag").and_then(Value::as_str) == Some("img")
                    || obj.get("type").and_then(Value::as_str) == Some("image")
            }) {
                let path = path.as_str().ok_or("Invalid definition image path")?;
                if !safe_path(path) || !image_path(path) {
                    return Err("Definition requires an unsafe or unsupported image path (PNG/JPEG/WebP only).".into());
                }
                assets.insert(path.into());
                if assets.len() > MAX_ENTRIES {
                    return Err("Too many distinct image references.".into());
                }
            }
            if obj.contains_key("style") || obj.contains_key("href") || obj.contains_key("data") {
                warnings.insert("Definition styles, links and data attributes are retained as raw data but disabled in safe content.".into());
            }
            if safe_glossary(value).get("unsupported").is_some() {
                warnings.insert("Unsupported definition elements retained as raw data with visible placeholders in safe content.".into());
            }
            for v in obj.values() {
                glossary_check(v, depth + 1, assets, warnings)?;
            }
        }
        _ => (),
    }
    Ok(())
}

struct Rows<'a, S> {
    sink: &'a mut S,
    term: bool,
    metadata: bool,
    cancel: &'a AtomicBool,
    progress: &'a mut ImportProgress,
    notify: &'a mut dyn FnMut(&ImportProgress),
    assets: &'a mut BTreeSet<String>,
    warnings: &'a mut BTreeSet<String>,
}
impl<'de, S: DictionaryImportSink> DeserializeSeed<'de> for Rows<'_, S> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}
impl<'de, S: DictionaryImportSink> Visitor<'de> for Rows<'_, S> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a format-3 bank array")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut rows: A) -> Result<(), A::Error> {
        let mut terms = Vec::with_capacity(BATCH);
        let mut tags = Vec::with_capacity(BATCH);
        let mut metadata = Vec::with_capacity(BATCH);
        let mut batch_bytes = 0;
        // RawValue prevents a malicious large row from allocating millions of JSON
        // nodes before its byte limit is checked. Its raw buffer is bank-bounded.
        while let Some(raw) = rows.next_element::<Box<serde_json::value::RawValue>>()? {
            canceled(self.cancel).map_err(A::Error::custom)?;
            let row_bytes = raw.get().len();
            if row_bytes > 1_000_000 {
                return Err(A::Error::custom("Bank row exceeds 1 MB."));
            }
            batch_bytes += row_bytes;
            let value: Value = serde_json::from_str(raw.get()).map_err(A::Error::custom)?;
            if self.progress.terms + self.progress.tags + self.progress.metadata >= MAX_ROWS {
                return Err(A::Error::custom("Dictionary exceeds 5 million rows."));
            }
            if self.metadata {
                let (term, mode, data): (String, String, Value) =
                    serde_json::from_value(value).map_err(A::Error::custom)?;
                if term.len() > 4096 {
                    return Err(A::Error::custom("Metadata term exceeds 4096 bytes"));
                }
                if matches!(mode.as_str(), "freq" | "pitch") {
                    validate_metadata(&mode, &data).map_err(A::Error::custom)?;
                    metadata.push((term, mode, data));
                } else {
                    self.warnings
                        .insert(format!("Unsupported term metadata mode: {mode}"));
                }
                self.progress.metadata += 1;
            } else if self.term {
                let (term, reading, definition_tags, rules, score, glossary, sequence, term_tags):
                    (String, String, Option<String>, String, f64, Vec<Value>, i64, String) = serde_json::from_value(value).map_err(A::Error::custom)?;
                if term.len() > 4096 || reading.len() > 4096 {
                    return Err(A::Error::custom("Term/reading exceeds 4096 bytes."));
                }
                for definition in &glossary {
                    if let Value::Array(deinflection) = definition {
                        if deinflection.len() != 2
                            || !deinflection[0].is_string()
                            || !deinflection[1]
                                .as_array()
                                .is_some_and(|a| a.iter().all(Value::is_string))
                        {
                            return Err(A::Error::custom(
                                "Invalid dictionary-provided deinflection record.",
                            ));
                        }
                    } else if !definition.is_string() && !definition.is_object() {
                        return Err(A::Error::custom("Invalid glossary value."));
                    }
                    glossary_check(definition, 0, self.assets, self.warnings)
                        .map_err(A::Error::custom)?;
                }
                terms.push(TermRecord {
                    term,
                    reading,
                    definition_tags: split(&definition_tags.unwrap_or_default()),
                    rules: split(&rules),
                    score,
                    glossary,
                    sequence,
                    term_tags: split(&term_tags),
                });
                self.progress.terms += 1;
            } else {
                let (name, category, order, notes, score): (String, String, f64, String, f64) =
                    serde_json::from_value(value).map_err(A::Error::custom)?;
                tags.push(TagRecord {
                    name,
                    category,
                    order,
                    notes,
                    score,
                });
                self.progress.tags += 1;
            }
            if terms.len() + tags.len() + metadata.len() >= BATCH || batch_bytes >= 4_000_000 {
                self.sink.write_terms(&terms).map_err(A::Error::custom)?;
                self.sink.write_tags(&tags).map_err(A::Error::custom)?;
                self.sink
                    .write_metadata(&metadata)
                    .map_err(A::Error::custom)?;
                metadata.clear();
                terms.clear();
                tags.clear();
                batch_bytes = 0;
                (self.notify)(self.progress);
            }
        }
        self.sink
            .write_metadata(&metadata)
            .map_err(A::Error::custom)?;
        self.sink.write_terms(&terms).map_err(A::Error::custom)?;
        self.sink.write_tags(&tags).map_err(A::Error::custom)?;
        Ok(())
    }
}
fn split(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

pub fn import<R: Read + Seek, S: DictionaryImportSink>(
    mut source: R,
    create: impl FnOnce(&DictionaryManifest) -> Result<S, String>,
    cancel: &AtomicBool,
    mut notify: impl FnMut(&ImportProgress),
) -> Result<ImportReport, String> {
    let size = source
        .seek(std::io::SeekFrom::End(0))
        .map_err(|e| e.to_string())?;
    if size > MAX_ARCHIVE_BYTES {
        return Err("Dictionary ZIP exceeds 512 MB.".into());
    }
    source
        .seek(std::io::SeekFrom::Start(0))
        .map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(source).map_err(|e| format!("Invalid dictionary ZIP: {e}"))?;
    if archive.len() > MAX_ENTRIES {
        return Err("ZIP exceeds 100,000 files.".into());
    }
    let mut names = HashSet::new();
    let mut total = 0u64;
    let mut banks = Vec::new();
    let mut images = Vec::new();
    let mut warnings = BTreeSet::new();
    for i in 0..archive.len() {
        canceled(cancel)?;
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_owned();
        let checked = if file.is_dir() {
            name.trim_end_matches('/')
        } else {
            &name
        };
        if !safe_path(checked)
            || !names.insert(name.clone())
            || file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000)
        {
            return Err("ZIP contains duplicate, unsafe, or symbolic-link paths.".into());
        }
        total = total
            .checked_add(file.size())
            .ok_or("ZIP expansion overflow")?;
        if total > MAX_EXPANDED || file.size() > MAX_BANK {
            return Err("ZIP expansion exceeds 2 GB total or 64 MB per bank/file.".into());
        }
        if bank(&name, "term_bank_") || bank(&name, "tag_bank_") || bank(&name, "term_meta_bank_") {
            banks.push((i, name));
        } else if image_path(&name) {
            if file.size() > MAX_ASSET {
                return Err("Image exceeds 16 MB.".into());
            }
            images.push((i, name));
        } else if !file.is_dir() && name != "index.json" {
            warnings
                .insert("Other banks/files (including kanji and audio) are unsupported.".into());
        }
    }
    let index = archive
        .by_name("index.json")
        .map_err(|_| "ZIP requires index.json at its root.")?;
    if index.size() > 1_000_000 {
        return Err("index.json exceeds 1 MB.".into());
    }
    let declared = index.size();
    let index: Value = serde_json::from_reader(BufReader::new(CheckedRead {
        reader: index,
        remaining: declared,
        cancel,
    }))
    .map_err(|e| format!("Invalid index.json: {e}"))?;
    let format = index
        .get("format")
        .or_else(|| index.get("version"))
        .and_then(Value::as_u64);
    if format != Some(3) {
        return Err("Only Yomitan format 3 term/tag dictionaries are supported.".into());
    }
    if index.get("format").is_some()
        && index.get("version").is_some()
        && index["format"] != index["version"]
    {
        return Err("Conflicting index format/version.".into());
    }
    let field = |key: &str| -> Result<String, String> {
        index
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty() && s.len() <= 4096)
            .map(str::to_owned)
            .ok_or_else(|| format!("index.json requires a nonempty {key} (up to 4096 bytes)."))
    };
    let manifest = DictionaryManifest {
        title: field("title")?,
        revision: field("revision")?,
        format: 3,
        sequenced: match index.get("sequenced") {
            None => false,
            Some(v) => v.as_bool().ok_or("Invalid sequenced flag")?,
        },
        attribution: match index.get("attribution") {
            None => None,
            Some(v) => Some(v.as_str().ok_or("Invalid attribution")?.to_owned()),
        },
        metadata: index.clone(),
    };
    if index.get("tagMeta").is_some() {
        warnings.insert("Legacy index tagMeta is retained in neither tags nor definitions; use tag-bank format 3.".into());
    }
    let mut sink = create(&manifest)?;
    let mut progress = ImportProgress {
        phase: "Importing term/tag banks".into(),
        files_total: banks.len() + images.len(),
        ..Default::default()
    };
    notify(&progress);
    let mut references = BTreeSet::new();
    for (i, name) in banks {
        canceled(cancel)?;
        let file = archive.by_index(i).map_err(|e| e.to_string())?;
        let declared = file.size();
        let mut parser = serde_json::Deserializer::from_reader(BufReader::new(CheckedRead {
            reader: file,
            remaining: declared,
            cancel,
        }));
        Rows {
            sink: &mut sink,
            term: bank(&name, "term_bank_"),
            metadata: bank(&name, "term_meta_bank_"),
            cancel,
            progress: &mut progress,
            notify: &mut notify,
            assets: &mut references,
            warnings: &mut warnings,
        }
        .deserialize(&mut parser)
        .map_err(|e| format!("Invalid {name}: {e}"))?;
        parser.end().map_err(|e| format!("Invalid {name}: {e}"))?;
        progress.files_done += 1;
        notify(&progress);
    }
    if progress.terms == 0 && progress.metadata == 0 {
        return Err("No supported term or metadata entries found.".into());
    }
    for path in &references {
        if !names.contains(path) {
            return Err(format!("Definition references missing local image: {path}"));
        }
    }
    progress.phase = "Importing local images".into();
    for (i, name) in &images {
        canceled(cancel)?;
        let file = archive.by_index(*i).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        let declared = file.size();
        CheckedRead {
            reader: file,
            remaining: declared,
            cancel,
        }
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Invalid image {name}: {e}"))?;
        if bytes.len() as u64 > MAX_ASSET {
            return Err("Image exceeds 16 MB.".into());
        }
        let reader = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        if !reader.format().is_some_and(|f| {
            [
                image::ImageFormat::Png,
                image::ImageFormat::Jpeg,
                image::ImageFormat::WebP,
            ]
            .contains(&f)
        }) {
            return Err(format!("Unsupported image encoding: {name}"));
        }
        let (w, h) = reader
            .into_dimensions()
            .map_err(|e| format!("Invalid image {name}: {e}"))?;
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 32_000_000 {
            return Err("Image dimensions exceed 32 million pixels.".into());
        }
        sink.stage_asset(name, &mut Cursor::new(bytes))?;
        progress.files_done += 1;
        notify(&progress);
    }
    canceled(cancel)?;
    progress.phase = "Publishing".into();
    notify(&progress);
    sink.write_warnings(&warnings.iter().cloned().collect::<Vec<_>>())?;
    canceled(cancel)?;
    sink.publish()?;
    Ok(ImportReport {
        manifest,
        terms: progress.terms,
        tags: progress.tags,
        metadata: progress.metadata,
        assets: images.len(),
        warnings: warnings.into_iter().collect(),
    })
}

fn validate_metadata(mode: &str, data: &Value) -> Result<(), String> {
    fn frequency(v: &Value) -> bool {
        v.is_number()
            || v.as_str().is_some_and(|s| s.len() <= 4096)
            || v.as_object().is_some_and(|o| {
                o.get("value").is_some_and(Value::is_number)
                    && o.get("displayValue")
                        .is_none_or(|x| x.as_str().is_some_and(|s| s.len() <= 4096))
                    && o.keys()
                        .all(|k| matches!(k.as_str(), "value" | "displayValue"))
            })
    }
    let valid = if mode == "freq" {
        frequency(data)
            || data.as_object().is_some_and(|o| {
                o.get("reading")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.len() <= 4096)
                    && o.get("frequency").is_some_and(frequency)
                    && o.keys()
                        .all(|k| matches!(k.as_str(), "reading" | "frequency"))
            })
    } else {
        data.get("reading")
            .and_then(Value::as_str)
            .is_some_and(|s| s.len() <= 4096)
            && data
                .get("pitches")
                .and_then(Value::as_array)
                .is_some_and(|a| {
                    a.len() <= 64
                        && a.iter().all(|p| {
                            p.get("position").is_some_and(|v| {
                                v.as_u64().is_some_and(|n| n <= 256)
                                    || v.as_str().is_some_and(|s| {
                                        !s.is_empty()
                                            && s.len() <= 256
                                            && s.bytes().all(|b| b == b'H' || b == b'L')
                                    })
                            })
                        })
                })
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Unsupported or invalid {mode} metadata shape"))
    }
}
