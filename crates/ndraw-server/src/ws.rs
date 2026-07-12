//! Binary WebSocket data-plane integration.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use futures_util::{
    SinkExt, StreamExt,
    stream::{SplitSink, SplitStream},
};
use ndraw_den::{ConnectionLease, JoinRequest, MailboxError, PlayerAction, RoomHandle};
use ndraw_proto::{
    ByeReason, ClientMessage, ErrorCode, ProtocolError, RoomCode, ServerMessage,
    codec::{decode_client, encode_server},
    limit::MAX_FRAME_BYTES,
};
use tokio::{sync::mpsc, time::Instant};

use crate::{AppState, rate_limit::ConnectionRateLimits};

const CONTROL_CAPACITY: usize = 16;

pub(crate) async fn upgrade(
    State(state): State<Arc<AppState>>,
    Path(room_code): Path<RoomCode>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Response {
    if !state.is_ready() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !origin_allowed(&headers, state.config().allowed_origins.as_slice()) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(room) = state.directory().get(room_code) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    upgrade
        .max_message_size(MAX_FRAME_BYTES)
        .max_frame_size(MAX_FRAME_BYTES)
        .on_upgrade(move |socket| serve_socket(socket, room, state))
}

fn origin_allowed(headers: &HeaderMap, allowed: &[String]) -> bool {
    if allowed.is_empty() {
        return true;
    }
    headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|origin| allowed.iter().any(|allowed| allowed == origin))
}

async fn serve_socket(mut socket: WebSocket, room: RoomHandle, state: Arc<AppState>) {
    let hello =
        match receive_hello(&mut socket, state.config().hello_timeout, state.metrics()).await {
            Ok(hello) => hello,
            Err(reason) => {
                send_bye(&mut socket, reason, state.metrics()).await;
                return;
            }
        };
    let (outbound_tx, outbound_rx) = mpsc::channel(state.config().outbound_capacity.max(1));
    let joined = match room
        .join(JoinRequest {
            client_token: hello.client_token,
            profile: hello.profile,
            outbound: outbound_tx,
        })
        .await
    {
        Ok(joined) => joined,
        Err(error) => {
            tracing::debug!(room = %room.room_code(), %error, "WebSocket join rejected");
            send_bye(&mut socket, ByeReason::RoomClosed, state.metrics()).await;
            return;
        }
    };

    state.metrics().socket_connected();
    run_connection(socket, room.clone(), joined.lease, outbound_rx, &state).await;
    let _ignored = room.try_leave(joined.lease);
    state.metrics().socket_disconnected();
}

async fn receive_hello(
    socket: &mut WebSocket,
    timeout: Duration,
    metrics: &crate::ServerMetrics,
) -> Result<ndraw_proto::Hello, ByeReason> {
    let received = tokio::time::timeout(timeout, socket.recv())
        .await
        .map_err(|_| ByeReason::TimedOut)?;
    let message = received
        .ok_or(ByeReason::ProtocolViolation)?
        .map_err(|_| ByeReason::ProtocolViolation)?;
    let Message::Binary(frame) = message else {
        return Err(ByeReason::ProtocolViolation);
    };
    metrics.incoming(frame.len());
    let decoded = decode_client(&frame).map_err(|_| {
        metrics.decode_failure();
        ByeReason::ProtocolViolation
    })?;
    match decoded {
        ClientMessage::Hello(hello) => Ok(hello),
        _ => Err(ByeReason::ProtocolViolation),
    }
}

async fn send_bye(socket: &mut WebSocket, reason: ByeReason, metrics: &crate::ServerMetrics) {
    let message = ServerMessage::Bye { reason };
    if let Ok(frame) = encode_server(&message) {
        metrics.outgoing(frame.len());
        let _ignored = socket.send(Message::Binary(frame.into())).await;
    }
    let _ignored = socket.close().await;
}

