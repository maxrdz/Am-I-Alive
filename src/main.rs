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

mod config;
mod database;
mod index;
mod redundancy;

use axum::{Router, routing::get};
use rand::rand_core::OsRng;
use redundancy::Redundant;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const MAX_DISPLAYED_HEARTBEATS: usize = 5;

#[derive(Clone)]
struct ServerState {
    state: Arc<Mutex<Redundant<LifeState>>>,
    /// Unix time. We don't use an atomic u64 data type because
    /// we want to make use of our custom anti-memory-corruption data type.
    last_heartbeat: Arc<Mutex<Redundant<u64>>>,
    server_start_time: Redundant<u64>,
    config: Arc<config::ServerConfig>,
    rng: Arc<Mutex<OsRng>>,
    displayed_heartbeats: [HeartbeatDisplay; MAX_DISPLAYED_HEARTBEATS],
    note: Arc<Mutex<Option<String>>>,
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
    Coma,
    /// enter this state once verified by 1 or more trusted users
    Dead,
}

impl std::fmt::Display for LifeState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Alive => write!(f, "ALIVE"),
            Self::ProbablyAlive => write!(f, "PROBABLY ALIVE"),
            Self::MissingOrDead => write!(f, "MISSING OR DEAD"),
            Self::Coma => write!(f, "ALIVE BUT UNRESPONSIVE"),
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
            "3" => Self::Coma,
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
    // read the configuration file
    let mut conf_file: File = match File::open("config.toml") {
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
        database::get_initial_state_from_disk("db.txt", daemon_config.clone());

    // get the unix timestamp of this instant, so we can record the time at which
    // the server was started. useful for avoiding immediately switching to a missing/dead
    // state if the server was down for longer than the maximum silence period.
    let boot_time: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // start the web server (with initial state)
    let app: Router = Router::new()
        .route("/", get(index::index))
        .with_state(ServerState {
            state: Arc::new(Mutex::new(Redundant::new(initial_state.state))),
            last_heartbeat: Arc::new(Mutex::new(Redundant::new(initial_state.last_heartbeat))),
            server_start_time: Redundant::new(boot_time),
            config: daemon_config,
            rng: Arc::new(Mutex::new(OsRng::default())),
            displayed_heartbeats: initial_state.heartbeat_display,
            note: Arc::new(Mutex::new(initial_state.note)),
        });
    let listener: TcpListener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
