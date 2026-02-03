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

mod api;
mod config;
mod database;
mod pow;
mod state;
mod templating;

use crate::state::{Redundant, ServerState};
use argon2::password_hash::PasswordHash;
use axum::{
    Router,
    routing::{get, post},
};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, broadcast};
use tokio::time::{self, Duration, Interval};

const BIND_ADDRESS: &str = "0.0.0.0:3000";
const CONFIG_PATH: &str = "./config.toml";
const DB_PATH: &str = "./db.txt";
const MAX_DISPLAYED_HEARTBEATS: usize = 5;
const INITIAL_RATE_LIMIT_PERIOD: u64 = 5 * 60;
const RATE_LIMIT_PERIOD_FACTOR: u64 = 2;

#[tokio::main]
async fn main() {
    if !std::path::Path::new(CONFIG_PATH).exists() {
        panic!(
            "Configuration file is missing or not accessible at: {}",
            CONFIG_PATH
        );
    }
    if !std::path::Path::new(DB_PATH).exists() {
        panic!("Database file is missing or not accessible at: {}", DB_PATH);
    }

    // read the configuration file
    let mut conf_file: File = match File::open(CONFIG_PATH) {
        Err(err) => {
            println!("Could not load TOML configuration.");
            println!("Cannot start without a configuration file present.");
            panic!("{}", err)
        }
        Ok(file) => file,
    };
    let mut contents: String = String::new();

    conf_file
        .read_to_string(&mut contents)
        .expect("Failed to read file contents to string.");
    drop(conf_file); // we're in the main scope, so lets drop manually here

    // deserialize the TOML config file to our [`config::ServerConfig`] struct.
    let daemon_config: Arc<config::ServerConfig> = match toml::from_str(contents.as_str()) {
        Ok(config) => Arc::new(config),
        Err(err) => {
            println!("An error occurred while parsing the TOML configuration.");
            panic!("{}", err)
        }
    };
    drop(contents);

    let initial_state: database::InitialState =
        database::get_initial_state_from_disk(DB_PATH, daemon_config.clone());

    // get the unix timestamp of this instant, so we can record the time at which
    // the server was started. useful for avoiding immediately switching to a missing/dead
    // state if the server was down for longer than the maximum silence period.
    let boot_time: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // get the password hash from our config and leak the string so we have
    // a string with a guaranteed static lifetime, required to store the [`PasswordHash`]
    // struct in our app shared state for quick password verification.
    let pwd_hash_str: &mut str = daemon_config.global.heartbeat_auth_hash.clone().leak();

    // broadcast channel for PoW challenges
    let (tx, _) = broadcast::channel::<String>(100);

    let pow_state: pow::PoWState = pow::PoWState {
        secret: daemon_config.pow.secret.clone().leak(), // leak string so it has static lifetime (read-only)
        difficulty: pow::DIFFICULTIES[daemon_config.pow.difficulty as usize - 1].0,
        difficulty_index: daemon_config.pow.difficulty as usize - 1,
        tx: Arc::new(tx),
    };

    // build our state struct
    let server_state: ServerState = ServerState {
        state: Arc::new(Mutex::new(Redundant::new(initial_state.state))),
        last_heartbeat: Arc::new(Mutex::new(Redundant::new(initial_state.last_heartbeat))),
        server_start_time: Redundant::new(boot_time),
        config: daemon_config.clone(),
        password_hash: PasswordHash::new(pwd_hash_str).expect("Invalid Argon2id hash."),
        displayed_heartbeats: Arc::new(Mutex::new(initial_state.heartbeat_display)),
        note: Arc::new(Mutex::new(initial_state.note)),
        baked_status_api_resp: Arc::new(Mutex::new(String::default())),
        rate_limited_ips: Arc::new(Mutex::new(HashMap::default())),
        pow_state,
    };

    // start a tokio job that updates our state every tick interval.
    //
    // this is useful for the digital will to take effect even if
    // no one is sending HTTP requests to serving endpoints
    tokio::spawn({
        let state: ServerState = server_state.clone();

        async move {
            let ival: u64 = state.config.state.tick_interval.into();
            let mut interval: Interval = time::interval(Duration::from_secs(ival * 60));

            loop {
                interval.tick().await;
                println!("Updating state per tick interval.");

                let now: u64 = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                state.update(now).await;
            }
        }
    });

    // start another tokio job that handles broadcasting PoW challenges
    tokio::spawn({
        let state: pow::PoWState = server_state.pow_state.clone();
        async move {
            pow::generate_pow_challenges(state).await;
        }
    });

    // start the web server (with initial state)
    let app: Router = Router::new()
        .route("/", get(templating::index))
        .route("/heartbeat", get(templating::heartbeat))
        .route("/api/status", get(api::status_api))
        .route("/api/heartbeat", post(api::heartbeat_api))
        .route("/api/pow", get(pow::ws_handler))
        .with_state(server_state);

    let listener: TcpListener = tokio::net::TcpListener::bind(BIND_ADDRESS).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