async fn run_connection(
    socket: WebSocket,
    room: RoomHandle,
    lease: ConnectionLease,
    outbound_rx: mpsc::Receiver<Arc<ServerMessage>>,
    state: &Arc<AppState>,
) {
    let (sink, stream) = socket.split();
    let (control_tx, control_rx) = mpsc::channel(CONTROL_CAPACITY);
    let started_at = Instant::now();
    let last_activity = Arc::new(AtomicU64::new(0));
    let expected_nonce = Arc::new(AtomicU32::new(0));
    let mut writer = tokio::spawn(writer_task(
        sink,
        outbound_rx,
        control_rx,
        state.metrics().clone(),
    ));
    let mut reader = tokio::spawn(reader_task(
        stream,
        room,
        lease,
        control_tx.clone(),
        Arc::clone(&last_activity),
        Arc::clone(&expected_nonce),
        started_at,
        state.metrics().clone(),
    ));
    let mut ping = tokio::time::interval(state.config().ping_interval);
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ping.tick().await;

    // A `JoinHandle` is itself a future and must never be polled again after
    // `tokio::select!` observes it as ready. Track which half completed so
    // teardown only awaits handles that are still pending.
    let (reader_stopped, writer_stopped) = loop {
        tokio::select! {
            result = &mut reader => {
                if let Err(error) = result {
                    tracing::debug!(%error, "WebSocket reader task stopped unexpectedly");
                }
                let _ignored = control_tx.try_send(WriterCommand::Close);
                break (true, false);
            }
            result = &mut writer => {
                if let Err(error) = result {
                    tracing::debug!(%error, "WebSocket writer task stopped unexpectedly");
                }
                break (false, true);
            }
            _ = ping.tick() => {
                let elapsed = elapsed_milliseconds(started_at);
                let inactive = elapsed.saturating_sub(last_activity.load(Ordering::Acquire));
                if inactive >= duration_milliseconds(state.config().inactivity_timeout) {
                    let _ignored = control_tx.try_send(WriterCommand::CloseAfter(
                        ServerMessage::Bye { reason: ByeReason::TimedOut },
                    ));
                    break (false, false);
                }
                let nonce = expected_nonce.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
                if control_tx
                    .try_send(WriterCommand::Application(ServerMessage::Ping { nonce }))
                    .is_err()
                {
                    state.metrics().slow_client_disconnect();
                    break (false, false);
                }
            }
        }
    };

    if !reader_stopped {
        reader.abort();
        let _ignored = reader.await;
    }
    if !writer_stopped
        && tokio::time::timeout(Duration::from_secs(1), &mut writer)
            .await
            .is_err()
    {
        writer.abort();
        let _ignored = writer.await;
    }
}

#[derive(Debug)]
enum WriterCommand {
    Application(ServerMessage),
    Transport(Message),
    CloseAfter(ServerMessage),
    Close,
}

async fn writer_task(
    mut sink: SplitSink<WebSocket, Message>,
    mut outbound: mpsc::Receiver<Arc<ServerMessage>>,
    mut control: mpsc::Receiver<WriterCommand>,
    metrics: crate::ServerMetrics,
) {
    loop {
        tokio::select! {
            biased;
            command = control.recv() => {
                match command {
                    Some(WriterCommand::Application(message)) => {
                        if send_application(&mut sink, &message, &metrics).await.is_err() { break; }
                    }
                    Some(WriterCommand::Transport(message)) => {
                        if sink.send(message).await.is_err() { break; }
                    }
                    Some(WriterCommand::CloseAfter(message)) => {
                        let _ignored = send_application(&mut sink, &message, &metrics).await;
                        break;
                    }
                    Some(WriterCommand::Close) | None => break,
                }
            }
            message = outbound.recv() => {
                let Some(message) = message else { break; };
                if send_application(&mut sink, message.as_ref(), &metrics).await.is_err() { break; }
            }
        }
    }
    let _ignored = sink.close().await;
}

async fn send_application(
    sink: &mut SplitSink<WebSocket, Message>,
    message: &ServerMessage,
    metrics: &crate::ServerMetrics,
) -> Result<(), ()> {
    let frame = encode_server(message).map_err(|error| {
        tracing::error!(%error, "could not encode authoritative server message");
    })?;
    metrics.outgoing(frame.len());
    sink.send(Message::Binary(frame.into()))
        .await
        .map_err(|_| ())
}

