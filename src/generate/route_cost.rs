//! Relative transport cost. Hidden tunnel spans still count as route length.

use std::collections::HashMap;

use anyhow::Result;

use super::{entity, power::tiles};
use crate::{GameData, blueprint::BlueprintEntity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Metrics {
    pub cost: u64,
    pub turns: usize,
    pub length: u64,
    pub area: u64,
}

pub(super) fn measure(data: &GameData, entities: &[BlueprintEntity]) -> Result<Metrics> {
    let mut occupied = HashMap::new();
    let mut transport = Vec::new();
    for e in entities {
        let p = entity(data, &e.name)?;
        if matches!(
            p.prototype.prototype_type.as_str(),
            "transport-belt" | "underground-belt" | "splitter"
        ) {
            for tile in tiles(e, p)? {
                occupied.insert(tile, e);
            }
            transport.push((e, p));
        }
    }
    let mut cost = 0;
    let mut length = 0;
    let mut turns = 0;
    for (e, p) in transport {
        let (dx, dy) = match e.direction {
            0 => (0, -1),
            4 => (1, 0),
            8 => (0, 1),
            _ => (-1, 0),
        };
        let tile = (e.position.x.floor() as i32, e.position.y.floor() as i32);
        match p.prototype.prototype_type.as_str() {
            "splitter" => {
                cost += 8;
                length += 2;
            }
            "underground-belt" => {
                cost += 4;
                length += 1;
                if e.belt_type.as_deref() == Some("input") {
                    let reach = p.prototype.properties["max_underground_distance"]
                        .as_u64()
                        .unwrap_or(0) as i32;
                    if let Some(distance) = (1..=reach).find(|&n| {
                        occupied
                            .get(&(tile.0 + dx * n, tile.1 + dy * n))
                            .is_some_and(|other| {
                                other.name == e.name
                                    && other.direction == e.direction
                                    && other.belt_type.as_deref() == Some("output")
                            })
                    }) {
                        let hidden = (distance - 1) as u64;
                        cost += hidden;
                        length += hidden;
                    }
                    continue;
                }
            }
            _ => {
                cost += 1;
                length += 1;
            }
        }
        if occupied
            .get(&(tile.0 + dx, tile.1 + dy))
            .is_some_and(|next| next.direction != e.direction)
        {
            turns += 1;
        }
    }
    let area = if occupied.is_empty() {
        0
    } else {
        let min_x = occupied.keys().map(|p| p.0).min().unwrap();
        let max_x = occupied.keys().map(|p| p.0).max().unwrap();
        let min_y = occupied.keys().map(|p| p.1).min().unwrap();
        let max_y = occupied.keys().map(|p| p.1).max().unwrap();
        (max_x - min_x + 1) as u64 * (max_y - min_y + 1) as u64
    };
    Ok(Metrics {
        cost: cost + 2 * turns as u64,
        turns,
        length,
        area,
    })
}
