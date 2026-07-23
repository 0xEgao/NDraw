mod common;

use std::error::Error;

use ndraw_proto::{
    ClientMessage, DecodeError, DrawOp, EncodeError, PlayerProfile, Point, RoomCode,
    RoomCodeParseError, RoomSettings, ServerMessage, Stroke, StrokeId, Validate, ValidationError,
    decode_client, encode_client, encode_server,
    limit::{
        CANVAS_WIDTH, MAX_FRAME_BYTES, MAX_NAME_GRAPHEMES, MAX_POINTS_PER_BATCH, PROTOCOL_VERSION,
    },
};

#[test]
fn room_code_normalizes_to_one_canonical_alphabet() {
    let valid = "ABC234".parse::<RoomCode>();
    assert!(valid.is_ok());
    assert_eq!(valid.map(|code| code.to_string()), Ok("ABC234".to_owned()));

    assert!(matches!(
        "ABC23".parse::<RoomCode>(),
        Err(RoomCodeParseError::InvalidLength { .. })
    ));
    assert!(matches!(
        "ABC01O".parse::<RoomCode>(),
        Err(RoomCodeParseError::InvalidCharacter { .. })
    ));
}

#[test]
fn display_names_use_graphemes_not_bytes() {
    let valid = PlayerProfile {
        display_name: "🧑🏽‍🚀".repeat(MAX_NAME_GRAPHEMES),
        avatar: [0; 8],
    };
    assert!(valid.validate().is_ok());

    let too_long = PlayerProfile {
        display_name: "🧑🏽‍🚀".repeat(MAX_NAME_GRAPHEMES + 1),
        avatar: [0; 8],
    };
    assert!(matches!(
        too_long.validate(),
        Err(ValidationError::TooLong {
            field: "display_name",
            ..
        })
    ));
}

#[test]
fn room_settings_enforce_supported_ranges() {
    assert!(RoomSettings::default().validate().is_ok());

    let invalid = RoomSettings {
        max_players: 1,
        ..RoomSettings::default()
    };
    assert!(matches!(
        invalid.validate(),
        Err(ValidationError::OutOfRange {
            field: "max_players",
            ..
        })
    ));
}

#[test]
fn point_batches_are_nonempty_bounded_and_on_canvas() {
    let empty = DrawOp::Points {
        stroke_id: StrokeId(1),
        sequence: 0,
        points: Vec::new(),
    };
    assert!(matches!(
        empty.validate(),
        Err(ValidationError::TooShort { .. })
    ));

    let oversized = DrawOp::Points {
        stroke_id: StrokeId(1),
        sequence: 0,
        points: vec![Point { x: 0, y: 0 }; MAX_POINTS_PER_BATCH + 1],
    };
    assert!(matches!(
        oversized.validate(),
        Err(ValidationError::TooLong { .. })
    ));

    let outside = DrawOp::Points {
        stroke_id: StrokeId(1),
        sequence: 0,
        points: vec![Point {
            x: CANVAS_WIDTH.saturating_add(1),
            y: 0,
        }],
    };
    assert!(matches!(
        outside.validate(),
        Err(ValidationError::OutOfRange {
            field: "point.x",
            ..
        })
    ));
}

#[test]
fn encode_rejects_invalid_messages() {
    let invalid = ClientMessage::Chat {
        text: "\n".to_owned(),
    };
    assert!(matches!(
        encode_client(&invalid),
        Err(EncodeError::Validation(ValidationError::Empty {
            field: "chat.text"
        }))
    ));
}

#[test]
fn encode_rejects_semantically_valid_payloads_that_exceed_the_frame_limit()
-> Result<(), Box<dyn Error>> {
    let mut messages = common::server_messages()?;
    let first = messages.first_mut();
    assert!(first.is_some());

    if let Some(ServerMessage::Welcome(welcome)) = first {
        welcome.snapshot.canvas.actions = (0..100)
            .map(|stroke_id| {
                ndraw_proto::CanvasAction::Stroke(Stroke {
                    stroke_id: StrokeId(stroke_id),
                    color: 0,
                    width: 1,
                    points: vec![Point { x: 1, y: 1 }; 128],
                })
            })
            .collect();

        assert!(matches!(
            encode_server(&ServerMessage::Welcome(welcome.clone())),
            Err(EncodeError::FrameTooLarge { .. })
        ));
    } else {
        panic!("sample server messages must begin with Welcome");
    }
    Ok(())
}

#[test]
fn decode_rejects_bad_headers_and_trailing_bytes() {
    assert!(matches!(
        decode_client(&[]),
        Err(DecodeError::FrameTooShort { actual: 0 })
    ));
    assert!(matches!(
        decode_client(&[PROTOCOL_VERSION + 1, 0x02]),
        Err(DecodeError::UnsupportedVersion { .. })
    ));
    assert!(matches!(
        decode_client(&[PROTOCOL_VERSION, 0xff]),
        Err(DecodeError::UnknownOpcode {
            direction: "client",
            opcode: 0xff
        })
    ));
    assert!(matches!(
        decode_client(&[PROTOCOL_VERSION, 0x02, 0]),
        Err(DecodeError::TrailingBytes { remaining: 1 })
    ));
}

#[test]
fn decode_rejects_invalid_payloads_after_deserialization() {
    let encoded_empty = postcard::to_stdvec("");
    assert!(encoded_empty.is_ok());

    if let Ok(payload) = encoded_empty {
        let mut frame = vec![PROTOCOL_VERSION, 0x05];
        frame.extend_from_slice(&payload);
        assert!(matches!(
            decode_client(&frame),
            Err(DecodeError::Validation(ValidationError::Empty {
                field: "guess.text"
            }))
        ));
    }
}

#[test]
fn decode_checks_size_before_deserialization() {
    let frame = vec![0; MAX_FRAME_BYTES + 1];
    assert!(matches!(
        decode_client(&frame),
        Err(DecodeError::FrameTooLarge { .. })
    ));
}

#[test]
fn welcome_snapshot_requires_the_assigned_player() -> Result<(), RoomCodeParseError> {
    let mut messages = common::server_messages()?;
    let first = messages.first_mut();
    assert!(first.is_some());

    if let Some(ndraw_proto::ServerMessage::Welcome(welcome)) = first {
        welcome.player_id = ndraw_proto::PlayerId(999);
        assert!(matches!(
            welcome.validate(),
            Err(ValidationError::Inconsistent {
                field: "welcome.player_id",
                ..
            })
        ));
    } else {
        panic!("sample server messages must begin with Welcome");
    }
    Ok(())
}
