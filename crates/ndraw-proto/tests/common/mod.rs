#![allow(dead_code)]

use ndraw_proto::{
    AwardReason, CanvasSnapshot, ChatEvent, ClientMessage, ClientToken, DrawEvent, DrawOp,
    DrawingVote, DrawingVoteUpdate, ErrorCode, GamePhase, GuessOutcome, GuessResult, Hello,
    HintView, PhaseView, PlayerId, PlayerProfile, PlayerView, ProtocolError, Resume, RoomCode,
    RoomCodeParseError, RoomSettings, RoomSnapshot, ScoreView, ServerMessage, StrokeId, TurnAward,
    TurnId, TurnResultView, Welcome, WordOptions,
};
use uuid::Uuid;

pub fn profile(name: &str) -> PlayerProfile {
    PlayerProfile {
        display_name: name.to_owned(),
        avatar: [1, 2, 3, 4, 5, 6, 7, 8],
    }
}

pub fn hello() -> Hello {
    Hello {
        client_token: ClientToken(Uuid::from_u128(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff)),
        profile: profile("Ada"),
    }
}

pub fn player() -> PlayerView {
    PlayerView {
        player_id: PlayerId(7),
        profile: profile("Ada"),
        score: 125,
        connected: true,
        is_host: true,
        has_guessed: false,
    }
}

pub fn phase() -> PhaseView {
    PhaseView {
        phase: GamePhase::Lobby,
        round: 0,
        total_rounds: 3,
        drawer: None,
        turn_id: None,
        deadline_unix_ms: Some(1_800_000_000_000),
        masked_word: None,
    }
}

pub fn snapshot() -> RoomSnapshot {
    RoomSnapshot {
        settings: RoomSettings::default(),
        players: vec![player()],
        phase: phase(),
        canvas: CanvasSnapshot::default(),
        word_options: None,
        secret_word: None,
        chat_history: Vec::new(),
        turn_result: None,
        drawing_rating: None,
    }
}

pub fn client_messages() -> Vec<ClientMessage> {
    vec![
        ClientMessage::Hello(hello()),
        ClientMessage::StartGame,
        ClientMessage::PickWord { choice: 2 },
        ClientMessage::Draw(DrawOp::Begin {
            stroke_id: StrokeId(3),
            color: 0x12_34_56,
            width: 8,
            start: ndraw_proto::Point { x: 10, y: 20 },
        }),
        ClientMessage::Draw(DrawOp::Points {
            stroke_id: StrokeId(3),
            sequence: 1,
            points: vec![
                ndraw_proto::Point { x: 11, y: 21 },
                ndraw_proto::Point { x: 12, y: 22 },
            ],
        }),
        ClientMessage::Draw(DrawOp::End {
            stroke_id: StrokeId(3),
            sequence: 2,
        }),
        ClientMessage::Draw(DrawOp::Undo),
        ClientMessage::Draw(DrawOp::Clear),
        ClientMessage::Draw(DrawOp::Fill { color: 0xab_cdef }),
        ClientMessage::Guess {
            text: "cat".to_owned(),
        },
        ClientMessage::Chat {
            text: "nice drawing".to_owned(),
        },
        ClientMessage::KickPlayer {
            player_id: PlayerId(9),
        },
        ClientMessage::Rematch,
        ClientMessage::Pong { nonce: 42 },
        ClientMessage::VoteDrawing {
            turn_id: TurnId(4),
            vote: Some(DrawingVote::Like),
        },
        ClientMessage::VoteDrawing {
            turn_id: TurnId(4),
            vote: None,
        },
    ]
}

pub fn server_messages() -> Result<Vec<ServerMessage>, RoomCodeParseError> {
    let room_code: RoomCode = "ABC234".parse()?;
    let player = player();
    let snapshot = snapshot();

    Ok(vec![
        ServerMessage::Welcome(Welcome {
            player_id: PlayerId(7),
            room_code,
            snapshot: snapshot.clone(),
        }),
        ServerMessage::Resume(Resume {
            player_id: PlayerId(7),
            snapshot,
        }),
        ServerMessage::PlayerJoined(player),
        ServerMessage::PlayerLeft {
            player_id: PlayerId(9),
        },
        ServerMessage::HostChanged {
            player_id: PlayerId(7),
        },
        ServerMessage::PhaseChanged(phase()),
        ServerMessage::WordOptions(WordOptions {
            turn_id: TurnId(4),
            words: vec!["cat", "tree", "train", "moon"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }),
        ServerMessage::SecretWord {
            turn_id: TurnId(4),
            word: "cat".to_owned(),
        },
        ServerMessage::HintRevealed(HintView {
            turn_id: TurnId(4),
            masked_word: "c _ _".to_owned(),
        }),
        ServerMessage::Draw(DrawEvent {
            player_id: PlayerId(7),
            turn_id: TurnId(4),
            operation: DrawOp::Clear,
        }),
        ServerMessage::Chat(ChatEvent {
            player_id: PlayerId(9),
            text: "hello".to_owned(),
        }),
        ServerMessage::GuessResult(GuessResult {
            player_id: PlayerId(9),
            outcome: GuessOutcome::Correct { points: 350 },
        }),
        ServerMessage::ScoreChanged(ScoreView {
            player_id: PlayerId(9),
            total_score: 350,
            delta: 350,
        }),
        ServerMessage::Error(ProtocolError {
            code: ErrorCode::InvalidPhase,
            message: "game is not drawing".to_owned(),
        }),
        ServerMessage::Ping { nonce: 42 },
        ServerMessage::Bye {
            reason: ndraw_proto::ByeReason::RoomClosed,
        },
        ServerMessage::TurnResult(TurnResultView {
            turn_id: TurnId(4),
            word: "cat".to_owned(),
            awards: vec![TurnAward {
                player_id: PlayerId(7),
                points: 350,
                reason: AwardReason::CorrectGuess,
            }],
        }),
        ServerMessage::DrawingVoteChanged(DrawingVoteUpdate {
            turn_id: TurnId(4),
            player_id: PlayerId(7),
            vote: Some(DrawingVote::Like),
            likes: 1,
            dislikes: 0,
        }),
    ])
}
