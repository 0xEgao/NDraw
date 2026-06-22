mod common;

use std::error::Error;

use ndraw_proto::{
    ClientMessage, DrawEvent, DrawOp, DrawingVote, DrawingVoteUpdate, PlayerId, ServerMessage,
    TurnId, encode_client, encode_server,
};

#[test]
fn unit_client_opcode_is_stable() -> Result<(), Box<dyn Error>> {
    let frame = encode_client(&ClientMessage::StartGame)?;
    assert_eq!(hex::encode(frame), "0202");
    Ok(())
}

#[test]
fn ping_opcode_and_payload_are_stable() -> Result<(), Box<dyn Error>> {
    let frame = encode_server(&ServerMessage::Ping { nonce: 42 })?;
    assert_eq!(hex::encode(frame), "028f2a");
    Ok(())
}

#[test]
fn hello_fixture_is_visible_for_cross_language_clients() -> Result<(), Box<dyn Error>> {
    let frame = encode_client(&ClientMessage::Hello(common::hello()))?;
    assert_eq!(
        hex::encode(frame),
        "02011000112233445566778899aabbccddeeff034164610102030405060708"
    );
    Ok(())
}

#[test]
fn drawing_vote_fixtures_match_browser_codec() -> Result<(), Box<dyn Error>> {
    let client = encode_client(&ClientMessage::VoteDrawing {
        turn_id: TurnId(4),
        vote: Some(DrawingVote::Like),
    })?;
    assert_eq!(hex::encode(client), "020a040100");

    let server = encode_server(&ServerMessage::DrawingVoteChanged(DrawingVoteUpdate {
        turn_id: TurnId(4),
        player_id: PlayerId(7),
        vote: Some(DrawingVote::Like),
        likes: 1,
        dislikes: 0,
    }))?;
    assert_eq!(hex::encode(server), "0292040701000100");
    Ok(())
}

#[test]
fn canvas_fill_fixture_matches_browser_codec() -> Result<(), Box<dyn Error>> {
    let client = encode_client(&ClientMessage::Draw(DrawOp::Fill { color: 42 }))?;
    assert_eq!(hex::encode(client), "0204052a");

    let server = encode_server(&ServerMessage::Draw(DrawEvent {
        player_id: PlayerId(7),
        turn_id: TurnId(4),
        operation: DrawOp::Fill { color: 42 },
    }))?;
    assert_eq!(hex::encode(server), "028a0704052a");
    Ok(())
}
