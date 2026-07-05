mod common;

use std::{sync::Arc, time::Duration};

use common::{TestResult, profile, settings};
use ndraw_den::{JoinRequest, PlayerAction, RoomConfig, RoomExitReason, RoomTask, spawn_room};
use ndraw_proto::{ByeReason, ClientToken, GamePhase, RoomCode, ServerMessage};
use tokio::sync::mpsc;

#[tokio::test]
async fn reconnect_fences_the_previous_socket_generation() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (host_tx, mut host_rx) = mpsc::channel(16);
    let first = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Host"),
            outbound: host_tx,
        })
        .await?;
    assert!(matches!(
        host_rx.recv().await.as_deref(),
        Some(ServerMessage::Welcome(_))
    ));

    let guest_token = ClientToken::new();
    let (guest_tx, mut guest_rx) = mpsc::channel(16);
    let _guest = room
        .handle
        .join(JoinRequest {
            client_token: guest_token,
            profile: profile("Guest"),
            outbound: guest_tx,
        })
        .await?;
    assert!(matches!(
        guest_rx.recv().await.as_deref(),
        Some(ServerMessage::Welcome(_))
    ));

    let (replacement_tx, mut replacement_rx) = mpsc::channel(16);
    let replacement = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Ignored on resume"),
            outbound: replacement_tx,
        })
        .await?;
    assert!(replacement.resumed);
    assert!(replacement.lease.generation > first.lease.generation);
    assert!(matches!(
        replacement_rx.recv().await.as_deref(),
        Some(ServerMessage::Resume(_))
    ));

    room.handle.try_leave(first.lease)?;
    room.handle
        .try_action(replacement.lease, PlayerAction::StartGame)?;
    assert!(receive_phase(&mut replacement_rx, GamePhase::ChoosingWord).await);

    room.handle.try_shutdown()?;
    assert_eq!(room.task.await?, RoomExitReason::Shutdown);
    Ok(())
}

#[tokio::test]
async fn an_ongoing_game_accepts_known_tokens_but_rejects_new_players() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (host_tx, mut host_rx) = mpsc::channel(32);
    let host = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Host"),
            outbound: host_tx,
        })
        .await?;
    let _welcome = host_rx.recv().await;

    let guest_token = ClientToken::new();
    let (guest_tx, mut guest_rx) = mpsc::channel(32);
    let _guest = room
        .handle
        .join(JoinRequest {
            client_token: guest_token,
            profile: profile("Guest"),
            outbound: guest_tx,
        })
        .await?;
    let _welcome = guest_rx.recv().await;

    room.handle
        .try_action(host.lease, PlayerAction::StartGame)?;
    assert!(receive_phase(&mut host_rx, GamePhase::ChoosingWord).await);

    let (late_tx, _late_rx) = mpsc::channel(8);
    let late_join = room
        .handle
        .join(JoinRequest {
            client_token: ClientToken::new(),
            profile: profile("Late player"),
            outbound: late_tx,
        })
        .await;
    assert!(matches!(
        late_join,
        Err(ndraw_den::JoinError::GameAlreadyStarted)
    ));

    let (resume_tx, mut resume_rx) = mpsc::channel(8);
    let resumed = room
        .handle
        .join(JoinRequest {
            client_token: guest_token,
            profile: profile("Ignored on resume"),
            outbound: resume_tx,
        })
        .await?;
    assert!(resumed.resumed);
    assert!(matches!(
        resume_rx.recv().await.as_deref(),
        Some(ServerMessage::Resume(_))
    ));

    room.handle.try_shutdown()?;
    assert_eq!(room.task.await?, RoomExitReason::Shutdown);
    Ok(())
}

