mod common;

use common::{TestResult, game, profile};
use ndraw_den::{GameTime, JoinError, PlayerAction, RuleError, WordDeck, WordDeckError};
use ndraw_proto::{ChatKind, DrawOp, GuessOutcome, Point, ServerMessage, StrokeId};

#[test]
fn authority_phase_and_privacy_rules_are_enforced() -> TestResult {
    let mut game = game()?;
    let (host, _) = game.add_player(profile("Host"), true)?;
    let (guest, _) = game.add_player(profile("Guest"), false)?;
    let (third, _) = game.add_player(profile("Third"), false)?;

    assert_eq!(
        game.apply(guest, PlayerAction::StartGame, GameTime(0)),
        Err(RuleError::HostOnly)
    );
    game.apply(host, PlayerAction::StartGame, GameTime(0))?;
    assert_eq!(
        game.add_player(profile("Late"), false),
        Err(JoinError::GameAlreadyStarted)
    );
    assert_eq!(
        game.apply(guest, PlayerAction::PickWord { choice: 0 }, GameTime(1),),
        Err(RuleError::DrawerOnly)
    );
    assert_eq!(
        game.apply(host, PlayerAction::PickWord { choice: 9 }, GameTime(1),),
        Err(RuleError::InvalidWordChoice)
    );
    game.apply(host, PlayerAction::PickWord { choice: 0 }, GameTime(1))?;
    let secret = game
        .snapshot_for(host)
        .secret_word
        .ok_or("drawer did not receive a secret word")?;

    assert_eq!(
        game.apply(
            guest,
            PlayerAction::Draw(DrawOp::Begin {
                stroke_id: StrokeId(1),
                color: 0,
                width: 1,
                start: Point { x: 1, y: 1 },
            }),
            GameTime(2),
        ),
        Err(RuleError::DrawerOnly)
    );
    assert_eq!(
        game.apply(
            host,
            PlayerAction::Guess {
                text: secret.clone(),
            },
            GameTime(2),
        ),
        Err(RuleError::DrawerCannotGuess)
    );
    assert_eq!(
        game.apply(
            host,
            PlayerAction::Chat {
                text: format!("the word is {secret}"),
            },
            GameTime(2),
        ),
        Err(RuleError::Spoiler)
    );

    let containing_guess = game.apply(
        guest,
        PlayerAction::Guess {
            text: format!("I think the answer is {secret}"),
        },
        GameTime(2),
    )?;
    assert!(
        containing_guess
            .iter()
            .all(|event| !matches!(event.message, ServerMessage::Chat(_)))
    );

    let close_events = game.apply(
        guest,
        PlayerAction::Guess {
            text: close_variant(&secret),
        },
        GameTime(2),
    )?;
    assert!(close_events.iter().any(|event| matches!(
        event.message,
        ServerMessage::GuessResult(ref result)
            if result.player_id == guest && result.outcome == GuessOutcome::Close
    )));
    assert!(close_events.iter().any(|event| matches!(
        event.message,
        ServerMessage::Chat(ref chat)
            if chat.player_id == guest && chat.kind == ChatKind::Guess
    )));
    assert!(
        game.snapshot_for(third)
            .chat_history
            .iter()
            .any(|chat| chat.player_id == guest && chat.kind == ChatKind::Guess)
    );

    game.apply(
        guest,
        PlayerAction::Guess {
            text: secret.clone(),
        },
        GameTime(2),
    )?;
    assert_eq!(
        game.apply(
            guest,
            PlayerAction::Guess {
                text: secret.clone(),
            },
            GameTime(3),
        ),
        Err(RuleError::AlreadyGuessed)
    );
    game.apply(third, PlayerAction::Guess { text: secret }, GameTime(3))?;
    Ok(())
}

#[test]
fn word_offers_are_unique_reproducible_and_validated() -> TestResult {
    let pool = vec![
        "red fox".to_owned(),
        "windmill".to_owned(),
        "telescope".to_owned(),
    ];
    let mut first = WordDeck::new(pool.clone(), 2, [42; 32])?;
    let mut second = WordDeck::new(pool, 2, [42; 32])?;
    assert_eq!(first.offer(2), second.offer(2));
    let offer = first.offer(3);
    assert_eq!(offer.len(), 3);
    assert_ne!(offer[0], offer[1]);
    assert_ne!(offer[1], offer[2]);

    assert!(matches!(
        WordDeck::new(vec!["T-shirt".to_owned(), "t shirt".to_owned()], 2, [0; 32]),
        Err(WordDeckError::DuplicateWord { index: 1 })
    ));
    Ok(())
}

fn close_variant(secret: &str) -> String {
    let mut characters: Vec<char> = secret.chars().collect();
    if !characters.is_empty() {
        characters.pop();
    }
    characters.into_iter().collect()
}
