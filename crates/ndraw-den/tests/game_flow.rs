mod common;

use std::time::Duration;

use common::{TestResult, game, profile};
use ndraw_den::{
    GameTime, PlayerAction, RuleError,
    game::{ROUND_END_DURATION, WORD_CHOICE_DURATION},
};
use ndraw_proto::{AwardReason, DrawingVote, GamePhase, GuessOutcome, ServerMessage};

#[test]
fn completes_a_full_game_and_rematch_without_networking() -> TestResult {
    let mut game = game()?;
    let (host, _) = game.add_player(profile("Host"), true)?;
    let (guest, _) = game.add_player(profile("Guest"), false)?;

    game.apply(host, PlayerAction::StartGame, GameTime(0))?;
    assert_eq!(game.state_view().phase, GamePhase::ChoosingWord);
    assert!(game.snapshot_for(host).word_options.is_some());
    assert!(game.snapshot_for(guest).word_options.is_none());

    game.apply(host, PlayerAction::PickWord { choice: 0 }, GameTime(1))?;
    let first_word = game
        .snapshot_for(host)
        .secret_word
        .ok_or("drawer snapshot did not contain the secret word")?;
    assert!(game.snapshot_for(guest).secret_word.is_none());

    let events = game.apply(
        guest,
        PlayerAction::Guess {
            text: first_word.clone(),
        },
        GameTime(1),
    )?;
    assert!(events.iter().any(|event| matches!(
        event.message,
        ServerMessage::GuessResult(ref result)
            if matches!(result.outcome, GuessOutcome::Correct { points: 500 })
    )));
    assert_eq!(game.state_view().phase, GamePhase::RoundEnd);
    let result = game
        .snapshot_for(guest)
        .turn_result
        .ok_or("missing turn result")?;
    assert_eq!(result.word, first_word);
    assert!(result.awards.iter().any(|award| {
        award.player_id == guest && award.points == 500 && award.reason == AwardReason::CorrectGuess
    }));
    assert!(result.awards.iter().any(|award| {
        award.player_id == host && award.points == 125 && award.reason == AwardReason::DrawerBonus
    }));

    let vote_events = game.apply(
        guest,
        PlayerAction::VoteDrawing {
            turn_id: result.turn_id,
            vote: Some(DrawingVote::Like),
        },
        GameTime(2),
    )?;
    assert!(vote_events.iter().any(|event| matches!(
        event.message,
        ServerMessage::DrawingVoteChanged(ref update)
            if update.likes == 1 && update.dislikes == 0
    )));
    assert_eq!(
        game.snapshot_for(guest)
            .drawing_rating
            .and_then(|rating| rating.viewer_vote),
        Some(DrawingVote::Like)
    );
    assert!(matches!(
        game.apply(
            host,
            PlayerAction::VoteDrawing {
                turn_id: result.turn_id,
                vote: Some(DrawingVote::Dislike),
            },
            GameTime(2),
        ),
        Err(RuleError::DrawerCannotVote)
    ));

    let second_turn_at = 1 + duration_millis(ROUND_END_DURATION);
    game.handle_deadline(GameTime(second_turn_at))?;
    assert_eq!(game.state_view().drawer, Some(guest));
    game.apply(
        guest,
        PlayerAction::PickWord { choice: 0 },
        GameTime(second_turn_at + 1),
    )?;
    let second_word = game
        .snapshot_for(guest)
        .secret_word
        .ok_or("second drawer snapshot did not contain the secret word")?;
    game.apply(
        host,
        PlayerAction::Guess { text: second_word },
        GameTime(second_turn_at + 1),
    )?;
    let game_over_at = second_turn_at + 1 + duration_millis(ROUND_END_DURATION);
    game.handle_deadline(GameTime(game_over_at))?;

    let view = game.state_view();
    assert_eq!(view.phase, GamePhase::GameOver);
    assert_eq!(view.scores, vec![(host, 625), (guest, 625)]);

    game.apply(host, PlayerAction::Rematch, GameTime(game_over_at + 1))?;
    let rematch = game.state_view();
    assert_eq!(rematch.phase, GamePhase::ChoosingWord);
    assert_eq!(rematch.round, 1);
    assert_eq!(rematch.scores, vec![(host, 0), (guest, 0)]);
    Ok(())
}

#[test]
fn deadlines_auto_select_and_end_the_turn() -> TestResult {
    let mut game = game()?;
    let (host, _) = game.add_player(profile("Host"), true)?;
    let (_guest, _) = game.add_player(profile("Guest"), false)?;
    game.apply(host, PlayerAction::StartGame, GameTime(0))?;

    game.handle_deadline(GameTime(duration_millis(WORD_CHOICE_DURATION)))?;
    assert_eq!(game.state_view().phase, GamePhase::Drawing);
    assert!(game.snapshot_for(host).secret_word.is_some());

    game.handle_deadline(GameTime(
        duration_millis(WORD_CHOICE_DURATION) + Duration::from_secs(15).as_millis() as u64,
    ))?;
    assert_eq!(game.state_view().phase, GamePhase::RoundEnd);
    Ok(())
}

#[test]
fn disconnected_host_transfers_and_absent_drawer_is_skipped() -> TestResult {
    let mut game = game()?;
    let (host, _) = game.add_player(profile("Host"), true)?;
    let (guest, _) = game.add_player(profile("Guest"), false)?;

    game.disconnect_player(host, GameTime(0))?;
    assert_eq!(game.state_view().host, Some(guest));
    game.reconnect_player(host)?;
    game.apply(guest, PlayerAction::StartGame, GameTime(1))?;
    assert_eq!(game.state_view().drawer, Some(host));

    game.disconnect_player(host, GameTime(2))?;
    game.handle_deadline(GameTime(duration_millis(WORD_CHOICE_DURATION) + 1))?;
    assert_eq!(game.state_view().drawer, Some(guest));
    Ok(())
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
