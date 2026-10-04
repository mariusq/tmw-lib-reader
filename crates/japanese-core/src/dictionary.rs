//! Offline importer for the locally supplied `jmdict-eng` JSON export. Its
//! source remains untouched; a compact, searchable copy lives in SQLite.
use serde::Deserialize;
use std::{collections::HashMap, fs::File, io::BufReader, path::Path};
use unicode_normalization::UnicodeNormalization;

pub fn normalize_query(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// Shared desktop/mobile result ordering. The caller supplies its enabled dictionary scope.
pub const LOOKUP_PREDICATE: &str = "(e.term_normalized=?1 OR e.reading_normalized=?1 OR e.term_normalized LIKE ?2) ORDER BY CASE WHEN e.term_normalized=?1 THEN 0 WHEN e.reading_normalized=?1 THEN 1 ELSE 2 END, length(e.term), e.id LIMIT 12";

#[derive(Debug, Clone)]
pub struct ImportedEntry {
    pub term: String,
    pub reading: Option<String>,
    pub definitions: Vec<String>,
    pub part_of_speech: Vec<String>,
}

#[derive(Deserialize)]
struct DictionaryFile {
    #[serde(default)]
    tags: HashMap<String, String>,
    words: Vec<Word>,
}
#[derive(Deserialize)]
struct Word {
    #[serde(default)]
    kanji: Vec<Form>,
    #[serde(default)]
    kana: Vec<Form>,
    #[serde(default)]
    sense: Vec<Sense>,
}
#[derive(Deserialize)]
struct Form {
    text: String,
}
#[derive(Deserialize)]
struct Sense {
    #[serde(rename = "partOfSpeech", default)]
    part_of_speech: Vec<String>,
    #[serde(default)]
    gloss: Vec<Gloss>,
}
#[derive(Deserialize)]
struct Gloss {
    lang: String,
    text: String,
}

/// Reads the jmdict-eng JSON schema used by `jmdict-eng-3.6.2.json`. JSON
/// import errors return before the old imported dictionary is replaced.
pub fn import_jmdict(path: &Path) -> Result<Vec<ImportedEntry>, String> {
    if path
        .extension()
        .and_then(|x| x.to_str())
        .map(|x| !x.eq_ignore_ascii_case("json"))
        .unwrap_or(true)
    {
        return Err("Select the jmdict-eng JSON file (for example jmdict-eng-3.6.2.json).".into());
    }
    let file = File::open(path).map_err(|e| format!("Could not open JMdict JSON: {e}"))?;
    let source: DictionaryFile = serde_json::from_reader(BufReader::new(file))
        .map_err(|e| format!("Invalid jmdict-eng JSON: {e}"))?;
    let DictionaryFile { tags, words } = source;
    let mut output = Vec::new();
    for word in words {
        let reading = word.kana.first().map(|form| form.text.clone());
        let definitions = word
            .sense
            .iter()
            .flat_map(|sense| sense.gloss.iter())
            .filter(|gloss| gloss.lang == "eng")
            .map(|gloss| gloss.text.clone())
            .collect::<Vec<_>>();
        if definitions.is_empty() {
            continue;
        }
        let part_of_speech = word
            .sense
            .iter()
            .flat_map(|sense| sense.part_of_speech.iter())
            .map(|tag| tags.get(tag).cloned().unwrap_or_else(|| tag.clone()))
            .collect::<Vec<_>>();
        let mut forms = word
            .kanji
            .into_iter()
            .map(|form| form.text)
            .collect::<Vec<_>>();
        forms.extend(word.kana.into_iter().map(|form| form.text));
        forms.sort();
        forms.dedup();
        for term in forms {
            output.push(ImportedEntry {
                reading: if term == reading.clone().unwrap_or_default() {
                    Some(term.clone())
                } else {
                    reading.clone()
                },
                term,
                definitions: definitions.clone(),
                part_of_speech: part_of_speech.clone(),
            });
        }
    }
    if output.is_empty() {
        return Err("No English JMdict entries were found in this JSON file.".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn imports_bundled_json_shape_and_expands_tags() {
        let file = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        let mut out = file.reopen().unwrap();
        write!(out,r#"{{"tags":{{"n":"noun"}},"words":[{{"kanji":[{{"text":"猫"}}],"kana":[{{"text":"ねこ"}}],"sense":[{{"partOfSpeech":["n"],"gloss":[{{"lang":"eng","text":"cat"}}]}}]}}]}}"#).unwrap();
        let entries = import_jmdict(file.path()).unwrap();
        assert_eq!(entries[0].term, "ねこ");
        assert_eq!(entries[1].reading.as_deref(), Some("ねこ"));
        assert_eq!(entries[1].definitions, ["cat"]);
        assert_eq!(entries[1].part_of_speech, ["noun"]);
    }
}
