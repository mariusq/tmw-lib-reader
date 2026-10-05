use crate::{chunk_lookup::*, dictionary::normalize_query, lookup::LookupRequest};
use serde::Deserialize;

#[derive(Default)]
struct MemoryStore {
    entries: Vec<Entry>,
    batches: Vec<Vec<String>>,
    error: Option<String>,
}
impl Store for MemoryStore {
    fn query_batch(&mut self, keys: &[String]) -> Result<Vec<Entry>, String> {
        self.batches.push(keys.to_vec());
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(self
            .entries
            .iter()
            .filter(|e| {
                keys.contains(&normalize_query(&e.term))
                    || keys.contains(&normalize_query(&e.reading))
            })
            .cloned()
            .collect())
    }
}
fn entry(id: i64, term: &str, reading: &str, rule: &str) -> Entry {
    Entry {
        assets: Default::default(),
        metadata: vec![],
        id,
        term: term.into(),
        reading: reading.into(),
        provenance: Provenance {
            source: "fixture".into(),
            title: "Synthetic dictionary".into(),
            revision: "1".into(),
        },
        rules: vec![rule.into()],
        glossary: serde_json::json!(["synthetic definition"]),
        definition_tags: vec![],
        term_tags: vec![],
        sequence: id,
        score: 0.0,
        priority: 0,
    }
}
fn request(text: &str, offset: usize) -> LookupRequest {
    LookupRequest {
        text: text.into(),
        offset,
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    id: String,
    text: String,
    offset: usize,
    expected_term: String,
    expected_readings: Vec<String>,
    expected_span: [usize; 2],
}
#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}

#[test]
fn comparison_corpus_recovers_terms_readings_and_original_anchors() {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../../../docs/dictionary-comparison-corpus.json"
    ))
    .unwrap();
    let mut fixture = Vec::new();
    for case in &corpus.cases {
        let rule = match case.expected_term.as_str() {
            "読む" => "v5m",
            "食べる" => "v1",
            "高い" => "adj-i",
            _ => "n",
        };
        for reading in &case.expected_readings {
            if !fixture
                .iter()
                .any(|e: &Entry| e.term == case.expected_term && e.reading == *reading)
            {
                fixture.push(entry(
                    fixture.len() as i64,
                    &case.expected_term,
                    reading,
                    rule,
                ));
            }
        }
    }
    fixture.push(entry(100, "学校", "がっこう", "n"));
    fixture.push(entry(101, "生", "なま", "n"));
    for case in corpus.cases {
        let mut store = MemoryStore {
            entries: fixture.clone(),
            ..Default::default()
        };
        let response = lookup(&request(&case.text, case.offset), &mut store).unwrap();
        assert_eq!(
            [response.matched_span.start, response.matched_span.end],
            case.expected_span,
            "{}",
            case.id
        );
        assert_eq!(response.target.lemma, case.expected_term, "{}", case.id);
        let mut readings: Vec<_> = response.groups.iter().map(|g| g.reading.clone()).collect();
        readings.sort();
        readings.dedup();
        let mut expected = case.expected_readings;
        expected.sort();
        assert_eq!(readings, expected, "{}", case.id);
        assert_eq!(store.batches.len(), 1);
        let keys = &store.batches[0];
        assert!(
            keys.windows(2).all(|w| w[0] < w[1]),
            "unique sorted exact keys"
        );
    }
}

#[test]
fn auxiliary_chains_and_contractions_keep_the_entire_inflected_span() {
    for (text, term, rule, min_depth) in [
        ("食べている", "食べる", "v1", 2),
        ("食べさせられました", "食べる", "v1", 3),
        ("読んじゃった", "読む", "v5m", 2),
        ("高くなかった", "高い", "adj-i", 2),
    ] {
        let mut store = MemoryStore {
            entries: vec![entry(1, term, "", rule)],
            ..Default::default()
        };
        let response = lookup(&request(text, 0), &mut store).unwrap();
        assert_eq!(response.target.lemma, term, "{text}");
        assert_eq!(response.matched_span.end, text.chars().count(), "{text}");
        assert!(
            response.groups[0].matches[0].deinflection_depth >= min_depth,
            "{text}"
        );
    }
}

