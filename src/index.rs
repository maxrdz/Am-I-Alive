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

use std::fmt::Display;

use crate::redundancy::Redundant;
use crate::{HeartbeatDisplay, LifeState, ServerState};
use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse},
};
use rand::rand_core::{OsRng, TryRngCore};
use tokio::sync::MutexGuard;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    name: String,
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
}

pub async fn index(State(server_state): State<ServerState>) -> impl IntoResponse {
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

    // pick a status image
    let status_img_paths: &Vec<String> = match **locked_state {
        LifeState::Alive => &server_state.config.global.ok_images,
        LifeState::Dead => &server_state.config.global.death_images,
        _ => &server_state.config.global.uncertain_images,
    };
    let num_images: usize = status_img_paths.len();
    let img_index: usize = usize::try_from(img_randint % (num_images as u64)).unwrap();
    let img_path: String = status_img_paths.get(img_index).unwrap().clone();

    // pick a status message
    let status_msgs: &Vec<String> = match **locked_state {
        LifeState::Alive => &server_state.config.global.ok_messages,
        LifeState::Dead => &vec![server_state.config.global.death_message.clone()],
        _ => &vec![server_state.config.global.uncertain_message.clone()],
    };
    drop(locked_state); // drop mutex as we no longer will read state

    let num_msgs: usize = status_msgs.len();
    let msg_index: usize = usize::try_from(msg_randint % (num_msgs as u64)).unwrap();

    let mut formatted_status_msg: String = status_msgs.get(msg_index).unwrap().clone();
    formatted_status_msg = formatted_status_msg.replace("{0}", &name);

    // get latest heartbeat table to display
    let heartbeats: &[HeartbeatDisplay; 5] = &server_state.displayed_heartbeats;

    let locked_note: MutexGuard<'_, Option<String>> = server_state.note.lock().await;

    let html = IndexTemplate {
        name,
        status_title,
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
