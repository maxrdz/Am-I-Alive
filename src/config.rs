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
    pub pow: Pow,
    pub state: StateGlobal,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct Global {
    pub name: String,
    pub full_name: String,
    pub utc_offset: i32,
    pub heartbeat_auth_hash: String,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct Pow {
    pub secret: String,
    pub difficulty: u8,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct StateGlobal {
    pub tick_interval: u16,
    pub time_until_uncertain: u16,
    pub time_until_missing: u16,
    pub minimum_uptime: u16,
    #[serde(default)]
    pub alive: State,
    #[serde(default)]
    pub uncertain: State,
    #[serde(default)]
    pub missing: State,
    #[serde(default)]
    pub incapacitated: State,
    #[serde(default)]
    pub dead: State,
}

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct State {
    pub images: Vec<String>,
    pub messages: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            images: vec!["https://placehold.co/400".into()],
            messages: vec!["The last heartbeat received from {0} was {1} hour{2} ago.".into()],
        }
    }
}
