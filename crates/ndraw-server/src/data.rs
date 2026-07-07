//! Compile-time word-list assets.

use std::collections::HashSet;

const EASY: &str = include_str!("data/words-easy.txt");
const MEDIUM: &str = include_str!("data/words-medium.txt");
const HARD: &str = include_str!("data/words-hard.txt");

/// Loads the built-in human-player word catalogue.
///
/// The three difficulty files are embedded in the server binary, so a
/// production container does not depend on mutable data files. Empty lines and
/// lines beginning with `#` are ignored. Normalized duplicates are removed
/// while the first spelling and the source-file order are preserved.
///
/// The `words-bot*.txt` assets remain reserved for a future drawing-bot mode;
/// they are intentionally not mixed into ordinary human games.
#[must_use]
pub fn builtin_words() -> Vec<String> {
    let mut words = Vec::new();
    let mut normalized = HashSet::new();
    for source in [EASY, MEDIUM, HARD] {
        for line in source.lines() {
            let word = line.trim();
            if word.is_empty() || word.starts_with('#') {
                continue;
            }
            if normalized.insert(ndraw_den::guess::normalize(word)) {
                words.push(word.to_owned());
            }
        }
    }
    words
}
