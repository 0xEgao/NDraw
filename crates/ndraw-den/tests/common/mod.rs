#![allow(dead_code)]

use std::error::Error;

use ndraw_den::{Game, GameDeadline, GameTime};
use ndraw_proto::{PlayerProfile, RoomSettings};

pub type TestResult = Result<(), Box<dyn Error + Send + Sync>>;

pub fn profile(name: &str) -> PlayerProfile {
    PlayerProfile {
        display_name: name.to_owned(),
        avatar: [0; 8],
    }
}

pub fn settings() -> RoomSettings {
    RoomSettings {
        rounds: 1,
        draw_seconds: 15,
        word_choices: 1,
        max_players: 4,
    }
}

pub fn game() -> Result<Game, ndraw_den::GameBuildError> {
    Game::new(
        settings(),
        vec!["windmill".to_owned(), "telescope".to_owned()],
        [7; 32],
        1_700_000_000_000,
        GameDeadline::after(GameTime::default(), std::time::Duration::from_secs(120)),
    )
}
