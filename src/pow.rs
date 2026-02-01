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

use crate::api::PowSolution;
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

/// Interval, in milliseconds, for sending new PoW challenges over WS.
pub static CHALLENGE_INTERVAL: u64 = 500;
/// Time period, in milliseconds, for which a PoW challenge is valid for.
pub static CHALLENGE_VALID_PERIOD: u128 = 10000;

/// Hardcoded difficulties 1-5 (as per PoW concept article)
/// with their respective expected leading zero hex bytes.
pub static DIFFICULTIES: [(u128, &str); 5] = [
    (0x0fffffffffffffffffffffffffffffff, "0"),
    (0x00ffffffffffffffffffffffffffffff, "00"),
    (0x000fffffffffffffffffffffffffffff, "000"),
    (0x0000ffffffffffffffffffffffffffff, "0000"),
    (0x00000fffffffffffffffffffffffffff, "00000"),
];

/// State used by the PoW challenge generator Tokio task.
#[derive(Clone)]
pub struct PoWState {
    /// Secret used to generate challenges that can't be predicted.
    pub secret: &'static str,
    pub difficulty: u128,
    /// Range 0-4, inclusive.
    pub difficulty_index: usize,
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
            "user_address": "{USER_ADDRESS}", // replaced per web socket connection
            "seed": seed,
            "difficulty": format!("{:032x}", pow_state.difficulty),
            "timestamp": timestamp_ms
        });

        let _ = pow_state.tx.send(challenge.to_string());
    }
}

pub fn verify_pow_solution(state: PoWState, ip: IpAddr, pow: PowSolution) -> bool {
    let now_ms: u128 = current_timestamp_ms();

    if (now_ms - pow.timestamp_ms) > CHALLENGE_VALID_PERIOD {
        // submitted solution too late
        return false;
    }
    // re-generate seed using the solution's timestamp and our secret
    let seed: String = generate_seed(state.secret, pow.timestamp_ms);
    // reconstruct their hash (address + seed + nonce)
    let message: String = format!("{}{}{}", &ip.to_string(), &seed, pow.nonce);
    let hash: String = hex::encode(Sha256::digest(message.as_bytes()));

    if pow.hash != hash {
        // SHA256(address + seed + nonce) does not output the hash they submitted
        return false;
    }

    match pow.hash.find(DIFFICULTIES[state.difficulty_index].1) {
        None => {
            // no continuous n zero bits found in hash
            return false;
        }
        Some(i) => {
            if i != 0 {
                // no leading n zero bits found
                return false;
            }
        }
    }
    true
}

fn current_timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}

/// Generate SHA256(seed + timestamp)
pub fn generate_seed(secret: &'static str, timestamp_ms: u128) -> String {
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
    ws.on_upgrade(move |ws| handle_websocket(ws, ip, server_state.pow_state.tx))
}

async fn handle_websocket(mut socket: WebSocket, ip: IpAddr, tx: Arc<broadcast::Sender<String>>) {
    let mut rx: broadcast::Receiver<String> = tx.subscribe();

    // spawn a task to forward messages from broadcast to websocket
    while let Ok(mut msg) = rx.recv().await {
        // inject user address based on the IP address the server sees they're from
        msg = msg.replace("{USER_ADDRESS}", &ip.to_string());

        if socket.send(Message::Text(msg)).await.is_err() {
            // client disconnected
            break;
        }
    }
}
