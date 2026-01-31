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
mod redundancy;
mod templating;

use argon2::password_hash::PasswordHash;
use axum::{
    Router,
    routing::{get, post},
};
use rand::rand_core::OsRng;
use redundancy::Redundant;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, MutexGuard};
use tokio::time::{self, Duration, Interval};

const BIND_ADDRESS: &str = "0.0.0.0:3000";
const CONFIG_PATH: &str = "/app/config.toml";
const DB_PATH: &str = "/app/db.txt";
const MAX_DISPLAYED_HEARTBEATS: usize = 5;
const INITIAL_RATE_LIMIT_PERIOD: u64 = 5 * 60;
const RATE_LIMIT_PERIOD_FACTOR: u64 = 2;

#[derive(Clone)]
struct ServerState {
    state: Arc<Mutex<Redundant<LifeState>>>,
    /// Unix time. We don't use an atomic u64 data type because
    /// we want to make use of our custom anti-memory-corruption data type.
    last_heartbeat: Arc<Mutex<Redundant<u64>>>,
    server_start_time: Redundant<u64>,
    config: Arc<config::ServerConfig>,
    rng: Arc<Mutex<OsRng>>,
    /// The parsed Argon2id password hash from our configuration file.
    /// Used to authenticate new heartbeat requests.
    password_hash: PasswordHash<'static>,
    displayed_heartbeats: [HeartbeatDisplay; MAX_DISPLAYED_HEARTBEATS],
    note: Arc<Mutex<Option<String>>>,
    /// Instead of borrowing locks for the server state on every
    /// API call, just bake a response every time the state is updated.
    ///
    /// This way, every API call is simply a [`String`] clone.
    baked_status_api_resp: Arc<Mutex<String>>,
    /// Store rate limiting expiration timestamps per IPv4/IPv6 address.
    rate_limited_ips: Arc<Mutex<HashMap<SocketAddr, RateLimit>>>,
}

struct RateLimit {
    /// the amount of time (seconds) this rate limit lasts for
    period: u64,
    /// the unix timestamp (seconds) of when the rate limit block expires
    timestamp: u64,
}

impl ServerState {
    /// Called at every point in the program where the latest state
    /// should be returned. (e.g. front page, /api/status)
    ///
    /// Refreshes the shared application state based on current Unix timestamp.
    ///
    async fn update(&self, now_unix_timestamp: u64) {
        let last_seen: u64 = *self.last_heartbeat.lock().await.clone();
        // just a sanity check to make sure this isnt possible past this point
        assert!(
            last_seen < now_unix_timestamp,
            "Last heartbeat recorded happened in the future!"
        );

        let seconds_since_last_seen: u64 = now_unix_timestamp - last_seen;

        let mut locked_state: MutexGuard<'_, Redundant<LifeState>> = self.state.lock().await;
        let mut changed: bool = true;

        match **locked_state {
            LifeState::Alive => {
                // config variable is in hours, so translate to seconds by * 60 * 60.
                let seconds_until_uncertain: u64 =
                    u64::from(self.config.state.time_until_uncertain) * 60 * 60;

                if seconds_since_last_seen > seconds_until_uncertain {
                    *locked_state = Redundant::new(LifeState::ProbablyAlive);
                }
            }
            LifeState::ProbablyAlive => {
                let seconds_until_missing: u64 =
                    u64::from(self.config.state.time_until_missing) * 60 * 60;

                if seconds_since_last_seen > seconds_until_missing {
                    *locked_state = Redundant::new(LifeState::MissingOrDead);
                }
            }
            // other states can only be reached by manual interaction
            // (e.g. trusted user verifying the state of the person, or the person sending a new heartbeat)
            _ => changed = false,
        }
        drop(locked_state);

        if changed {
            // re-bake any baked stuff
            let _: String = api::bake_status_api_response(self.clone()).await;
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum LifeState {
    #[default]
    Alive,
    /// enter this state once we have not received a heartbeat
    /// after the full grace period (default 24 hours)
    ProbablyAlive,
    /// enter this state after the end of the maximum silence period
    MissingOrDead,
    /// enter this state once verified by 1 or more trusted users
    Incapacitated,
    /// enter this state once verified by 1 or more trusted users
    Dead,
}

/// Implement on any enum that represents a state which has an
/// associated visual CSS color on the rendered HTML.
trait AssociatedColor
where
    Self: PartialEq + Eq,
{
    fn css_color(&self) -> String;
}

impl AssociatedColor for LifeState {
    fn css_color(&self) -> String {
        match self {
            LifeState::Alive => "#00cd00".into(),
            LifeState::ProbablyAlive => "#b1d000".into(),
            LifeState::MissingOrDead => "#d80000".into(),
            LifeState::Incapacitated => "#515cef".into(),
            LifeState::Dead => "#828282".into(),
        }
    }
}

impl std::fmt::Display for LifeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alive => write!(f, "ALIVE"),
            Self::ProbablyAlive => write!(f, "PROBABLY ALIVE"),
            Self::MissingOrDead => write!(f, "MISSING OR DEAD"),
            Self::Incapacitated => write!(f, "ALIVE BUT INCAPACITATED"),
            Self::Dead => write!(f, "DEAD"),
        }
    }
}

impl From<&str> for LifeState {
    fn from(value: &str) -> Self {
        match value {
            "0" => Self::Alive,
            "1" => Self::ProbablyAlive,
            "2" => Self::MissingOrDead,
            "3" => Self::Incapacitated,
            "4" => Self::Dead,
            _ => panic!("'{}' does not represent a valid state!", value),
        }
    }
}

#[derive(Clone)]
struct HeartbeatDisplay {
    timestamp: String,
    message: String,
}

impl Default for HeartbeatDisplay {
    fn default() -> Self {
        HeartbeatDisplay {
            timestamp: String::from("N/A"),
            message: String::from("N/A"),
        }
    }
}

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

    // build our state struct
    let server_state: ServerState = ServerState {
        state: Arc::new(Mutex::new(Redundant::new(initial_state.state))),
        last_heartbeat: Arc::new(Mutex::new(Redundant::new(initial_state.last_heartbeat))),
        server_start_time: Redundant::new(boot_time),
        config: daemon_config.clone(),
        rng: Arc::new(Mutex::new(OsRng::default())),
        password_hash: PasswordHash::new(pwd_hash_str).expect("Invalid Argon2id hash."),
        displayed_heartbeats: initial_state.heartbeat_display,
        note: Arc::new(Mutex::new(initial_state.note)),
        baked_status_api_resp: Arc::new(Mutex::new(String::default())),
        rate_limited_ips: Arc::new(Mutex::new(HashMap::default())),
    };

    // start a tokio job that updates our state every tick interval.
    //
    // this is useful for the digital will to take effect even if
    // no one is sending HTTP requests to serving endpoints
    tokio::spawn({
        let state: ServerState = server_state.clone();

        async move {
            let ival: u64 = state.config.state.tick_interval.clone().into();
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

    // start the web server (with initial state)
    let app: Router = Router::new()
        .route("/", get(templating::index))
        .route("/heartbeat", get(templating::heartbeat))
        .route("/api/status", get(api::status_api))
        .route("/api/heartbeat", post(api::heartbeat_api))
        .with_state(server_state);

    let listener: TcpListener = tokio::net::TcpListener::bind(BIND_ADDRESS).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
