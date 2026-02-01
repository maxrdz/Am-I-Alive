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

use crate::{RateLimit, ServerState};
use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{MutexGuard, broadcast};
use tokio::time::{Duration, Interval, interval};

const CHALLENGE_INTERVAL: u64 = 50;

pub static DIFFICULTIES: [u128; 5] = [
    0x0fffffffffffffffffffffffffffffff,
    0x00ffffffffffffffffffffffffffffff,
    0x000fffffffffffffffffffffffffffff,
    0x0000ffffffffffffffffffffffffffff,
    0x00000fffffffffffffffffffffffffff,
];

/// State used by the PoW challenge generator Tokio task.
#[derive(Clone)]
pub struct PoWState {
    /// Secret used to generate challenges that can't be predicted.
    pub secret: &'static str,
    pub difficulty: u128,
    /// Tokio async channel for broadcasted PoW challenges for auth rate limiting.
    pub tx: Arc<broadcast::Sender<String>>,
}

/// Generate PoW challenges every 50ms.
pub async fn generate_pow_challenges(pow_state: PoWState) {
    let mut interval: Interval = interval(Duration::from_millis(CHALLENGE_INTERVAL));

    loop {
        interval.tick().await;

        let timestamp_ms: u128 = current_timestamp_ms();
        let seed: String = generate_seed(pow_state.secret, timestamp_ms);

        let challenge = json!({
            "seed": seed,
            "difficulty": format!("{:032x}", pow_state.difficulty),
            "timestamp": timestamp_ms
        });

        let _ = pow_state.tx.send(challenge.to_string());
    }
}

/// Get current timestamp in milliseconds
fn current_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// Generate SHA256(seed + timestamp)
fn generate_seed(secret: &'static str, timestamp_ms: u128) -> String {
    let message: String = format!("{}{}", secret, timestamp_ms);
    let hash = Sha256::digest(message.as_bytes());
    hex::encode(hash)
}

/// WebSocket handler for `/api/pow`, which serves PoW challenges at an interval.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(server_state): State<ServerState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    // we will also enforce the IP-based rate limit block on this WebSocket endpoint
    let ip: IpAddr = addr.ip();
    let locked_map: MutexGuard<'_, HashMap<IpAddr, RateLimit>> =
        server_state.rate_limited_ips.lock().await;

    // check if this address is currently rate limited..
    if let Some(rate_limit) = locked_map.get(&ip) {
        let now: u64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if now < rate_limit.timestamp {
            // return here to enforce rate limit, and send seconds left until retry available
            return Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .header("Retry-After", rate_limit.timestamp - now)
                .body(Body::default())
                .unwrap();
        }
    }

    // finalize the upgrade process by returning upgrade callback.
    // we can customize the callback by sending additional info such as address.
    ws.on_upgrade(move |ws| handle_websocket(ws, server_state.pow_state.tx))
}

async fn handle_websocket(mut socket: WebSocket, tx: Arc<broadcast::Sender<String>>) {
    let mut rx: broadcast::Receiver<String> = tx.subscribe();

    // spawn a task to forward messages from broadcast to websocket
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if socket.send(Message::Text(msg)).await.is_err() {
                    // client disconnected
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
