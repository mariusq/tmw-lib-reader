//! Offline, best-effort Japanese reading assistance. This module uses Lindera's
//! embedded IPADIC dictionary and never changes catalog display metadata.
use crate::services::performance::{self, Stage};
use lindera::{dictionary::load_dictionary, mode::Mode, segmenter::Segmenter};
use serde::Serialize;
use std::{borrow::Cow, sync::OnceLock};
use unicode_normalization::UnicodeNormalization;

static ANALYZER: OnceLock<Result<Segmenter, String>> = OnceLock::new();

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryTarget {
    pub surface: String,
    pub lemma: String,
    pub reading: Option<String>,
}

/// Finds the IPADIC token containing a character offset in a small EPUB text window.
/// This is assistive only: errors are returned so the UI can use its plain-text fallback.
pub fn dictionary_target(text: &str, offset: usize) -> Result<DictionaryTarget, String> {
    let analyzer = ANALYZER
        .get_or_init(|| {
            load_dictionary("embedded://ipadic")
                .map(|dictionary| Segmenter::new(Mode::Normal, dictionary, None))
                .map_err(|error| format!("Could not load bundled IPADIC dictionary: {error}"))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let mut start = 0;
    for mut token in analyzer
        .segment(Cow::Borrowed(text))
        .map_err(|e| e.to_string())?
    {
        let surface = token.surface.to_string();
        let end = start + surface.chars().count();
        if offset >= start && offset < end {
            let details = token.details();
            let lemma = details
                .get(6)
                .filter(|v| **v != "*")
                .map(|v| (*v).to_string())
                .unwrap_or_else(|| surface.clone());
            let reading = details
                .get(7)
                .filter(|v| **v != "*")
                .map(|v| (*v).to_string());
            return Ok(DictionaryTarget {
                surface,
                lemma,
                reading,
            });
        }
        start = end;
    }
    Err("No dictionary token was found at that position.".into())
}

pub fn normalize_romaji(value: &str) -> String {
    value
        .nfkc()
        .collect::<String>()
        .to_lowercase()
        .replace(['-', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Returns IPADIC readings for known words. Unknown tokens use their original
/// surface rather than a guessed reading, so indexing always stays non-fatal.
pub fn derive_reading(value: &str) -> Option<String> {
    performance::measure(Stage::ReadingDerivation, || derive_reading_inner(value))
}

fn derive_reading_inner(value: &str) -> Option<String> {
    if value.trim().is_empty() {
        return None;
    }
    let normalized = value.nfkc().collect::<String>();
    if is_kana_text_ignoring_separators(&normalized) {
        return Some(normalized);
    }
    let analyzer = ANALYZER
        .get_or_init(|| {
            load_dictionary("embedded://ipadic")
                .map(|dictionary| Segmenter::new(Mode::Normal, dictionary, None))
                .map_err(|error| format!("Could not load bundled IPADIC dictionary: {error}"))
        })
        .as_ref()
        .ok()?;
    let mut tokens = analyzer.segment(Cow::Borrowed(&normalized)).ok()?;
    let reading = tokens
        .iter_mut()
        .filter_map(|token| {
            let details = token.details();
            details
                .get(7)
                .filter(|reading| **reading != "*")
                .map(|reading| (*reading).to_string())
                .or_else(|| is_kana_text(token.surface.as_ref()).then(|| token.surface.to_string()))
        })
        .collect::<Vec<_>>()
        .join(" ");
    (!reading.is_empty()).then_some(reading)
}

pub fn kana_to_romaji(value: &str) -> String {
    let hira: String = value
        .nfkc()
        .collect::<String>()
        .chars()
        .map(to_hiragana)
        .collect();
    let pairs = [
        ("きゃ", "kya"),
        ("きゅ", "kyu"),
        ("きょ", "kyo"),
        ("しゃ", "sha"),
        ("しゅ", "shu"),
        ("しょ", "sho"),
        ("ちゃ", "cha"),
        ("ちゅ", "chu"),
        ("ちょ", "cho"),
        ("じゅ", "ju"),
        ("しん", "shin"),
    ];
    let mut result = String::new();
    let mut index = 0;
    while index < hira.len() {
        let rest = &hira[index..];
        if let Some((kana, roma)) = pairs.iter().find(|(k, _)| rest.starts_with(*k)) {
            result.push_str(roma);
            index += kana.len();
            continue;
        }
        let ch = rest.chars().next().unwrap();
        let roma = match ch {
            'あ' => "a",
            'い' => "i",
            'う' => "u",
            'え' => "e",
            'お' => "o",
            'か' => "ka",
            'き' => "ki",
            'く' => "ku",
            'け' => "ke",
            'こ' => "ko",
            'さ' => "sa",
            'し' => "shi",
            'す' => "su",
            'せ' => "se",
            'そ' => "so",
            'た' => "ta",
            'ち' => "chi",
            'つ' => "tsu",
            'て' => "te",
            'と' => "to",
            'な' => "na",
            'に' => "ni",
            'ぬ' => "nu",
            'ね' => "ne",
            'の' => "no",
            'は' => "ha",
            'ひ' => "hi",
            'ふ' => "fu",
            'へ' => "he",
            'ほ' => "ho",
            'ま' => "ma",
            'み' => "mi",
            'む' => "mu",
            'め' => "me",
            'も' => "mo",
            'や' => "ya",
            'ゆ' => "yu",
            'よ' => "yo",
            'ら' => "ra",
            'り' => "ri",
            'る' => "ru",
            'れ' => "re",
            'ろ' => "ro",
            'わ' => "wa",
            'を' => "o",
            'ん' => "n",
            'が' => "ga",
            'ぎ' => "gi",
            'ぐ' => "gu",
            'げ' => "ge",
            'ご' => "go",
            'ざ' => "za",
            'じ' => "ji",
            'ず' => "zu",
            'ぜ' => "ze",
            'ぞ' => "zo",
            'だ' => "da",
            'で' => "de",
            'ど' => "do",
            'ば' => "ba",
            'び' => "bi",
            'ぶ' => "bu",
            'べ' => "be",
            'ぼ' => "bo",
            'ぱ' => "pa",
            'ぴ' => "pi",
            'ぷ' => "pu",
            'ぺ' => "pe",
            'ぽ' => "po",
            'ー' => "-",
            _ => " ",
        };
        result.push_str(roma);
        index += ch.len_utf8();
    }
    normalize_romaji(&result.replace('-', ""))
}
pub fn reading_to_romaji(value: &str) -> Option<String> {
    derive_reading(value).map(|reading| kana_to_romaji(&reading))
}
fn is_kana_text(value: &str) -> bool {
    value
        .chars()
        .all(|c| matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}'))
}
fn is_kana_text_ignoring_separators(value: &str) -> bool {
    value.chars().all(|c| {
        c.is_whitespace() || matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}')
    }) && value.chars().any(|c| !c.is_whitespace())
}
fn to_hiragana(c: char) -> char {
    if matches!(c, '\u{30a1}'..='\u{30f6}') {
        char::from_u32(c as u32 - 0x60).unwrap_or(c)
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kana_and_kanji_are_transliterated_offline() {
        assert_eq!(
            reading_to_romaji("進撃の巨人").as_deref(),
            Some("shingeki no kyojin")
        );
        assert_eq!(reading_to_romaji("カタカナ").as_deref(), Some("katakana"));
    }
    #[test]
    fn input_normalization_is_consistent() {
        assert_eq!(
            normalize_romaji(" Ｓｈｉｎｇｅｋｉ-no　Ｋｙｏｊｉｎ "),
            "shingeki no kyojin"
        );
    }
}
