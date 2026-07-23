mod common;

use common::TestResult;
use ndraw_den::{
    CanvasError, CanvasState, GameDeadline, GameTime,
    game::scoring::guess_score,
    guess::{contains_secret, is_close, is_correct, normalize},
};
use ndraw_proto::{CanvasAction, DrawOp, Point, StrokeId};

#[test]
fn reconstructs_valid_strokes_and_rejects_bad_sequences() -> TestResult {
    let mut canvas = CanvasState::default();
    let stroke_id = StrokeId(7);
    canvas.apply(&DrawOp::Begin {
        stroke_id,
        color: 0x11_22_33,
        width: 4,
        start: Point { x: 10, y: 20 },
    })?;
    canvas.apply(&DrawOp::Points {
        stroke_id,
        sequence: 0,
        points: vec![Point { x: 11, y: 21 }, Point { x: 12, y: 22 }],
    })?;

    assert_eq!(
        canvas.apply(&DrawOp::End {
            stroke_id,
            sequence: 0,
        }),
        Err(CanvasError::WrongSequence {
            expected: 1,
            received: 0,
        })
    );
    canvas.apply(&DrawOp::End {
        stroke_id,
        sequence: 1,
    })?;
    assert_eq!(canvas.snapshot().actions.len(), 1);
    assert_eq!(canvas.total_points(), 3);

    canvas.apply(&DrawOp::Undo)?;
    assert!(canvas.snapshot().actions.is_empty());
    assert_eq!(canvas.total_points(), 0);
    assert_eq!(
        canvas.apply(&DrawOp::Begin {
            stroke_id,
            color: 0,
            width: 1,
            start: Point { x: 0, y: 0 },
        }),
        Err(CanvasError::DuplicateStroke)
    );
    Ok(())
}

#[test]
fn canvas_fill_is_authoritative_and_reconnect_safe() -> TestResult {
    let mut canvas = CanvasState::default();
    let at = Point { x: 120, y: 240 };
    canvas.apply(&DrawOp::Fill {
        color: 0x12_34_56,
        at,
    })?;
    assert_eq!(
        canvas.snapshot().actions,
        vec![CanvasAction::Fill {
            color: 0x12_34_56,
            at,
        }]
    );

    canvas.apply(&DrawOp::Clear)?;
    assert!(canvas.snapshot().actions.is_empty());
    Ok(())
}

#[test]
fn undo_removes_the_latest_fill_without_losing_previous_strokes() -> TestResult {
    let mut canvas = CanvasState::default();
    let stroke_id = StrokeId(8);
    canvas.apply(&DrawOp::Begin {
        stroke_id,
        color: 0,
        width: 4,
        start: Point { x: 10, y: 10 },
    })?;
    canvas.apply(&DrawOp::End {
        stroke_id,
        sequence: 0,
    })?;
    canvas.apply(&DrawOp::Fill {
        color: 0xff_00_00,
        at: Point { x: 20, y: 20 },
    })?;
    canvas.apply(&DrawOp::Undo)?;

    assert!(matches!(
        canvas.snapshot().actions.as_slice(),
        [CanvasAction::Stroke(stroke)] if stroke.stroke_id == stroke_id
    ));
    assert_eq!(canvas.total_points(), 1);
    Ok(())
}

#[test]
fn normalizes_guesses_and_blocks_complete_secret_phrases() {
    assert_eq!(normalize("  T\u{2011}SHIRT!! "), "t shirt");
    assert!(is_correct("T-shirt", "t shirt"));
    assert!(is_close("telescop", "telescope"));
    assert!(!is_close("bicycle", "telescope"));
    assert!(contains_secret("I think it is a RED--FOX!", "red fox"));
    assert!(!contains_secret("the foxtrot is red", "fox"));
}

#[test]
fn score_is_bounded_and_rounded_from_server_time() {
    let deadline = GameDeadline(80_000);
    assert_eq!(guess_score(deadline, GameTime(0), 80_000), 500);
    assert_eq!(guess_score(deadline, GameTime(40_000), 80_000), 250);
    assert_eq!(guess_score(deadline, GameTime(79_999), 80_000), 50);
    assert_eq!(guess_score(deadline, GameTime(90_000), 80_000), 50);
}
