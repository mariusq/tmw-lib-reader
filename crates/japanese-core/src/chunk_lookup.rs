//! Bounded chunk lookup using pinned Yomitan Japanese transformations.
//! Phase 3 engine; reader presentation/portable history integration is Phase 4.
use crate::{
    dictionary::normalize_query,
    lookup::{LookupRequest, MatchedSpan, MAX_TEXT_CHARACTERS},
    readings::{dictionary_target_with_span, DictionaryTarget},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_WINDOW: usize = 96;
pub const MAX_STARTS: usize = 8;
pub const MAX_TERM: usize = 32;
pub const MAX_CANDIDATES: usize = 512;
pub const MAX_PENDING: usize = 4096;
pub const MAX_DEPTH: usize = 4;
pub const MAX_ROWS: usize = 4096;
pub const MAX_RESULTS: usize = 12;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// Portable source identity; never a SQLite row ID.
    pub source: String,
    pub title: String,
    pub revision: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TermMetadata {
    pub source: String,
    pub title: String,
    pub mode: String,
    pub data: serde_json::Value,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub assets: BTreeMap<String, String>,
    pub metadata: Vec<TermMetadata>,
    /// Ephemeral ID, scoped to provenance.source.
    pub id: i64,
    pub term: String,
    pub reading: String,
    pub provenance: Provenance,
    pub rules: Vec<String>,
    pub glossary: serde_json::Value,
    pub definition_tags: Vec<String>,
    pub term_tags: Vec<String>,
    pub sequence: i64,
    pub score: f64,
    pub priority: i64,
}
#[derive(Debug, Clone)]
pub struct Candidate {
    pub value: String,
    pub start: usize,
    pub end: usize,
    pub depth: usize,
    pub rule: Option<u32>,
    pub variant: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub entry: Entry,
    pub matched_span: MatchedSpan,
    pub deinflection_depth: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub term: String,
    pub reading: String,
    pub matches: Vec<Match>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub engine_version: u32,
    pub target: DictionaryTarget,
    pub matched_span: MatchedSpan,
    pub groups: Vec<Group>,
}
/// One batch of unique normalized exact term/reading keys. Prefix hits are forbidden.
pub trait Store {
    fn query_batch(&mut self, keys: &[String]) -> Result<Vec<Entry>, String>;
}
/// Compose app-private imports with the existing JMdict store without moving
/// either database or making storage failures look like dictionary misses.
impl<A: Store + ?Sized, B: Store + ?Sized> Store for (&mut A, &mut B) {
    fn query_batch(&mut self, keys: &[String]) -> Result<Vec<Entry>, String> {
        let mut entries = self.0.query_batch(keys)?;
        let other = self.1.query_batch(keys)?;
        if entries.len() + other.len() > MAX_ROWS {
            return Err("Combined dictionary query exceeded its bounded row contract".into());
        }
        entries.extend(other);
        Ok(entries)
    }
}

fn hira(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if ('ァ'..='ヶ').contains(&c) {
                char::from_u32(c as u32 - 0x60).unwrap()
            } else {
                c
            }
        })
        .collect()
}
fn kata(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if ('ぁ'..='ゖ').contains(&c) {
                char::from_u32(c as u32 + 0x60).unwrap()
            } else {
                c
            }
        })
        .collect()
}
fn word(c: char) -> bool {
    c.is_alphanumeric()
        || ('\u{3040}'..='\u{30ff}').contains(&c)
        || ('\u{ff66}'..='\u{ff9f}').contains(&c)
        || c == '々'
}

pub fn candidates(request: &LookupRequest) -> Result<Vec<Candidate>, String> {
    let chars: Vec<char> = request.text.chars().take(MAX_TEXT_CHARACTERS + 1).collect();
    if chars.len() > MAX_TEXT_CHARACTERS {
        return Err("Text window exceeds 16000 characters".into());
    }
    if request.offset >= chars.len() {
        return Err("Dictionary offset is outside the text window".into());
    }
    if !word(chars[request.offset]) {
        return Ok(vec![]);
    }
    let left = request.offset.saturating_sub(MAX_STARTS - 1);
    let right = (request.offset + MAX_WINDOW / 2).min(chars.len());
    let mut queue = VecDeque::new();
    // Prioritize starts nearest the click, but reserve all direct lengths before expansion.
    for start in (left..=request.offset).rev() {
        if chars[start..=request.offset].iter().any(|c| !word(*c)) {
            continue;
        }
        for end in ((request.offset + 1)..=(start + MAX_TERM).min(right)).rev() {
            if chars[start..end].iter().any(|c| !word(*c)) {
                continue;
            }
            let original: String = chars[start..end].iter().collect();
            let value = normalize_query(&original);
            queue.push_back(Candidate {
                value,
                start,
                end,
                depth: 0,
                rule: None,
                variant: false,
            });
        }
    }
    // Exact surfaces get a reserved slot; transformation frontiers then rotate
    // across surfaces, so a productive short ending cannot exhaust longer roots.
    let mut seeds: Vec<_> = queue.into_iter().collect();
    seeds.sort_by_key(|c| {
        (
            chars.get(c.end).is_some_and(|ch| word(*ch)),
            std::cmp::Reverse(c.end - c.start),
            c.start,
        )
    });
    let mut seen = BTreeSet::new();
    let mut output = Vec::new();
    let mut frontiers = VecDeque::new();
    let mut pending = 0usize;
    for c in seeds {
        seen.insert((c.value.clone(), c.start, c.end, c.rule));
        output.push(c.clone());
        let mut frontier = VecDeque::new();
        for value in [hira(&c.value), kata(&c.value)].into_iter() {
            if value != c.value && pending < MAX_PENDING {
                frontier.push_back(Candidate {
                    value,
                    variant: true,
                    ..c.clone()
                });
                pending += 1;
            }
        }
        for (value, rule) in crate::yomitan_rules::transitions(&c.value, 0) {
            if pending < MAX_PENDING {
                frontier.push_back(Candidate {
                    value,
                    depth: 1,
                    rule: Some(rule),
                    ..c.clone()
                });
                pending += 1;
            }
        }
        if !frontier.is_empty() {
            frontiers.push_back(frontier);
        }
    }
    let mut processed = 0;
    while let Some(mut frontier) = frontiers.pop_front() {
        let c = frontier.pop_front().expect("Nonempty frontier");
        pending -= 1;
        processed += 1;
        if processed > MAX_PENDING || output.len() >= MAX_CANDIDATES {
            break;
        }
        if seen.insert((c.value.clone(), c.start, c.end, c.rule)) {
            if c.depth < MAX_DEPTH {
                for (value, rule) in
                    crate::yomitan_rules::transitions(&c.value, c.rule.unwrap_or(0))
                {
                    if pending < MAX_PENDING && value.chars().count() <= MAX_TERM {
                        frontier.push_back(Candidate {
                            value,
                            depth: c.depth + 1,
                            rule: Some(rule),
                            ..c.clone()
                        });
                        pending += 1;
                    }
                }
            }
            output.push(c);
        }
        if !frontier.is_empty() {
            frontiers.push_back(frontier);
        }
    }
    Ok(output)
}

