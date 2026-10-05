// SPDX-License-Identifier: GPL-3.0-or-later
//! Adapter for the pinned GPL-3.0-or-later Yomitan Japanese transform data.
//! See vendor/yomitan/NOTICE.txt for source and attribution.
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Condition {
    sub_conditions: Option<Vec<String>>,
    #[serde(default)]
    is_dictionary_form: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRule {
    #[serde(rename = "type")]
    kind: String,
    input: String,
    output: String,
    conditions_in: Vec<String>,
    conditions_out: Vec<String>,
}
#[derive(Deserialize)]
struct Data {
    conditions: BTreeMap<String, Condition>,
    rules: Vec<RawRule>,
}
struct Rule {
    whole: bool,
    input: String,
    output: String,
    input_flags: u32,
    output_flags: u32,
}
struct Rules {
    flags: BTreeMap<String, u32>,
    dictionary_flags: BTreeMap<String, u32>,
    suffix: BTreeMap<char, Vec<Rule>>,
    whole: BTreeMap<String, Vec<Rule>>,
}
static RULES: OnceLock<Rules> = OnceLock::new();
fn rules() -> &'static Rules {
    RULES.get_or_init(|| {
        let data: Data = serde_json::from_str(include_str!("../vendor/yomitan/rules.json"))
            .expect("Pinned Yomitan rule data is valid");
        let mut flags = BTreeMap::new();
        let mut next = 0;
        for (name, condition) in &data.conditions {
            if condition.sub_conditions.is_none() {
                assert!(next < 32);
                flags.insert(name.clone(), 1u32 << next);
                next += 1;
            }
        }
        while flags.len() != data.conditions.len() {
            let previous = flags.len();
            for (name, condition) in &data.conditions {
                if flags.contains_key(name) {
                    continue;
                }
                let children = condition.sub_conditions.as_ref().expect("Leaf initialized");
                if children.iter().all(|name| flags.contains_key(name)) {
                    flags.insert(
                        name.clone(),
                        children.iter().fold(0, |bits, name| bits | flags[name]),
                    );
                }
            }
            assert!(flags.len() > previous, "Pinned condition graph is acyclic");
        }
        let dictionary_flags = data
            .conditions
            .iter()
            .filter(|(_, condition)| condition.is_dictionary_form)
            .map(|(name, _)| (name.clone(), flags[name]))
            .collect();
        let mut result = Rules {
            flags,
            dictionary_flags,
            suffix: BTreeMap::new(),
            whole: BTreeMap::new(),
        };
        for raw in data.rules {
            assert!(matches!(raw.kind.as_str(), "suffix" | "wholeWord"));
            let rule = Rule {
                whole: raw.kind == "wholeWord",
                input: raw.input,
                output: raw.output,
                input_flags: raw
                    .conditions_in
                    .iter()
                    .fold(0, |bits, name| bits | result.flags[name]),
                output_flags: raw
                    .conditions_out
                    .iter()
                    .fold(0, |bits, name| bits | result.flags[name]),
            };
            if rule.whole {
                result
                    .whole
                    .entry(rule.input.clone())
                    .or_default()
                    .push(rule);
            } else {
                result
                    .suffix
                    .entry(rule.input.chars().last().expect("Nonempty pinned suffix"))
                    .or_default()
                    .push(rule);
            }
        }
        result
    })
}
/// A zero current mask is unrestricted; otherwise at least one bit must overlap.
pub fn transitions(value: &str, current: u32) -> Vec<(String, u32)> {
    let rules = rules();
    let mut output = Vec::new();
    let suffix = value
        .chars()
        .last()
        .and_then(|last| rules.suffix.get(&last));
    for rule in suffix
        .into_iter()
        .flatten()
        .chain(rules.whole.get(value).into_iter().flatten())
    {
        if current != 0 && current & rule.input_flags == 0 {
            continue;
        }
        if let Some(stem) = value.strip_suffix(&rule.input) {
            output.push((format!("{stem}{}", rule.output), rule.output_flags));
        }
    }
    output
}
pub fn dictionary_flags(tags: &[String]) -> u32 {
    let flags = &rules().dictionary_flags;
    tags.iter().fold(0, |bits, name| {
        let name = if name.starts_with("v5") {
            "v5"
        } else if name.starts_with("vs") {
            "vs"
        } else if name == "v1-s" {
            "v1"
        } else {
            name.as_str()
        };
        bits | flags.get(name).copied().unwrap_or(0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_inventory_and_hierarchical_conditions() {
        let rules = rules();
        assert_eq!(rules.flags.len(), 22);
        assert_eq!(
            rules.suffix.values().map(Vec::len).sum::<usize>()
                + rules.whole.values().map(Vec::len).sum::<usize>(),
            889
        );
        assert_eq!(rules.flags["v1"], rules.flags["v1d"] | rules.flags["v1p"]);
        assert_eq!(dictionary_flags(&["v5k-s".into()]), rules.flags["v5"]);
        assert_eq!(dictionary_flags(&["noun".into()]), 0);
    }
    #[test]
    fn nonzero_conditions_cannot_restart_an_unrestricted_chain() {
        assert!(transitions("食べました", 0)
            .iter()
            .any(|(text, _)| text == "食べます"));
        assert!(transitions("食べました", rules().flags["adj-i"]).is_empty());
        let first = transitions("食べました", 0);
        assert!(first.iter().any(|(text, flags)| text == "食べます"
            && transitions(text, *flags)
                .iter()
                .any(|(text, _)| text == "食べる")));
    }
}
