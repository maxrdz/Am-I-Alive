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
use crate::{AssociatedColor, HeartbeatDisplay, LifeState, ServerState};
use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
};
use rand::rand_core::{OsRng, TryRngCore};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::MutexGuard;

// any specific IDs that we assign to HTML elements
// dynamically depending on our state
const HIDE_CSS_ID: &str = "hidden";
const DEAD_CSS_ID: &str = "dead";

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    name: String,
    status_color: String,
    status_image: String,
    status_title: String,
    status_message: String,
    row_1_timestamp: String,
    row_1_message: String,
    row_2_timestamp: String,
    row_2_message: String,
    row_3_timestamp: String,
    row_3_message: String,
    row_4_timestamp: String,
    row_4_message: String,
    row_5_timestamp: String,
    row_5_message: String,
    show_note: String,
    note_message: String,
    is_dead: String,
}

pub async fn index(State(server_state): State<ServerState>) -> impl IntoResponse {
    let now: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    server_state.update(now).await;

    // first get a random number from the OS rng
    let mut rng: MutexGuard<'_, OsRng> = server_state.rng.lock().await;
    let img_randint: u64 = rng.try_next_u64().expect("OS RNG error.");
    let msg_randint: u64 = rng.try_next_u64().expect("OS RNG error.");
    drop(rng);

    let locked_state: MutexGuard<'_, Redundant<LifeState>> = server_state.state.lock().await;

    // short name when alive, full name when in any negative state.
    let name: String = match **locked_state {
        LifeState::Alive => server_state.config.global.name.clone(),
        _ => server_state.config.global.full_name.clone(),
    };

    let status_title: String = locked_state.to_string();
    let status_color: String = locked_state.css_color();

    // whether we want to grayscale certain UI elements out of respect
    let is_dead: String = match **locked_state {
        LifeState::Dead | LifeState::MissingOrDead => DEAD_CSS_ID.into(),
        _ => "".into(),
    };

    // pick a status image
    let status_img_paths: &Vec<String> = match **locked_state {
        LifeState::Alive => &server_state.config.state.alive.images,
        LifeState::ProbablyAlive => &server_state.config.state.uncertain.images,
        LifeState::MissingOrDead => &server_state.config.state.missing.images,
        LifeState::Incapacitated => &server_state.config.state.incapacitated.images,
        LifeState::Dead => &server_state.config.state.dead.images,
    };
    let num_images: usize = status_img_paths.len();
    let img_index: usize = usize::try_from(img_randint % (num_images as u64)).unwrap();
    let img_path: String = status_img_paths.get(img_index).unwrap().clone();

    // pick a status message
    let status_msgs: &Vec<String> = match **locked_state {
        LifeState::Alive => &server_state.config.state.alive.messages,
        LifeState::ProbablyAlive => &server_state.config.state.uncertain.messages,
        LifeState::MissingOrDead => &server_state.config.state.missing.messages,
        LifeState::Incapacitated => &server_state.config.state.incapacitated.messages,
        LifeState::Dead => &server_state.config.state.dead.messages,
    };
    let num_msgs: usize = status_msgs.len();
    let msg_index: usize = usize::try_from(msg_randint % (num_msgs as u64)).unwrap();

    let mut formatted_status_msg: String = status_msgs.get(msg_index).unwrap().clone();
    formatted_status_msg = formatted_status_msg.replace("{0}", &name);

    // if we're in the uncertain/unresponsive state, we need to also
    // format the number of hours since the last heartbeat
    match **locked_state {
        LifeState::ProbablyAlive | LifeState::MissingOrDead | LifeState::Incapacitated => {
            let last_seen: u64 = **server_state.last_heartbeat.lock().await;

            // just a sanity check to make sure this isnt possible past this point
            assert!(
                last_seen < now,
                "Last heartbeat recorded happened in the future!"
            );
            // also make sure we're able to truncate it to a u32 to convert to f64 later
            assert!((now - last_seen) <= u32::MAX.into());

            let seconds_since_last_seen: u32 = (now - last_seen) as u32;
            let mut hours_since_last_seen: f64 =
                ((f64::from(seconds_since_last_seen) / 60.0) / 60.0).round();

            // floor to 1 hour, so we don't display "last seen 0 hour ago."
            if hours_since_last_seen < 1_f64 {
                hours_since_last_seen = 1_f64;
            }

            formatted_status_msg =
                formatted_status_msg.replace("{1}", &hours_since_last_seen.to_string());

            let mut plural_str: &str = "";

            if hours_since_last_seen > 1_f64 {
                // make the text, 'hour', plural to 'hours'.
                plural_str = "s";
            }
            formatted_status_msg = formatted_status_msg.replace("{2}", plural_str);
        }
        _ => {}
    }
    drop(locked_state); // drop mutex as we no longer will read state

    // get latest heartbeat table / note to display
    let heartbeats: MutexGuard<'_, [HeartbeatDisplay; 5]> =
        server_state.displayed_heartbeats.lock().await;
    let locked_note: MutexGuard<'_, Option<String>> = server_state.note.lock().await;

    let html = IndexTemplate {
        name,
        status_title,
        status_color,
        status_image: img_path,
        status_message: formatted_status_msg,
        row_1_timestamp: heartbeats[0].timestamp.clone(),
        row_1_message: heartbeats[0].message.clone(),
        row_2_timestamp: heartbeats[1].timestamp.clone(),
        row_2_message: heartbeats[1].message.clone(),
        row_3_timestamp: heartbeats[2].timestamp.clone(),
        row_3_message: heartbeats[2].message.clone(),
        row_4_timestamp: heartbeats[3].timestamp.clone(),
        row_4_message: heartbeats[3].message.clone(),
        row_5_timestamp: heartbeats[4].timestamp.clone(),
        row_5_message: heartbeats[4].message.clone(),
        show_note: match *locked_note {
            Some(_) => String::default(),
            None => HIDE_CSS_ID.into(),
        },
        note_message: match &*locked_note {
            Some(note) => note.clone(),
            None => String::default(),
        },
        is_dead,
    }
    .render()
    .unwrap();

    Html(html)
}

#[derive(Template)]
#[template(path = "heartbeat.html")]
struct HeartbeatTemplate {
    name: String,
    show_note: String,
    note_message: String,
}

pub async fn heartbeat(State(server_state): State<ServerState>) -> impl IntoResponse {
    let locked_state: MutexGuard<'_, Redundant<LifeState>> = server_state.state.lock().await;

    // short name when alive, full name when in any negative state.
    let name: String = match **locked_state {
        LifeState::Alive => server_state.config.global.name.clone(),
        _ => server_state.config.global.full_name.clone(),
    };
    drop(locked_state); // drop mutex as we no longer will read state

    let locked_note: MutexGuard<'_, Option<String>> = server_state.note.lock().await;

    let html = HeartbeatTemplate {
        name,
        show_note: match *locked_note {
            Some(_) => String::default(),
            None => "hidden".into(),
        },
        note_message: match &*locked_note {
            Some(note) => note.clone(),
            None => String::default(),
        },
    }
    .render()
    .unwrap();

    Html(html)
}
