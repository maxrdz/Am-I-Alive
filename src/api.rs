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
use crate::{LifeState, ServerState};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::{self, Error};
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
