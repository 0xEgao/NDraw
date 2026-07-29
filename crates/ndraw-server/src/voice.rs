//! LiveKit access-token minting for optional room voice chat.

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use jsonwebtoken::{EncodingKey, Header, encode};
use ndraw_proto::{PlayerProfile, RoomCode, Validate};
use rand::{Rng, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

use crate::AppState;

const TOKEN_TTL_SECONDS: u64 = 6 * 60 * 60;

#[derive(Debug, Deserialize)]
pub(crate) struct TokenQuery {
    room_code: String,
    display_name: String,
    player_id: Option<u32>,
}

#[derive(Debug, Serialize)]
struct VideoGrant {
    room: String,
    #[serde(rename = "roomJoin")]
    room_join: bool,
    #[serde(rename = "canPublish")]
    can_publish: bool,
    #[serde(rename = "canSubscribe")]
    can_subscribe: bool,
    #[serde(rename = "canPublishData")]
    can_publish_data: bool,
}

#[derive(Debug, Serialize)]
struct Claims {
    iss: String,
    sub: String,
    name: String,
    nbf: u64,
    exp: u64,
    video: VideoGrant,
}

#[derive(Debug, Serialize)]
pub(crate) struct TokenResponse {
    token: String,
    url: String,
}

pub(crate) async fn token(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TokenQuery>,
) -> Result<Json<TokenResponse>, (StatusCode, String)> {
    let room_code = query
        .room_code
        .parse::<RoomCode>()
        .map_err(|_| bad_request("invalid room code"))?;
    let profile = PlayerProfile {
        display_name: query.display_name,
        avatar: [0; 8],
    };
    profile
        .validate()
        .map_err(|_| bad_request("invalid display name"))?;

    if state.directory().get(room_code).is_none() {
        return Err((StatusCode::NOT_FOUND, "room not found".to_owned()));
    }

    let api_key = required_env("LIVEKIT_API_KEY")?;
    let api_secret = required_env("LIVEKIT_API_SECRET")?;
    let url = required_env("LIVEKIT_URL")?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let suffix: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    let identity = query.player_id.map_or_else(
        || format!("guest-{suffix}"),
        |player_id| format!("player-{player_id}-{suffix}"),
    );
    let room = room_code.to_string();
    let claims = Claims {
        iss: api_key,
        sub: identity,
        name: profile.display_name,
        nbf: now,
        exp: now.saturating_add(TOKEN_TTL_SECONDS),
        video: VideoGrant {
            room,
            room_join: true,
            can_publish: true,
            can_subscribe: true,
            can_publish_data: false,
        },
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(api_secret.as_bytes()),
    )
    .map_err(|error| {
        tracing::error!(%error, "failed to sign LiveKit token");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not create voice token".to_owned(),
        )
    })?;

    Ok(Json(TokenResponse { token, url }))
}

fn required_env(name: &'static str) -> Result<String, (StatusCode, String)> {
    std::env::var(name).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "voice chat is not configured".to_owned(),
        )
    })
}

fn bad_request(message: &'static str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, message.to_owned())
}
