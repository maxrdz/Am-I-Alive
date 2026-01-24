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

use serde::Deserialize;

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct ServerConfig {
    pub global: Global,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct Global {
    pub name: String,
    pub full_name: String,
    pub utc_offset: i32,
    pub time_until_uncertain: u16,
    pub time_until_missing: u16,
    pub pow_difficulty: u8,
    pub heartbeat_auth_hash: String,
    pub ok_images: Vec<String>,
    pub ok_messages: Vec<String>,
    pub uncertain_images: Vec<String>,
    pub uncertain_message: String,
    pub death_images: Vec<String>,
    pub death_message: String,
}
