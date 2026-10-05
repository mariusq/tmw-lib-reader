//! Behavior-preserving Phase 1 engine. Storage and reader anchors stay platform-owned.
use crate::readings::{dictionary_target_with_span, DictionaryTarget};
use serde::{Deserialize, Serialize};

pub const LOOKUP_CONTRACT_VERSION: u32 = 1;
pub const MAX_TEXT_CHARACTERS: usize = 16_000;

/// Offset uses Unicode scalar values, not UTF-8 bytes or JavaScript UTF-16 units.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupRequest {
    pub text: String,
    pub offset: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedSpan {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    /// Ephemeral storage ID. Never use as portable passage/history identity.
    pub id: i64,
    pub term: String,
    pub reading: Option<String>,
    pub definitions: Vec<String>,
    pub part_of_speech: Vec<String>,
    pub dictionary_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResponse {
    pub contract_version: u32,
    pub target: DictionaryTarget,
    pub matched_span: MatchedSpan,
    pub entries: Vec<DictionaryEntry>,
}

/// Query adapter supplies enabled dictionary scope and deterministic, <=12 rows
/// using the existing exact-term / exact-reading / prefix ordering. No SQL in UI.
/// Errors must propagate; a storage failure is not an empty successful lookup.
pub trait DictionaryStore {
    fn query(&mut self, value: &str) -> Result<Vec<DictionaryEntry>, String>;
}

impl<F> DictionaryStore for F
where
    F: FnMut(&str) -> Result<Vec<DictionaryEntry>, String>,
{
    fn query(&mut self, value: &str) -> Result<Vec<DictionaryEntry>, String> {
        self(value)
    }
}

pub fn lookup_text(
    request: &LookupRequest,
    store: &mut impl DictionaryStore,
) -> Result<LookupResponse, String> {
    // Stop counting at the bound instead of traversing arbitrarily large input.
    let count = request.text.chars().take(MAX_TEXT_CHARACTERS + 1).count();
    if count > MAX_TEXT_CHARACTERS {
        return Err("Text window exceeds 16000 characters".into());
    }
    if request.offset >= count {
        return Err("Dictionary offset is outside the text window".into());
    }
    let (target, start, end) = dictionary_target_with_span(&request.text, request.offset)?;
    let entries = lookup_target(&target, store)?;
    Ok(LookupResponse {
        contract_version: LOOKUP_CONTRACT_VERSION,
        target,
        matched_span: MatchedSpan { start, end },
        entries,
    })
}

fn lookup_target(
    target: &DictionaryTarget,
    store: &mut impl DictionaryStore,
) -> Result<Vec<DictionaryEntry>, String> {
    let mut entries = store.query(&target.surface)?;
    if entries.is_empty() && target.lemma != target.surface {
        entries = store.query(&target.lemma)?;
    }
    if entries.is_empty() {
        if let Some(reading) = &target.reading {
            entries = store.query(reading)?;
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str) -> DictionaryEntry {
        DictionaryEntry {
            id: 1,
            term: term.into(),
            reading: None,
            definitions: vec!["fixture".into()],
            part_of_speech: vec![],
            dictionary_name: "Generated fixture".into(),
        }
    }

    #[test]
    fn preserves_fallback_order_and_stops_on_first_results() {
        let target = DictionaryTarget {
            surface: "食べ".into(),
            lemma: "食べる".into(),
            reading: Some("タベ".into()),
        };
        let mut calls = Vec::new();
        let rows = lookup_target(&target, &mut |value: &str| {
            calls.push(value.to_owned());
            Ok(if value == "食べる" {
                vec![entry(value)]
            } else {
                vec![]
            })
        })
        .unwrap();
        assert_eq!(calls, ["食べ", "食べる"]);
        assert_eq!(rows[0].term, "食べる");
        calls.clear();
        lookup_target(&target, &mut |value: &str| {
            calls.push(value.to_owned());
            Ok(vec![])
        })
        .unwrap();
        assert_eq!(calls, ["食べ", "食べる", "タベ"]);
        assert!(lookup_target(&target, &mut |_: &str| Err("storage failed".into())).is_err());
    }

    #[test]
    fn rejects_invalid_requests_before_querying() {
        for request in [
            LookupRequest {
                text: "".into(),
                offset: 0,
            },
            LookupRequest {
                text: "猫".into(),
                offset: 1,
            },
            LookupRequest {
                text: "猫".repeat(MAX_TEXT_CHARACTERS + 1),
                offset: 0,
            },
        ] {
            assert!(lookup_text(&request, &mut |_: &str| panic!("must not query")).is_err());
        }
    }

    #[test]
    fn original_span_handles_non_bmp_and_repeated_words() {
        let request = LookupRequest {
            text: "😀 猫と猫。".into(),
            offset: 4,
        };
        let result = lookup_text(&request, &mut |_: &str| Ok(vec![])).unwrap();
        assert_eq!(result.target.surface, "猫");
        assert_eq!((result.matched_span.start, result.matched_span.end), (4, 5));
        assert_eq!(result.contract_version, 1);
    }
}
