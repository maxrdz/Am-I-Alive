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
use std::fmt::{Display, Formatter, Write};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::sync::Arc;
use tokio::fs::write as tokio_write;
use tokio::io::Result as TokioIOResult;

pub struct InitialState {
    pub state: LifeState,
    pub last_heartbeat: u64,
    pub note: Option<String>,
    pub heartbeat_display: [HeartbeatDisplay; 5],
}

#[derive(Debug, Default)]
pub struct Database {
    pub state: String,
    pub last_heartbeat: u64,
    pub note: String,
    pub heartbeat_history: Vec<HeartbeatLog>,
}

impl Database {
    pub async fn write_to_disk(&self) -> TokioIOResult<()> {
        tokio_write(crate::DB_PATH, self.to_string()).await
    }
}

impl Hash for Database {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.state.as_bytes());
        state.write_u64(self.last_heartbeat);
        state.write(self.note.as_bytes());

        for log in self.heartbeat_history.iter() {
            log.hash(state);
        }
    }
}

impl Display for Database {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.state)?;
        f.write_char('\n')?;
        f.write_str(&self.last_heartbeat.to_string())?;
        f.write_char('\n')?;
        f.write_str(&self.note)?;
        f.write_char('\n')?;

        for log in self.heartbeat_history.iter() {
            log.fmt(f)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct HeartbeatLog {
    pub timestamp: u64,
    /// e.g. "16.13.35.105" (IPv4), "2700:3600:a3bf::3" (IPv6)
    pub from_address: String,
    pub message: String,
}

impl Hash for HeartbeatLog {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.timestamp);
        state.write(self.from_address.as_bytes());
        state.write(self.message.as_bytes());
    }
}

impl Display for HeartbeatLog {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.timestamp.to_string())?;
        f.write_char(' ')?;
        f.write_str(&self.from_address)?;
        f.write_char(' ')?;
        f.write_str(&self.message)?;
        f.write_char('\n')
    }
}

pub fn read_db_file(path: &str) -> Result<String, std::io::Error> {
    let mut db_file: File = File::open(path)?;
    let mut db_contents: String = String::new();
    db_file.read_to_string(&mut db_contents)?;
    Ok(db_contents)
}

/// Loads the entire database file onto memory as a [`Database`] struct.
///
pub fn load_database(path: &str) -> Result<Database, std::io::Error> {
    let db_contents: String = read_db_file(path)?;

    // get the db data from disk
    let mut db: Database = Database::default();

    for (i, line) in db_contents.lines().enumerate() {
        match i {
            0 => {
                if line.is_empty() {
                    panic!("Invalid db entry on line {}", i + 1);
                }
                db.state = line.to_owned();
            }
            1 => {
                db.last_heartbeat = line
                    .parse::<u64>()
                    .unwrap_or_else(|_| panic!("Invalid timestamp in db file; line {}.", i + 1));
            }
            2 => {
                db.note = line.to_owned();
            }
            _ => {
                let line_number: usize = db_contents.lines().count() - i;

                let split_index: usize = match line.find(" ") {
                    Some(index) => index,
                    None => panic!("Corrupted database entry on line {}", line_number),
                };
                let data: (&str, &str) = line.split_at(split_index);

                let mut second_half: String = data.1.to_owned();
                let _: char = second_half.remove(0);

                let second_split_index: usize = match second_half.find(" ") {
                    Some(index) => index,
                    None => panic!("Corrupted database entry on line {}", line_number),
                };
                let address_and_msg: (&str, &str) = second_half.split_at(second_split_index);

                let timestamp: u64 = data
                    .0
                    .parse::<u64>()
                    .unwrap_or_else(|_| panic!("Invalid unix timestamp on line {}", line_number));

                let from_address: String = address_and_msg.0.to_owned();
                let mut message: String = address_and_msg.1.to_owned();
                let _: char = message.remove(0);

                db.heartbeat_history.push(HeartbeatLog {
                    timestamp,
                    from_address,
                    message,
                });
            }
        }
    }

    Ok(db)
}

/// Reads the given file from the disk and returns the parsed [`InitialState`].
///
pub fn get_initial_state_from_disk(path: &str, config: Arc<ServerConfig>) -> InitialState {
    let db_contents: String = match read_db_file(path) {
        Err(err) => {
            eprintln!("Could not load database file.");
            eprintln!("Cannot start without a database file present.");
            panic!("{}", err)
        }
        Ok(db) => db,
    };

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

        let mut second_half: String = data.1.to_owned();
        let _: char = second_half.remove(0);

        let second_split_index: usize = match second_half.find(" ") {
            Some(index) => index,
            None => panic!("Corrupted database entry on line {}", line_number),
        };
        let address_and_msg: (_, &str) = second_half.split_at(second_split_index);

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

        let mut message: String = address_and_msg.1.to_owned();
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
