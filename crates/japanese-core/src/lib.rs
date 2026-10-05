//! Portable Japanese analysis and the existing desktop JMdict import semantics.
#[cfg(feature = "tokenizer")]
pub mod chunk_lookup;
#[cfg(all(test, feature = "tokenizer"))]
mod chunk_lookup_tests;
pub mod dictionary;
#[cfg(feature = "yomitan")]
pub mod dictionary_storage;
#[cfg(feature = "tokenizer")]
pub mod lookup;
#[cfg(all(feature = "tokenizer", feature = "yomitan"))]
pub mod lookup_storage;
#[cfg(feature = "tokenizer")]
pub mod readings;
#[cfg(feature = "yomitan")]
pub mod yomitan;
#[cfg(feature = "tokenizer")]
mod yomitan_rules;
#[cfg(all(test, feature = "yomitan"))]
mod yomitan_tests;
