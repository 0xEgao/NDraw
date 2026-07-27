//! Axum control-plane routes.

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use ndraw_proto::{ClientToken, RoomSettings};
use serde::{Deserialize, Serialize};
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};

use crate::{AppState, CreateRoomError};

/// JSON request accepted by `POST /v1/rooms`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomRequest {
    /// Browser-generated token whose first join owns host controls.
    pub client_token: ClientToken,
    /// Requested game rules.
    pub settings: RoomSettings,
}

/// JSON response returned after a room actor is registered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoomResponse {
    /// Six-character invite code.
    pub room_code: ndraw_proto::RoomCode,
    /// Public binary WebSocket endpoint.
    pub websocket_url: String,
    /// Unix-millisecond lobby expiration timestamp.
    pub lobby_expires_at_ms: u64,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

pub(crate) fn router(state: Arc<AppState>) -> Router {
    let cors = cors_layer(&state.config().allowed_origins);
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/metrics", get(metrics))
        .route("/v1/rooms", post(create_room))
        .route("/v1/voice/token", get(crate::voice::token))
        .route("/v1/ws/{room_code}", get(crate::ws::upgrade))
        .layer(DefaultBodyLimit::max(8 * 1_024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let layer = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE]);
    if allowed_origins.is_empty() {
        return layer.allow_origin(Any);
    }
    let origins = allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok());
    layer.allow_origin(AllowOrigin::list(origins))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn ready(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> StatusCode {
    if state.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn metrics(axum::extract::State(state): axum::extract::State<Arc<AppState>>) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8"),
        )],
        state.metrics().render(),
    )
        .into_response()
}

async fn create_room(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    Json(request): Json<CreateRoomRequest>,
) -> Result<(StatusCode, Json<CreateRoomResponse>), ApiError> {
    if !state.is_ready() {
        return Err(ApiError::unavailable("server is shutting down"));
    }
    let created = state
        .directory()
        .create(request.client_token, request.settings)
        .map_err(ApiError::from_create)?;
    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse {
            room_code: created.room_code,
            websocket_url: format!(
                "{}/v1/ws/{}",
                state.config().public_ws_base_url,
                created.room_code
            ),
            lobby_expires_at_ms: created.lobby_expires_at_ms,
        }),
    ))
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "unavailable",
            message: message.into(),
        }
    }

    fn from_create(error: CreateRoomError) -> Self {
        match error {
            CreateRoomError::Unavailable | CreateRoomError::CodeSpaceExhausted => {
                Self::unavailable(error.to_string())
            }
            CreateRoomError::InvalidGame(_) | CreateRoomError::InvalidGeneratedCode => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_room",
                message: error.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.code,
                message: self.message,
            }),
        )
            .into_response()
    }
}
