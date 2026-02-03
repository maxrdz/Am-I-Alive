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

use crate::MAX_DISPLAYED_HEARTBEATS;
use crate::api::bake_status_api_response;
use crate::config::ServerConfig;
use crate::pow::PoWState;
use crate::redundancy::Redundant;
use argon2::password_hash::PasswordHash;
use std::sync::Arc;
use std::{collections::HashMap, net::IpAddr};
use tokio::sync::{Mutex, MutexGuard};

#[derive(Clone)]
pub struct ServerState {
    pub state: Arc<Mutex<Redundant<LifeState>>>,
    /// Unix time. We don't use an atomic u64 data type because
    /// we want to make use of our custom anti-memory-corruption data type.
    pub last_heartbeat: Arc<Mutex<Redundant<u64>>>,
    pub server_start_time: Redundant<u64>,
    pub config: Arc<ServerConfig>,
    /// The parsed Argon2id password hash from our configuration file.
    /// Used to authenticate new heartbeat requests.
    pub password_hash: PasswordHash<'static>,
    pub displayed_heartbeats: Arc<Mutex<[HeartbeatDisplay; MAX_DISPLAYED_HEARTBEATS]>>,
    pub note: Arc<Mutex<Option<String>>>,
    /// Instead of borrowing locks for the server state on every
    /// API call, just bake a response every time the state is updated.
    ///
    /// This way, every API call is simply a [`String`] clone.
    pub baked_status_api_resp: Arc<Mutex<String>>,
    /// Store rate limiting expiration timestamps per IPv4/IPv6 address.
    pub rate_limited_ips: Arc<Mutex<HashMap<IpAddr, RateLimit>>>,
    /// State used by the PoW challenge generator Tokio task.
    pub pow_state: PoWState,
}

pub struct RateLimit {
    /// the amount of time (seconds) this rate limit lasts for
    pub period: u64,
    /// the unix timestamp (seconds) of when the rate limit block expires
    pub timestamp: u64,
}

impl ServerState {
    /// Called at every point in the program where the latest state
    /// should be returned. (e.g. front page, /api/status)
    ///
    /// Refreshes the shared application state based on current Unix timestamp.
    ///
    pub async fn update(&self, now_unix_timestamp: u64) {
        let last_seen: u64 = **self.last_heartbeat.lock().await;
        // just a sanity check to make sure this isnt possible past this point
        assert!(
            last_seen <= now_unix_timestamp,
            "Last heartbeat recorded happened in the future!"
        );

        let seconds_since_last_seen: u64 = now_unix_timestamp - last_seen;

        // config variable is in hours, so translate to seconds by * 60 * 60.
        let seconds_until_uncertain: u64 =
            u64::from(self.config.state.time_until_uncertain) * 60 * 60;

        let mut locked_state: MutexGuard<'_, Redundant<LifeState>> = self.state.lock().await;
        let mut new_state: Option<LifeState> = None;

        match **locked_state {
            LifeState::Alive => {
                if seconds_since_last_seen > seconds_until_uncertain {
                    new_state = Some(LifeState::ProbablyAlive);
                    println!("Entering \"Probably Alive\" state.");
                }
            }
            LifeState::ProbablyAlive => {
                let seconds_until_missing: u64 =
                    u64::from(self.config.state.time_until_missing) * 60 * 60;

                if seconds_since_last_seen > seconds_until_missing {
                    new_state = Some(LifeState::MissingOrDead);
                    println!("Assuming Missing or Dead.");
                }
                // check if the latest heartbeat maybe restores our state back to "Alive"
                if seconds_since_last_seen < seconds_until_uncertain {
                    new_state = Some(LifeState::Alive);
                    println!("Restoring state to \"Alive\".");
                }
            }
            // other states can only be reached by manual interaction
            // (e.g. trusted user verifying the state of the person, or the person sending a new heartbeat)
            _ => {
                // check if the latest heartbeat maybe restores our state back to "Alive"
                if seconds_since_last_seen < seconds_until_uncertain {
                    new_state = Some(LifeState::Alive);
                    println!("Restoring state to \"Alive\".");
                }
            }
        }

        if let Some(state) = new_state {
            match state {
                LifeState::MissingOrDead | LifeState::ProbablyAlive => {
                    let uptime: u64 = now_unix_timestamp - *self.server_start_time;

                    if uptime < (self.config.state.minimum_uptime as u64 * 60) {
                        println!("Holding back from switching state. Server too young.");
                        return;
                    }
                }
                // if we're restoring to an OK state, it's due to human interaction
                // (user sent a heartbeat), so don't hold back
                _ => (),
            }
            *locked_state = Redundant::new(state);
            drop(locked_state);

            // re-bake any baked stuff
            let _: String = bake_status_api_response(self.clone()).await;
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum LifeState {
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
pub trait AssociatedColor
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
pub struct HeartbeatDisplay {
    pub timestamp: String,
    pub message: String,
}

impl Default for HeartbeatDisplay {
    fn default() -> Self {
        HeartbeatDisplay {
            timestamp: String::from("N/A"),
            message: String::from("N/A"),
        }
    }
}