#[test]
fn normalization_preserves_scalar_offsets_and_surface() {
    let mut store = MemoryStore {
        entries: vec![entry(1, "ガク", "がく", "n")],
        ..Default::default()
    };
    let response = lookup(&request("😀 ｶﾞｸ。", 3), &mut store).unwrap();
    assert_eq!(
        (response.matched_span.start, response.matched_span.end),
        (2, 5)
    );
    assert_eq!(response.target.surface, "ｶﾞｸ");
    assert_eq!(response.target.lemma, "ガク");
}

#[test]
fn composed_import_and_fallback_stores_keep_namespace_and_errors() {
    let mut imported = MemoryStore {
        entries: vec![entry(1, "猫", "ねこ", "n")],
        ..Default::default()
    };
    imported.entries[0].provenance.source = "yomitan:local".into();
    let mut fallback = MemoryStore {
        entries: vec![entry(1, "猫", "ねこ", "n")],
        ..Default::default()
    };
    fallback.entries[0].provenance.source = "builtin:JMdict".into();
    let result = lookup(&request("猫", 0), &mut (&mut imported, &mut fallback)).unwrap();
    assert_eq!(result.groups[0].matches.len(), 2);
    fallback.error = Some("fallback unavailable".into());
    assert_eq!(
        lookup(&request("猫", 0), &mut (&mut imported, &mut fallback)).unwrap_err(),
        "fallback unavailable"
    );
}

#[test]
fn inflected_hits_require_compatible_pos_and_never_accept_prefix_rows() {
    let mut store = MemoryStore {
        entries: vec![entry(1, "読む", "よむ", "n"), entry(2, "読", "どく", "n")],
        ..Default::default()
    };
    let response = lookup(&request("読んだ", 0), &mut store).unwrap();
    assert!(response.groups.iter().all(|g| g.term != "読む"));
    let mut store = MemoryStore {
        entries: vec![entry(1, "学校生活", "がっこうせいかつ", "n")],
        ..Default::default()
    };
    assert!(lookup(&request("学校", 0), &mut store)
        .unwrap()
        .groups
        .is_empty());
    let mut store = MemoryStore {
        entries: vec![
            entry(1, "書く", "かく", "v5g"),
            entry(2, "書ぐ", "かぐ", "v5g"),
        ],
        ..Default::default()
    };
    let response = lookup(&request("書いた", 0), &mut store).unwrap();
    // Pinned Yomitan uses the broad v5 dictionary class; suffix spelling still
    // distinguishes 書く from 書ぐ. The deliberately mislabeled v5g row is accepted.
    assert_eq!(response.target.lemma, "書く");
    assert!(response.groups.iter().all(|g| g.term != "書ぐ"));
}

#[test]
fn dictionary_chunks_do_not_require_tokenizer_boundaries_and_punctuation_does_not_query() {
    let mut store = MemoryStore {
        entries: vec![entry(1, "猫雲星", "ねこうんせい", "n")],
        ..Default::default()
    };
    let response = lookup(&request("猫雲星。", 1), &mut store).unwrap();
    assert_eq!(response.target.lemma, "猫雲星");
    assert_eq!(
        (response.matched_span.start, response.matched_span.end),
        (0, 3)
    );
    store.batches.clear();
    let response = lookup(&request("猫雲星。", 3), &mut store).unwrap();
    assert!(response.groups.is_empty());
    assert!(store.batches.is_empty());
}

#[test]
fn grouping_priority_provenance_and_limits_are_deterministic() {
    let mut entries: Vec<_> = (0..20)
        .map(|i| {
            let mut e = entry(i, "猫", "ねこ", "n");
            e.provenance.source = format!("source-{i:02}");
            e.priority = i;
            e
        })
        .collect();
    let mut low = entry(99, "猫", "びょう", "n");
    low.priority = -1;
    entries.push(low);
    let mut store = MemoryStore {
        entries: entries.clone(),
        ..Default::default()
    };
    let first = lookup(&request("猫", 0), &mut store).unwrap();
    entries.reverse();
    store.entries = entries;
    let second = lookup(&request("猫", 0), &mut store).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
    assert_eq!(first.groups.len(), 1);
    assert_eq!(first.groups[0].matches.len(), MAX_RESULTS);
    assert_eq!(
        first.groups[0].matches[0].entry.provenance.source,
        "source-19"
    );
    assert_eq!(
        first.groups[0].matches[0].entry.provenance.title,
        "Synthetic dictionary"
    );
}

