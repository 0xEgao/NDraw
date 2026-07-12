use std::{error::Error, sync::Arc, time::Duration};

use futures_util::{SinkExt, StreamExt};
use ndraw_proto::{
    ByeReason, ClientMessage, ClientToken, DrawOp, ErrorCode, GamePhase, Hello, PlayerProfile,
    RoomSettings, ServerMessage,
    codec::{decode_server, encode_client},
};
use ndraw_server::{AppState, CreateRoomRequest, CreateRoomResponse, ServerConfig, build_router};
use tokio::{net::TcpStream, task::JoinHandle};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tokio_util::sync::CancellationToken;

type TestResult = Result<(), Box<dyn Error + Send + Sync>>;
type TestSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct RunningServer {
    http_base: String,
    ws_base: String,
    state: Arc<AppState>,
    cancellation: CancellationToken,
    task: JoinHandle<Result<(), std::io::Error>>,
}

impl RunningServer {
    async fn start(mut config: ServerConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let http_base = format!("http://{address}");
        let ws_base = format!("ws://{address}");
        config.public_ws_base_url = ws_base.clone();
        let state = Arc::new(AppState::new(config));
        let cancellation = CancellationToken::new();
        let shutdown = cancellation.clone();
        let app = build_router(Arc::clone(&state));
        let task = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move { shutdown.cancelled().await })
                .await
        });
        Ok(Self {
            http_base,
            ws_base,
            state,
            cancellation,
            task,
        })
    }

    async fn stop(self) -> TestResult {
        self.state.shutdown().await;
        self.cancellation.cancel();
        self.task.await??;
        Ok(())
    }
}

