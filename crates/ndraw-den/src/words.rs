//! Seeded word offering without repeats inside one deck cycle.

use std::{collections::HashSet, sync::Arc};

use ndraw_proto::limit::MAX_WORD_BYTES;
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};

use crate::{error::WordDeckError, guess::normalize};

/// Deterministic shuffled word deck owned by one game.
#[derive(Debug)]
pub struct WordDeck {
    words: Arc<[String]>,
    order: Vec<usize>,
    cursor: usize,
    rng: StdRng,
}

impl WordDeck {
    /// Validates a pool and initializes deterministic shuffle state.
    pub fn new(
        words: Vec<String>,
        required_offer_size: usize,
        seed: [u8; 32],
    ) -> Result<Self, WordDeckError> {
        let required_offer_size = required_offer_size.max(1);
        let mut normalized = HashSet::with_capacity(words.len());
        for (index, word) in words.iter().enumerate() {
            if word.trim().is_empty() {
                return Err(WordDeckError::InvalidWord {
                    index,
                    reason: "word must not be empty",
                });
            }
            if word.trim() != word {
                return Err(WordDeckError::InvalidWord {
                    index,
                    reason: "leading and trailing whitespace is not allowed",
                });
            }
            if word.len() > MAX_WORD_BYTES {
                return Err(WordDeckError::InvalidWord {
                    index,
                    reason: "word exceeds the protocol byte limit",
                });
            }
            if word.chars().any(char::is_control) {
                return Err(WordDeckError::InvalidWord {
                    index,
                    reason: "word contains a control character",
                });
            }
            if !normalized.insert(normalize(word)) {
                return Err(WordDeckError::DuplicateWord { index });
            }
        }

        if normalized.len() < required_offer_size {
            return Err(WordDeckError::TooSmall {
                required: required_offer_size,
                actual: normalized.len(),
            });
        }

        let mut rng = StdRng::from_seed(seed);
        let mut order: Vec<usize> = (0..words.len()).collect();
        order.shuffle(&mut rng);
        Ok(Self {
            words: words.into(),
            order,
            cursor: 0,
            rng,
        })
    }

    /// Returns a unique offer, reshuffling between exhausted deck cycles.
    ///
    /// Requests larger than the pool are safely limited to the pool size.
    pub fn offer(&mut self, count: usize) -> Vec<String> {
        let count = count.min(self.words.len());
        let mut offered = Vec::with_capacity(count);
        let mut used = HashSet::with_capacity(count);

        while offered.len() < count {
            if self.cursor >= self.order.len() {
                self.order.shuffle(&mut self.rng);
                self.cursor = 0;
            }

            let index = self.order[self.cursor];
            self.cursor += 1;
            if used.insert(index) {
                offered.push(self.words[index].clone());
            }
        }

        offered
    }
}
