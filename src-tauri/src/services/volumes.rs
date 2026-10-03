use unicode_normalization::UnicodeNormalization;

/// A non-authoritative volume hint extracted from a visible title or filename.
/// Callers must present this only as a suggestion.
pub fn suggested_volume(value: &str) -> Option<f64> {
    let normalized: String = value.nfkc().collect::<String>().to_lowercase();
    let patterns = ["巻", "vol.", "vol ", "第"];
    for pattern in patterns {
        if let Some(position) = normalized.find(pattern) {
            let before = &normalized[..position];
            let after = &normalized[position + pattern.len()..];
            if pattern == "第" {
                if let Some(number) = leading_number(after) {
                    return Some(number);
                }
            } else if let Some(number) = trailing_number(before).or_else(|| leading_number(after)) {
                return Some(number);
            }
        }
    }
    trailing_number(normalized.trim_end_matches(".epub"))
}

fn leading_number(value: &str) -> Option<f64> {
    let text: String = value
        .trim_start()
        .chars()
        .take_while(|character| {
            character.is_ascii_digit() || *character == '.' || *character == '-'
        })
        .collect();
    parse_first_number(&text)
}

fn trailing_number(value: &str) -> Option<f64> {
    let text: String = value
        .chars()
        .rev()
        .take_while(|character| {
            character.is_ascii_digit() || *character == '.' || *character == '-'
        })
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    parse_first_number(&text)
}

fn parse_first_number(value: &str) -> Option<f64> {
    value.split('-').next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::suggested_volume;

    #[test]
    fn recognizes_common_japanese_and_latin_volume_forms() {
        assert_eq!(suggested_volume("作品 １巻"), Some(1.0));
        assert_eq!(suggested_volume("作品 第2巻"), Some(2.0));
        assert_eq!(suggested_volume("Work Vol. 3"), Some(3.0));
        assert_eq!(suggested_volume("Work 4.epub"), Some(4.0));
        assert_eq!(suggested_volume("作品 1-2"), Some(1.0));
    }
}