#[tokio::test]
async fn kicked_tokens_cannot_resume() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (host_tx, mut host_rx) = mpsc::channel(16);
    let host = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Host"),
            outbound: host_tx,
        })
        .await?;
    let _welcome = host_rx.recv().await;

    let guest_token = ClientToken::new();
    let (guest_tx, mut guest_rx) = mpsc::channel(16);
    let guest = room
        .handle
        .join(JoinRequest {
            client_token: guest_token,
            profile: profile("Guest"),
            outbound: guest_tx,
        })
        .await?;
    let _welcome = guest_rx.recv().await;

    room.handle.try_action(
        host.lease,
        PlayerAction::KickPlayer {
            player_id: guest.lease.player_id,
        },
    )?;
    assert!(matches!(
        guest_rx.recv().await.as_deref(),
        Some(ServerMessage::Bye {
            reason: ByeReason::Kicked
        })
    ));

    let (retry_tx, _retry_rx) = mpsc::channel(4);
    let retry = room
        .handle
        .join(JoinRequest {
            client_token: guest_token,
            profile: profile("Guest"),
            outbound: retry_tx,
        })
        .await;
    assert!(matches!(retry, Err(ndraw_den::JoinError::Kicked)));

    room.handle.try_shutdown()?;
    assert_eq!(room.task.await?, RoomExitReason::Shutdown);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_unused_lobby_expires_on_virtual_time() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    tokio::time::advance(Duration::from_secs(121)).await;
    tokio::task::yield_now().await;
    assert_eq!(room.task.await?, RoomExitReason::LobbyExpired);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_abandoned_room_expires_after_its_empty_grace_period() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (outbound, mut messages) = mpsc::channel(4);
    let joined = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Host"),
            outbound,
        })
        .await?;
    let _welcome = messages.recv().await;
    room.handle.try_leave(joined.lease)?;
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(61)).await;
    tokio::task::yield_now().await;
    assert_eq!(room.task.await?, RoomExitReason::Empty);
    Ok(())
}

#[tokio::test]
async fn failed_writer_attachment_can_resume_the_retained_identity() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (closed_tx, closed_rx) = mpsc::channel(1);
    drop(closed_rx);
    let failed = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Host"),
            outbound: closed_tx,
        })
        .await;
    assert!(matches!(
        failed,
        Err(ndraw_den::JoinError::OutboundUnavailable)
    ));

    let (replacement_tx, mut replacement_rx) = mpsc::channel(4);
    let resumed = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Ignored"),
            outbound: replacement_tx,
        })
        .await?;
    assert!(resumed.resumed);
    assert!(matches!(
        replacement_rx.recv().await.as_deref(),
        Some(ServerMessage::Resume(_))
    ));

    room.handle.try_shutdown()?;
    assert_eq!(room.task.await?, RoomExitReason::Shutdown);
    Ok(())
}

#[tokio::test]
async fn a_saturated_outbound_disconnects_only_the_slow_player() -> TestResult {
    let creator = ClientToken::new();
    let room = room(creator, Duration::from_secs(120), Duration::from_secs(60))?;
    let (slow_tx, _slow_rx) = mpsc::channel(1);
    let slow = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Slow host"),
            outbound: slow_tx,
        })
        .await?;

    let (healthy_tx, mut healthy_rx) = mpsc::channel(8);
    let _healthy = room
        .handle
        .join(JoinRequest {
            client_token: ClientToken::new(),
            profile: profile("Healthy guest"),
            outbound: healthy_tx,
        })
        .await?;
    assert!(matches!(
        healthy_rx.recv().await.as_deref(),
        Some(ServerMessage::Welcome(_))
    ));
    assert!(matches!(
        healthy_rx.recv().await.as_deref(),
        Some(ServerMessage::PlayerLeft { player_id }) if *player_id == slow.lease.player_id
    ));

    let (resume_tx, mut resume_rx) = mpsc::channel(8);
    let resumed = room
        .handle
        .join(JoinRequest {
            client_token: creator,
            profile: profile("Ignored"),
            outbound: resume_tx,
        })
        .await?;
    assert!(resumed.resumed);
    assert!(matches!(
        resume_rx.recv().await.as_deref(),
        Some(ServerMessage::Resume(_))
    ));

    room.handle.try_shutdown()?;
    assert_eq!(room.task.await?, RoomExitReason::Shutdown);
    Ok(())
}

fn room(
    creator_token: ClientToken,
    lobby_timeout: Duration,
    empty_timeout: Duration,
) -> Result<RoomTask, Box<dyn std::error::Error + Send + Sync>> {
    let room_code: RoomCode = "ABCDEF".parse()?;
    Ok(spawn_room(RoomConfig {
        room_code,
        room_generation: 3,
        creator_token,
        settings: settings(),
        words: vec!["windmill".to_owned(), "telescope".to_owned()],
        random_seed: [9; 32],
        command_capacity: 32,
        lobby_timeout,
        empty_timeout,
        exit_tx: None,
    })?)
}

async fn receive_phase(
    receiver: &mut mpsc::Receiver<Arc<ServerMessage>>,
    expected: GamePhase,
) -> bool {
    for _ in 0..16 {
        match receiver.recv().await {
            Some(message) => {
                if matches!(
                    message.as_ref(),
                    ServerMessage::PhaseChanged(phase) if phase.phase == expected
                ) {
                    return true;
                }
            }
            None => return false,
        }
    }
    false
}