#[test]
fn failures_and_pathological_input_obey_bounded_contracts() {
    let mut store = MemoryStore {
        error: Some("storage unavailable".into()),
        ..Default::default()
    };
    assert_eq!(
        lookup(&request("猫", 0), &mut store).unwrap_err(),
        "storage unavailable"
    );
    store.batches.clear();
    for invalid in [
        request("", 0),
        request("猫", 1),
        request(&"猫".repeat(16_001), 0),
    ] {
        assert!(lookup(&invalid, &mut store).is_err());
    }
    assert!(store.batches.is_empty());
    let values = candidates(&request(&"なかった".repeat(100), 200)).unwrap();
    assert!(values.len() <= MAX_CANDIDATES);
    assert!(values.iter().all(|c| c.depth <= MAX_DEPTH
        && c.end - c.start <= MAX_TERM
        && c.start <= 200
        && c.end > 200));
    struct Overflow;
    impl Store for Overflow {
        fn query_batch(&mut self, _: &[String]) -> Result<Vec<Entry>, String> {
            Ok(vec![entry(1, "猫", "ねこ", "n"); MAX_ROWS + 1])
        }
    }
    assert!(lookup(&request("猫", 0), &mut Overflow)
        .unwrap_err()
        .contains("bounded row"));
}

// Expected transformations generated by executing pinned upstream Yomitan code,
// not inferred from TMW's implementation. GPL-3.0-or-later fixture in vendor.
#[test]
fn pinned_yomitan_comparison_forms_keep_full_original_spans() {
    #[derive(Deserialize)]
    struct Comparison {
        cases: Vec<OracleCase>,
    }
    #[derive(Deserialize)]
    struct OracleCase {
        text: String,
        term: String,
        pos: String,
        matched: bool,
    }
    let corpus: Comparison =
        serde_json::from_str(include_str!("../vendor/yomitan/comparison.json")).unwrap();
    for case in corpus.cases.into_iter().filter(|c| c.matched) {
        for offset in [0, case.text.chars().count() / 2] {
            let mut store = MemoryStore {
                entries: vec![entry(1, &case.term, "", &case.pos)],
                ..Default::default()
            };
            let response = lookup(&request(&case.text, offset), &mut store).unwrap();
            assert_eq!(
                response.target.lemma, case.term,
                "{} offset {offset}",
                case.text
            );
            assert_eq!(
                (response.matched_span.start, response.matched_span.end),
                (0, case.text.chars().count()),
                "{} offset {offset}",
                case.text
            );
        }
    }
}

#[test]
fn user_spoken_examples_preserve_unicode_anchors_and_validate_pos() {
    let text = "信じらんない！　バカ！　死んじゃえ!!";
    for (surface, term, reading, pos) in [
        ("信じらんない", "信じる", "しんじる", "v1"),
        ("死んじゃえ", "死ぬ", "しぬ", "v5n"),
    ] {
        let start = text
            .chars()
            .collect::<Vec<_>>()
            .windows(surface.chars().count())
            .position(|w| w.iter().collect::<String>() == surface)
            .unwrap();
        for relative in [0, 2] {
            let mut store = MemoryStore {
                entries: vec![entry(1, term, reading, pos), entry(2, term, reading, "n")],
                ..Default::default()
            };
            let response = lookup(&request(text, start + relative), &mut store).unwrap();
            assert_eq!(response.target.lemma, term, "{surface} offset {relative}");
            assert_eq!(
                (response.matched_span.start, response.matched_span.end),
                (start, start + surface.chars().count())
            );
            assert_eq!(response.target.surface, surface);
            assert!(response
                .groups
                .iter()
                .flat_map(|g| g.matches.iter())
                .all(|m| m.entry.id != 2));
        }
    }
}

#[test]
fn expressive_internal_small_tsu_is_not_silently_removed() {
    let mut store = MemoryStore {
        entries: vec![entry(1, "信じる", "しんじる", "v1")],
        ..Default::default()
    };
    assert!(lookup(&request("信っじらんない", 0), &mut store)
        .unwrap()
        .groups
        .is_empty());
}
