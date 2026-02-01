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

use crate::config::ServerConfig;
use crate::{HeartbeatDisplay, LifeState};
use chrono::{FixedOffset, TimeZone};
use std::fs::File;
use std::io::Read;
use std::sync::Arc;

pub struct InitialState {
    pub state: LifeState,
    pub last_heartbeat: u64,
    pub note: Option<String>,
    pub heartbeat_display: [HeartbeatDisplay; 5],
}

pub struct Database {
    pub state: String,
    pub last_heartbeat: u64,
    pub note: String,
    pub heartbeat_history: Vec<HeartbeatLog>,
}

pub struct HeartbeatLog {
    pub timestamp: u64,
    pub message: String,
}

/// Reads the given file from the disk and returns the parsed initial state.
pub fn get_initial_state_from_disk(path: &str, config: Arc<ServerConfig>) -> InitialState {
    // read the db file
    let mut db_file: File = match File::open(path) {
        Err(err) => {
            println!("Could not load database file.");
            println!("Cannot start without a database file present.");
            panic!("{}", err)
        }
        Ok(file) => file,
    };
    let mut db_contents: String = String::new();

    db_file
        .read_to_string(&mut db_contents)
        .expect("Failed to read file contents to string.");
    drop(db_file); // we're in the main scope, so lets drop manually here

    // get the initial state from disk
    let mut state: LifeState = LifeState::default();
    let mut last_heartbeat: u64 = 0;
    let mut note: Option<String> = None;

    for (i, line) in db_contents.lines().enumerate() {
        match i {
            0 => {
                if line.is_empty() {
                    panic!("Invalid db entry on line {}", i + 1);
                }
                state = LifeState::from(line);
            }
            1 => {
                last_heartbeat = line
                    .parse::<u64>()
                    .unwrap_or_else(|_| panic!("Invalid timestamp in db file; line {}.", i + 1));
            }
            2 => {
                if !line.is_empty() {
                    note = Some(line.to_owned());
                }
            }
            _ => break,
        }
    }

    // get the latest 5 heartbeats to display
    let mut heartbeat_display: [HeartbeatDisplay; 5] = [
        HeartbeatDisplay::default(),
        HeartbeatDisplay::default(),
        HeartbeatDisplay::default(),
        HeartbeatDisplay::default(),
        HeartbeatDisplay::default(),
    ];

    for (i, line) in db_contents.lines().rev().enumerate() {
        if i > 4 {
            break;
        }
        let line_number: usize = db_contents.lines().count() - i;

        // don't read the first 3 lines, which are reserved for other values stored on disk
        if line_number <= 3 {
            break;
        }
        let split_index: usize = match line.find(" ") {
            Some(index) => index,
            None => panic!("Corrupted database entry on line {}", line_number),
        };
        let data: (&str, &str) = line.split_at(split_index);

        let unix_timestamp: i64 = data
            .0
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("Invalid unix timestamp on line {}", line_number));

        let timezone: FixedOffset =
            FixedOffset::east_opt(config.global.utc_offset * 60 * 60).unwrap();

        let ts: String = timezone
            .timestamp_opt(unix_timestamp, 0)
            .unwrap()
            .to_rfc2822();

        heartbeat_display[i].timestamp = ts;

        let mut message: String = data.1.to_owned();
        let _: char = message.remove(0);

        if !message.is_empty() {
            heartbeat_display[i].message = message;
        }
    }

    InitialState {
        state,
        last_heartbeat,
        note,
        heartbeat_display,
    }
}