#[allow(clippy::too_many_arguments)]
async fn reader_task(
    mut stream: SplitStream<WebSocket>,
    room: RoomHandle,
    lease: ConnectionLease,
    control: mpsc::Sender<WriterCommand>,
    last_activity: Arc<AtomicU64>,
    expected_nonce: Arc<AtomicU32>,
    started_at: Instant,
    metrics: crate::ServerMetrics,
) {
    let mut limits = ConnectionRateLimits::new(Instant::now());
    while let Some(result) = stream.next().await {
        let message = match result {
            Ok(message) => message,
            Err(_) => break,
        };
        last_activity.store(elapsed_milliseconds(started_at), Ordering::Release);
        match message {
            Message::Binary(frame) => {
                metrics.incoming(frame.len());
                let decoded = match decode_client(&frame) {
                    Ok(decoded) => decoded,
                    Err(_) => {
                        metrics.decode_failure();
                        protocol_close(&control);
                        break;
                    }
                };
                if !limits.allow(&decoded, Instant::now()) {
                    metrics.rate_limit_rejection();
                    send_protocol_error(&control, ErrorCode::RateLimited, "rate limit exceeded");
                    continue;
                }
                match decoded {
                    ClientMessage::Hello(_) => {
                        protocol_close(&control);
                        break;
                    }
                    ClientMessage::Pong { nonce } => {
                        if nonce != expected_nonce.load(Ordering::Acquire) {
                            tracing::trace!(received = nonce, "ignored stale application pong");
                        }
                    }
                    message => {
                        let Some(action) = into_action(message) else {
                            continue;
                        };
                        match room.try_action(lease, action) {
                            Ok(()) => {}
                            Err(MailboxError::Full) => send_protocol_error(
                                &control,
                                ErrorCode::ServerBusy,
                                "room command queue is full",
                            ),
                            Err(MailboxError::Closed) => break,
                        }
                    }
                }
            }
            Message::Ping(payload) => {
                let _ignored = control.try_send(WriterCommand::Transport(Message::Pong(payload)));
            }
            Message::Pong(_) => {}
            Message::Close(_) => break,
            Message::Text(_) => {
                protocol_close(&control);
                break;
            }
        }
    }
}

fn into_action(message: ClientMessage) -> Option<PlayerAction> {
    match message {
        ClientMessage::StartGame => Some(PlayerAction::StartGame),
        ClientMessage::PickWord { choice } => Some(PlayerAction::PickWord { choice }),
        ClientMessage::Draw(operation) => Some(PlayerAction::Draw(operation)),
        ClientMessage::Guess { text } => Some(PlayerAction::Guess { text }),
        ClientMessage::Chat { text } => Some(PlayerAction::Chat { text }),
        ClientMessage::KickPlayer { player_id } => Some(PlayerAction::KickPlayer { player_id }),
        ClientMessage::Rematch => Some(PlayerAction::Rematch),
        ClientMessage::VoteDrawing { turn_id, vote } => {
            Some(PlayerAction::VoteDrawing { turn_id, vote })
        }
        ClientMessage::Hello(_) | ClientMessage::Pong { .. } => None,
    }
}

fn protocol_close(control: &mpsc::Sender<WriterCommand>) {
    let _ignored = control.try_send(WriterCommand::CloseAfter(ServerMessage::Bye {
        reason: ByeReason::ProtocolViolation,
    }));
}

fn send_protocol_error(
    control: &mpsc::Sender<WriterCommand>,
    code: ErrorCode,
    message: &'static str,
) {
    let _ignored = control.try_send(WriterCommand::Application(ServerMessage::Error(
        ProtocolError {
            code,
            message: message.to_owned(),
        },
    )));
}

fn elapsed_milliseconds(start: Instant) -> u64 {
    duration_milliseconds(start.elapsed())
}

fn duration_milliseconds(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
