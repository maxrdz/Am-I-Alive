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

use axum::{
    response::{Html, IntoResponse},
    routing::get,
    Router,
    extract::State,
};
use askama::Template;
use std::io::Read;
use std::fs::File;
use std::sync::Arc;

mod config;

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    name: String,
    status_image: String,
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
}

async fn index(
    State(config): State<Arc<config::ServerConfig>>,
) -> impl IntoResponse {
    let name: String = config.global.name.clone();
    
    let mut formatted_status_msg: String = config.global.ok_messages.get(0).unwrap().clone();
    formatted_status_msg = formatted_status_msg.replace("{0}", &name);

    let html = IndexTemplate {
        name: name.clone(),
        status_image: config.global.ok_images.get(0).unwrap().into(),
        status_message: formatted_status_msg,
        row_1_timestamp: "N/A".into(),
        row_1_message: "N/A".into(),
        row_2_timestamp: "N/A".into(),
        row_2_message: "N/A".into(),
        row_3_timestamp: "N/A".into(),
        row_3_message: "N/A".into(),
        row_4_timestamp: "N/A".into(),
        row_4_message: "N/A".into(),
        row_5_timestamp: "N/A".into(),
        row_5_message: "N/A".into(),
    }
    .render()
    .unwrap();

    Html(html)
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

    conf_file.read_to_string(&mut contents).expect("Failed to read file contents to string.");
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

    // start the web server
    let app = Router::new()
        .route("/", get(index))
        .with_state(daemon_config);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
