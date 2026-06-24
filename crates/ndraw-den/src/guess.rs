//! Guess normalization, spoiler detection, and close-guess classification.

use strsim::levenshtein;
use unicode_normalization::UnicodeNormalization;

/// Produces the canonical form used for guesses and word-pool uniqueness.
///
/// Unicode is NFKC-normalized and lowercased. Runs of whitespace and
/// punctuation become one ASCII space, allowing `t-shirt` and `t shirt` to
/// compare equally without relying on locale-specific behavior.
#[must_use]
pub fn normalize(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut pending_space = false;

    for character in value.nfkc().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            if pending_space && !output.is_empty() {
                output.push(' ');
            }
            pending_space = false;
            output.push(character);
        } else {
            pending_space = true;
        }
    }

    output
}

/// Returns whether two values normalize to the same guess.
#[must_use]
pub fn is_correct(guess: &str, secret: &str) -> bool {
    let guess = normalize(guess);
    !guess.is_empty() && guess == normalize(secret)
}

/// Returns whether an incorrect guess is close enough for a private hint.
#[must_use]
pub fn is_close(guess: &str, secret: &str) -> bool {
    let guess = normalize(guess);
    let secret = normalize(secret);
    if guess.is_empty() || secret.is_empty() || guess == secret {
        return false;
    }

    let secret_len = secret.chars().count();
    let maximum_distance = match secret_len {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    };
    levenshtein(&guess, &secret) <= maximum_distance
}

/// Detects the complete normalized secret as a phrase in normalized chat.
#[must_use]
pub fn contains_secret(message: &str, secret: &str) -> bool {
    let message = normalize(message);
    let secret = normalize(secret);
    if message.is_empty() || secret.is_empty() {
        return false;
    }

    let padded_message = format!(" {message} ");
    let padded_secret = format!(" {secret} ");
    padded_message.contains(&padded_secret)
}
