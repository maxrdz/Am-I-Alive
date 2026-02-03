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

use std::ops::Deref;

/// Store multiple copies of a value in memory in case they
/// are somehow corrupted by a cosmic ray or something.
///
/// I think some memory chips have this kind of protection, but,
/// in case you don't- I don't want people to think you're dead
/// when you're not haha.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Redundant<T: Eq + Copy> {
    a: T,
    b: T,
    c: T,
}

impl<T: Eq + Copy> Redundant<T> {
    pub fn new(v: T) -> Self {
        Self { a: v, b: v, c: v }
    }
}

impl<T: Eq + Copy> Deref for Redundant<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        if (self.a == self.b) && (self.b == self.c) {
            &self.a
        } else {
            // the state of this struct at this point is not possible,
            // which means there was some memory corruption somehow
            panic!("Memory corruption detected. Hoping your docker container restarts itself.")
        }
    }
}
