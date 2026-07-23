use std::{collections::HashSet, error::Error, time::Duration};

use ndraw_den::{WordDeck, guess::normalize};
use ndraw_proto::limit::MAX_WORD_CHOICES;
use ndraw_server::{ServerConfig, builtin_words};

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

#[test]
fn built_in_human_catalogue_is_large_unique_and_game_ready() -> TestResult {
    let words = builtin_words();
    assert!(words.len() >= 1_500);
    let normalized: HashSet<String> = words.iter().map(|word| normalize(word)).collect();
    assert_eq!(normalized.len(), words.len());
    let _deck = WordDeck::new(words, usize::from(MAX_WORD_CHOICES), [11; 32])?;
    Ok(())
}

#[test]
fn default_server_configuration_uses_the_embedded_catalogue() {
    let config = ServerConfig::default();
    assert_eq!(config.words, builtin_words());
    assert_eq!(config.lobby_timeout, Duration::from_secs(180));
}