fn compatible(c: &Candidate, e: &Entry) -> bool {
    c.rule.is_none_or(|mask| {
        mask == 0 || mask & crate::yomitan_rules::dictionary_flags(&e.rules) != 0
    })
}
pub fn lookup(request: &LookupRequest, store: &mut impl Store) -> Result<Response, String> {
    let candidates = candidates(request)?;
    let keys: Vec<String> = candidates
        .iter()
        .map(|c| c.value.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows = if keys.is_empty() {
        vec![]
    } else {
        store.query_batch(&keys)?
    };
    if rows.len() > MAX_ROWS {
        return Err("Dictionary query exceeded its bounded row contract".into());
    }
    let mut by_key: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, e) in rows.iter().enumerate() {
        by_key.entry(normalize_query(&e.term)).or_default().push(i);
        if e.reading != e.term {
            by_key
                .entry(normalize_query(&e.reading))
                .or_default()
                .push(i);
        }
    }
    // Keep one best original span per row rather than materializing the
    // candidate x homophone cross product (at most MAX_ROWS matches).
    let mut best: BTreeMap<usize, &Candidate> = BTreeMap::new();
    for c in &candidates {
        for &i in by_key.get(&c.value).into_iter().flatten() {
            let e = &rows[i];
            if compatible(c, e) {
                let rank = |a: &Candidate| {
                    (
                        std::cmp::Reverse(a.end - a.start),
                        a.depth,
                        a.variant,
                        normalize_query(&e.term) != a.value,
                        a.start,
                    )
                };
                if best.get(&i).is_none_or(|old| rank(c) < rank(old)) {
                    best.insert(i, c);
                }
            }
        }
    }
    let mut matches: Vec<_> = best.into_iter().map(|(i, c)| (c, &rows[i])).collect();
    matches.sort_by(|(a, x), (b, y)| {
        (b.end - b.start)
            .cmp(&(a.end - a.start))
            .then(a.depth.cmp(&b.depth))
            .then(a.variant.cmp(&b.variant))
            .then((normalize_query(&x.term) != a.value).cmp(&(normalize_query(&y.term) != b.value)))
            .then(y.priority.cmp(&x.priority))
            .then(y.score.total_cmp(&x.score))
            .then(a.start.cmp(&b.start))
            .then(x.term.cmp(&y.term))
            .then(x.reading.cmp(&y.reading))
            .then(x.provenance.source.cmp(&y.provenance.source))
            .then(x.id.cmp(&y.id))
    });
    let (start, end, lemma, reading) = if let Some((c, e)) = matches.first() {
        (c.start, c.end, e.term.clone(), Some(e.reading.clone()))
    } else {
        // Tokenizer is an optional bounded hint on a miss, never a candidate boundary.
        let chars: Vec<char> = request.text.chars().collect();
        let base = request.offset.saturating_sub(MAX_WINDOW / 2);
        let window: String = chars[base..(base + MAX_WINDOW).min(chars.len())]
            .iter()
            .collect();
        match dictionary_target_with_span(&window, request.offset - base) {
            Ok((t, s, e)) => (base + s, base + e, t.lemma, t.reading),
            Err(_) => (
                request.offset,
                request.offset + 1,
                chars[request.offset].to_string(),
                None,
            ),
        }
    };
    let surface = request.text.chars().skip(start).take(end - start).collect();
    let mut seen = BTreeSet::new();
    let mut groups: Vec<Group> = Vec::new();
    for (c, e) in matches {
        // A response describes a single original match, never combines shorter anchors.
        if (c.start, c.end) != (start, end) || !seen.insert((e.provenance.source.clone(), e.id)) {
            continue;
        }
        if seen.len() > MAX_RESULTS {
            break;
        }
        let index = groups
            .iter()
            .position(|g| g.term == e.term && g.reading == e.reading)
            .unwrap_or_else(|| {
                groups.push(Group {
                    term: e.term.clone(),
                    reading: e.reading.clone(),
                    matches: vec![],
                });
                groups.len() - 1
            });
        groups[index].matches.push(Match {
            entry: e.clone(),
            matched_span: MatchedSpan { start, end },
            deinflection_depth: c.depth,
        });
    }
    Ok(Response {
        engine_version: 2,
        target: DictionaryTarget {
            surface,
            lemma,
            reading,
        },
        matched_span: MatchedSpan { start, end },
        groups,
    })
}