#[tokio::test]
async fn control_plane_creates_rooms_and_exports_metrics() -> TestResult {
    let server = RunningServer::start(ServerConfig::default()).await?;
    let client = reqwest::Client::new();
    assert_eq!(
        client
            .get(format!("{}/healthz", server.http_base))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::OK
    );
    assert_eq!(
        client
            .get(format!("{}/readyz", server.http_base))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::OK
    );

    let token = ClientToken::new();
    let created = create_room(&server, token).await?;
    assert!(created.websocket_url.starts_with(&server.ws_base));
    assert_eq!(created.room_code.to_string().len(), 6);

    let metrics = client
        .get(format!("{}/metrics", server.http_base))
        .send()
        .await?
        .text()
        .await?;
    assert!(metrics.contains("ndraw_active_rooms 1"));
    assert!(metrics.contains("ndraw_rooms_created_total 1"));

    let invalid = client
        .post(format!("{}/v1/rooms", server.http_base))
        .json(&CreateRoomRequest {
            client_token: ClientToken::new(),
            settings: RoomSettings {
                rounds: 0,
                ..settings()
            },
        })
        .send()
        .await?;
    assert_eq!(invalid.status(), reqwest::StatusCode::BAD_REQUEST);

    server.state.shutdown().await;
    assert_eq!(
        client
            .get(format!("{}/readyz", server.http_base))
            .send()
            .await?
            .status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    let rejected_during_shutdown = client
        .post(format!("{}/v1/rooms", server.http_base))
        .json(&CreateRoomRequest {
            client_token: ClientToken::new(),
            settings: settings(),
        })
        .send()
        .await?;
    assert_eq!(
        rejected_during_shutdown.status(),
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    server.stop().await
}

#[tokio::test]
async fn websocket_drives_gameplay_and_replaces_an_old_connection() -> TestResult {
    let server = RunningServer::start(ServerConfig::default()).await?;
    let host_token = ClientToken::new();
    let guest_token = ClientToken::new();
    let created = create_room(&server, host_token).await?;

    let (mut host, _) = connect_async(&created.websocket_url).await?;
    send_client(&mut host, ClientMessage::Hello(hello(host_token, "Host"))).await?;
    assert!(matches!(
        receive_server(&mut host).await?,
        ServerMessage::Welcome(_)
    ));

    let (mut old_guest, _) = connect_async(&created.websocket_url).await?;
    send_client(
        &mut old_guest,
        ClientMessage::Hello(hello(guest_token, "Guest")),
    )
    .await?;
    assert!(matches!(
        receive_server(&mut old_guest).await?,
        ServerMessage::Welcome(_)
    ));

    send_client(&mut host, ClientMessage::StartGame).await?;
    let phase = receive_matching(&mut host, |message| {
        matches!(message, ServerMessage::PhaseChanged(phase) if phase.phase == GamePhase::ChoosingWord)
    })
    .await?;
    assert!(matches!(phase, ServerMessage::PhaseChanged(_)));
    let options = receive_matching(&mut host, |message| {
        matches!(message, ServerMessage::WordOptions(_))
    })
    .await?;
    assert!(matches!(options, ServerMessage::WordOptions(_)));

    send_client(&mut host, ClientMessage::PickWord { choice: 0 }).await?;
    let secret_message = receive_matching(&mut host, |message| {
        matches!(message, ServerMessage::SecretWord { .. })
    })
    .await?;
    let secret = match secret_message {
        ServerMessage::SecretWord { word, .. } => word,
        _ => return Err(test_error("expected secret word")),
    };
    let guest_phase = receive_matching(&mut old_guest, |message| {
        matches!(message, ServerMessage::PhaseChanged(phase) if phase.phase == GamePhase::Drawing)
    })
    .await?;
    assert!(matches!(guest_phase, ServerMessage::PhaseChanged(_)));

    send_client(&mut host, ClientMessage::Draw(DrawOp::Clear)).await?;
    assert!(matches!(
        receive_matching(&mut old_guest, |message| matches!(
            message,
            ServerMessage::Draw(_)
        ))
        .await?,
        ServerMessage::Draw(_)
    ));
    send_client(
        &mut old_guest,
        ClientMessage::Guess {
            text: secret.clone(),
        },
    )
    .await?;
    assert!(matches!(
        receive_matching(&mut old_guest, |message| matches!(
            message,
            ServerMessage::GuessResult(_)
        ))
        .await?,
        ServerMessage::GuessResult(_)
    ));

    let (mut resumed_guest, _) = connect_async(&created.websocket_url).await?;
    send_client(
        &mut resumed_guest,
        ClientMessage::Hello(hello(guest_token, "Changed name is ignored")),
    )
    .await?;
    assert!(matches!(
        receive_server(&mut resumed_guest).await?,
        ServerMessage::Resume(_)
    ));
    assert!(matches!(
        receive_matching(&mut old_guest, |message| matches!(
            message,
            ServerMessage::Bye {
                reason: ByeReason::Replaced
            }
        ))
        .await?,
        ServerMessage::Bye {
            reason: ByeReason::Replaced
        }
    ));

    send_client(
        &mut host,
        ClientMessage::Chat {
            text: "still connected".to_owned(),
        },
    )
    .await?;
    assert!(matches!(
        receive_matching(&mut resumed_guest, |message| matches!(
            message,
            ServerMessage::Chat(_)
        ))
        .await?,
        ServerMessage::Chat(_)
    ));

    let _ignored = host.close(None).await;
    let _ignored = resumed_guest.close(None).await;
    server.stop().await
}

#[tokio::test]
async fn origin_policy_and_protocol_violations_are_enforced() -> TestResult {
    let config = ServerConfig {
        allowed_origins: vec!["https://game.example".to_owned()],
        ..ServerConfig::default()
    };
    let server = RunningServer::start(config).await?;
    let token = ClientToken::new();
    let created = create_room(&server, token).await?;

    let mut rejected_request = created.websocket_url.clone().into_client_request()?;
    rejected_request
        .headers_mut()
        .insert("origin", "https://evil.example".parse()?);
    let rejection = connect_async(rejected_request).await;
    assert!(matches!(
        rejection,
        Err(tokio_tungstenite::tungstenite::Error::Http(response))
            if response.status() == 403
    ));

    let mut accepted_request = created.websocket_url.clone().into_client_request()?;
    accepted_request
        .headers_mut()
        .insert("origin", "https://game.example".parse()?);
    let (mut socket, _) = connect_async(accepted_request).await?;
    socket
        .send(Message::Text("json is forbidden".into()))
        .await?;
    assert!(matches!(
        receive_server(&mut socket).await?,
        ServerMessage::Bye {
            reason: ByeReason::ProtocolViolation
        }
    ));
    server.stop().await
}

#[tokio::test]
async fn drawing_rate_limit_returns_a_controlled_error() -> TestResult {
    let server = RunningServer::start(ServerConfig::default()).await?;
    let host_token = ClientToken::new();
    let created = create_room(&server, host_token).await?;
    let (mut host, _) = connect_async(&created.websocket_url).await?;
    send_client(&mut host, ClientMessage::Hello(hello(host_token, "Host"))).await?;
    let _welcome = receive_server(&mut host).await?;

    for _ in 0..61 {
        send_client(&mut host, ClientMessage::Draw(DrawOp::Clear)).await?;
    }
    let rate_limited = receive_matching(&mut host, |message| {
        matches!(
            message,
            ServerMessage::Error(error) if error.code == ErrorCode::RateLimited
        )
    })
    .await?;
    assert!(matches!(
        rate_limited,
        ServerMessage::Error(error) if error.code == ErrorCode::RateLimited
    ));
    let _ignored = host.close(None).await;
    server.stop().await
}

async fn create_room(
    server: &RunningServer,
    token: ClientToken,
) -> Result<CreateRoomResponse, Box<dyn Error + Send + Sync>> {
    let response = reqwest::Client::new()
        .post(format!("{}/v1/rooms", server.http_base))
        .json(&CreateRoomRequest {
            client_token: token,
            settings: settings(),
        })
        .send()
        .await?;
    if response.status() != reqwest::StatusCode::CREATED {
        return Err(test_error("room creation did not return 201"));
    }
    Ok(response.json().await?)
}

fn settings() -> RoomSettings {
    RoomSettings {
        rounds: 1,
        draw_seconds: 15,
        word_choices: 1,
        max_players: 4,
    }
}

fn hello(token: ClientToken, name: &str) -> Hello {
    Hello {
        client_token: token,
        profile: PlayerProfile {
            display_name: name.to_owned(),
            avatar: [0; 8],
        },
    }
}

async fn send_client(socket: &mut TestSocket, message: ClientMessage) -> TestResult {
    let frame = encode_client(&message)?;
    socket.send(Message::Binary(frame.into())).await?;
    Ok(())
}

async fn receive_server(
    socket: &mut TestSocket,
) -> Result<ServerMessage, Box<dyn Error + Send + Sync>> {
    let received = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .map_err(|_| test_error("timed out waiting for server message"))?;
    let message =
        received.ok_or_else(|| test_error("WebSocket closed before the expected message"))??;
    let Message::Binary(frame) = message else {
        return Err(test_error("expected a binary application message"));
    };
    Ok(decode_server(&frame)?)
}

async fn receive_matching<F>(
    socket: &mut TestSocket,
    predicate: F,
) -> Result<ServerMessage, Box<dyn Error + Send + Sync>>
where
    F: Fn(&ServerMessage) -> bool,
{
    for _ in 0..128 {
        let message = receive_server(socket).await?;
        if predicate(&message) {
            return Ok(message);
        }
    }
    Err(test_error("matching server message was not received"))
}

fn test_error(message: &'static str) -> Box<dyn Error + Send + Sync> {
    Box::new(std::io::Error::other(message))
}
