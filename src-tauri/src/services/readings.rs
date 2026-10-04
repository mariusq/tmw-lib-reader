use crate::services::performance::{self, Stage};
pub use tmw_japanese_core::readings::{
    dictionary_target, kana_to_romaji, normalize_romaji, DictionaryTarget,
};

pub fn derive_reading(value: &str) -> Option<String> {
    performance::measure(Stage::ReadingDerivation, || {
        tmw_japanese_core::readings::derive_reading(value)
    })
}

pub fn reading_to_romaji(value: &str) -> Option<String> {
    derive_reading(value).map(|reading| kana_to_romaji(&reading))
}
