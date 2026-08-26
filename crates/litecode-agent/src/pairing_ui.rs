use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, State},
    http::{StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use qrcode::{QrCode, render::svg};
use serde::Serialize;

use crate::AppState;

const PAGE: &str = include_str!("pairing.html");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingStatus {
    computer_name: String,
    agent_id: String,
    listening_address: String,
    tls_enabled: bool,
    invitation_status: &'static str,
    expires_at_unix: u64,
}

pub async fn page(
    ConnectInfo(source): ConnectInfo<SocketAddr>,
) -> Result<Html<&'static str>, StatusCode> {
    require_loopback(source)?;
    Ok(Html(PAGE))
}

pub async fn status(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
) -> Result<Json<PairingStatus>, StatusCode> {
    require_loopback(source)?;
    let invitation = state.auth.invitation();
    Ok(Json(PairingStatus {
        computer_name: state.computer_name.to_string(),
        agent_id: invitation.agent_id,
        listening_address: state.endpoint.to_string(),
        tls_enabled: state.tls_enabled,
        invitation_status: invitation.status,
        expires_at_unix: invitation.expires_at_unix,
    }))
}

pub async fn qr(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
) -> Result<Response, StatusCode> {
    require_loopback(source)?;
    let invitation = state
        .auth
        .invitation_uri(&state.endpoint, state.fingerprint.as_deref());
    if invitation.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let code = QrCode::new(invitation.as_bytes()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let image = code
        .render::<svg::Color>()
        .min_dimensions(320, 320)
        .quiet_zone(true)
        .dark_color(svg::Color("#111827"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok((
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        image,
    )
        .into_response())
}

pub async fn regenerate(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
) -> Result<StatusCode, StatusCode> {
    require_loopback(source)?;
    state.auth.regenerate_invitation();
    Ok(StatusCode::NO_CONTENT)
}

pub async fn cancel(
    State(state): State<AppState>,
    ConnectInfo(source): ConnectInfo<SocketAddr>,
) -> Result<StatusCode, StatusCode> {
    require_loopback(source)?;
    state.auth.cancel_invitation();
    Ok(StatusCode::NO_CONTENT)
}

fn require_loopback(source: SocketAddr) -> Result<(), StatusCode> {
    source
        .ip()
        .is_loopback()
        .then_some(())
        .ok_or(StatusCode::FORBIDDEN)
}
