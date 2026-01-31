/*
    This file is part of "Am I Alive".

    Copyright © 2026 Max Rodriguez <me@maxrdz.com>

    "Am I Alive" is free software; you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License,
    as published by the Free Software Foundation, either version 3
    of the License, or (at your option) any later version.

    "Am I Alive" is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public
    License along with "Am I Alive". If not, see <https://www.gnu.org/licenses/>.
*/

use crate::redundancy::Redundant;
use crate::{
    INITIAL_RATE_LIMIT_PERIOD, LifeState, RATE_LIMIT_PERIOD_FACTOR, RateLimit, ServerState,
};
use argon2::{Argon2, PasswordVerifier};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Json, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{self, Error};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::MutexGuard;

/// Rust Representation of the JSON response
/// that is served on /api/status.
///
#[derive(Serialize, Deserialize, Debug, Default, Clone)]
struct StatusApiResponse {
    /// [`std::fmt::Display`] output of [`crate::LifeState`]
    pub status: String,
    /// Unix timestamp
    pub last_heartbeat: u64,
    pub active_note: String,
}

impl StatusApiResponse {
    /// Serialize the struct to a JSON string
    fn serve(&self) -> Result<String, Error> {
        serde_json::to_string(self)
    }
}

#[derive(Deserialize)]
pub struct HeartbeatRequest {
    remove_current_note: bool,
    updated_note: String,
    message: String,
    password: String,
    pow: PowSolution,
}

#[derive(Deserialize)]
pub struct PowSolution {
    nonce: u64,
    hash: String,
    timestamp_ms: u64,
}

#[derive(Deserialize)]
pub struct PowChallenge {
    seed: String,
    difficulty: String,
    timestamp_ms: u64,
}

/// Using our shared state, [`ServerState`], build a [`StatusApiResponse`]
/// and serialize it into a JSON string, then update the baked API response
/// JSON string stored in our [`ServerState`].
///
pub async fn bake_status_api_response(server_state: ServerState) -> String {
    // build our response by reading from our shared state
    let mut resp: StatusApiResponse = StatusApiResponse::default();

    let locked_state: MutexGuard<'_, Redundant<LifeState>> = server_state.state.lock().await;
    resp.status = locked_state.to_string();
    drop(locked_state);

    let locked_note: MutexGuard<'_, Option<String>> = server_state.note.lock().await;

    resp.active_note = match locked_note.as_ref() {
        Some(note_content) => note_content.clone(),
        None => "".into(),
    };
    drop(locked_note);

    // finally, serialize our assembled struct to a JSON string
    // and replace the baked response string in our shared state
    let json_string: String = resp
        .serve()
        .expect("Failed to serialize `StatusApiResponse`.");

    let mut locked_baked_resp: MutexGuard<'_, String> =
        server_state.baked_status_api_resp.lock().await;
    locked_baked_resp.clear();
    locked_baked_resp.push_str(&json_string);

    json_string
}

/// Handles requests on `/api/status`.
pub async fn status_api(State(server_state): State<ServerState>) -> impl IntoResponse {
    let now: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    server_state.update(now).await;

    // simply lock the baked response stored in our shared state & clone the JSON string
    let mut baked_response: String = server_state.baked_status_api_resp.lock().await.clone();

    if baked_response.is_empty() {
        // the server may have just been started and this is its first request
        // for this endpoint. our state has not updated since the initial state
        // was loaded from disk, so lets bake a JSON string for our initial state now.
        baked_response = bake_status_api_response(server_state).await;
    }
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(baked_response)
        .unwrap()
}

/// Handles requests on `/api/heartbeat` for registering new heartbeats.
pub async fn heartbeat_api(
    State(server_state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Json(req): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let now: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let mut locked_map: MutexGuard<'_, HashMap<SocketAddr, RateLimit>> =
        server_state.rate_limited_ips.lock().await;
    let mut previous_rate_limit_period: Option<u64> = None;

    // check if this address is currently rate limited..
    if let Some(rate_limit) = locked_map.get(&addr) {
        // store current rate limit wait period in case we need to extend it
        previous_rate_limit_period = Some(rate_limit.timestamp);

        if now < rate_limit.timestamp {
            // return here to enforce rate limit, and send seconds left until retry available
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", rate_limit.timestamp - now)
                .body(Body::default())
                .unwrap();
        }
    }
    // OK, let's authenticate the heartbeat
    if let Err(_) =
        Argon2::default().verify_password(req.password.as_bytes(), &server_state.password_hash)
    {
        // auth failed, let's give them (or extend) a rate limit
        let wait_period: u64 = match previous_rate_limit_period {
            Some(period) => period * RATE_LIMIT_PERIOD_FACTOR,
            None => INITIAL_RATE_LIMIT_PERIOD,
        };
        locked_map.insert(
            addr,
            RateLimit {
                period: wait_period,
                timestamp: now + wait_period,
            },
        );

        return Response::builder()
            .status(StatusCode::FORBIDDEN)
            .header("Retry-After", wait_period)
            .body(Body::default())
            .unwrap();
    }
    if previous_rate_limit_period.is_some() {
        locked_map.remove(&addr);
    }
    drop(locked_map);

    // past this point, we're successfully authenticated + past rate limit check
    let mut locked_note: MutexGuard<'_, Option<String>> = server_state.note.lock().await;

    if req.remove_current_note {
        let _: Option<String> = locked_note.take();
    } else {
        let _: Option<String> = locked_note.replace(req.updated_note);
    }
    drop(locked_note);

    // update the last heartbeat
    let mut locked_heartbeat: MutexGuard<'_, Redundant<u64>> =
        server_state.last_heartbeat.lock().await;
    *locked_heartbeat = Redundant::new(now);
    drop(locked_heartbeat);

    // finally, make sure our state is up-to-date
    server_state.update(now).await;

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::default())
        .unwrap()
}

/// WebSocket handler for `/api/pow`, which serves PoW challenges at an interval.
async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    println!("`{addr} connected to PoW challenge WebSocket endpoint.");
    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |socket| handle_websocket(socket, addr))
}

async fn handle_websocket(mut socket: WebSocket, address: SocketAddr) {}
