//! Inserter capacities for the generated, normal-quality, one-item-hand profile.
//!
//! Swing physics and belt-pickup calibration adapted from Inserter Throughput
//! Tooltip, commit 49c666e4022c69526ad74dd2577e3155c18cdd89 (Factorio 2.1.22).
//! https://github.com/mpodreshetnikov/inserter-throughput-tooltip
//! See licenses/inserter-throughput-tooltip-MIT.txt. The JSON stores its original
//! 150 pickup trees with one-based feature/child indices, converted from Lua.

use std::{f64::consts::TAU, sync::OnceLock};

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

use super::{property_number, stat};
use crate::model::Entity;

const EPSILON: f64 = 1e-6;
// Belt pickup is approximate; reserve 5% capacity in addition to factory headroom.
const PICKUP_UTILIZATION: f64 = 0.95;

pub(super) struct Rates {
    pub output: f64,
    pub input_both: f64,
    pub input_single: f64,
}

impl Rates {
    pub fn input(&self, lanes: &str) -> f64 {
        if lanes == "both" {
            self.input_both
        } else {
            self.input_single
        }
    }
}

#[derive(Deserialize)]
struct Model {
    base: f64,
    rate: f64,
    trees: Vec<Tree>,
}

#[derive(Deserialize)]
struct Tree {
    f: Vec<usize>,
    t: Vec<f64>,
    l: Vec<usize>,
    r: Vec<usize>,
    v: Vec<f64>,
}

fn pickup_correction(features: [f64; 13]) -> f64 {
    static MODEL: OnceLock<Model> = OnceLock::new();
    let model = MODEL.get_or_init(|| {
        serde_json::from_str(include_str!("inserter_pickup_model.json"))
            .expect("bundled inserter pickup model must be valid")
    });
    let sum: f64 = model
        .trees
        .iter()
        .map(|tree| {
            let mut node = 0;
            while tree.f[node] != 0 {
                node = if features[tree.f[node] - 1] <= tree.t[node] {
                    tree.l[node] - 1
                } else {
                    tree.r[node] - 1
                };
            }
            tree.v[node]
        })
        .sum();
    (model.base + model.rate * sum).clamp(-0.7, 0.7)
}

fn vector(arm: &Entity, key: &str) -> Result<[f64; 2]> {
    let value = arm
        .prototype
        .properties
        .get(key)
        .context("Missing inserter vector")?;
    let x = value[0].as_f64().context("Invalid inserter vector x")?;
    let y = value[1].as_f64().context("Invalid inserter vector y")?;
    ensure!(x.is_finite() && y.is_finite(), "Invalid inserter vector");
    Ok([x, y])
}

fn movement_ticks(distance: f64, speed: f64) -> f64 {
    if distance <= EPSILON {
        0.0
    } else {
        (distance / speed - EPSILON).ceil() - 1.0
    }
}

pub(super) fn rates(arm: &Entity, belt: &Entity) -> Result<Rates> {
    let rotation = stat(arm, "inserter_rotation_speed")?;
    let extension = stat(arm, "inserter_extension_speed")?;
    let belt_speed = property_number(belt, "belt_speed")?;
    ensure!(
        rotation > 0.0 && extension > 0.0 && belt_speed > 0.0,
        "Inserter and belt speeds must be positive"
    );
    let pickup = vector(arm, "inserter_pickup_position")?;
    let drop = vector(arm, "inserter_drop_position")?;
    let radius = pickup[0].hypot(pickup[1]);
    let drop_radius = drop[0].hypot(drop[1]);
    ensure!(
        radius > 0.0 && drop_radius > 0.0,
        "Inserter reach must be positive"
    );
    let angle = (pickup[1].atan2(pickup[0]) - drop[1].atan2(drop[0])).abs();
    let turns = angle.min(TAU - angle) / TAU;
    // Rotation and extension run concurrently. Extension covers the difference
    // of radii, not the entire distance from the inserter to its target.
    let half = movement_ticks(turns, rotation)
        .max(movement_ticks((drop_radius - radius).abs(), extension))
        .max(1.0);
    let swing = 2.0 * half;
    let input = |lanes: f64| {
        // This profile fixes the hand and belt item stack to one. The pickup
        // model features retain the upstream order (including the zero terms).
        let correction = pickup_correction([
            half,
            rotation.ln(),
            extension.ln(),
            radius,
            belt_speed,
            1.0,
            lanes,
            1.0,
            belt_speed / extension,
            belt_speed / (TAU * radius * rotation),
            0.25 / (lanes * belt_speed) / (0.2 / extension).max(1.0),
            0.0,
            0.0,
        ]);
        (60.0 / (swing * correction.exp())).min(lanes * belt_speed * 240.0) * PICKUP_UTILIZATION
    };
    let rates = Rates {
        // One item can be dropped immediately; only the destination lane caps it.
        output: (60.0 / swing).min(belt_speed * 240.0),
        input_both: input(2.0),
        input_single: input(1.0),
    };
    ensure!(
        [rates.output, rates.input_both, rates.input_single]
            .into_iter()
            .all(|r| r.is_finite() && r > 0.0),
        "Invalid inserter capacity"
    );
    Ok(rates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn arm(rotation: f64, extension: f64, reach: f64) -> Entity {
        serde_json::from_value(json!({
            "name":"test-arm", "type":"inserter", "hidden":false, "tile_width":1, "tile_height":1,
            "fluid_boxes":[],
            "collision_box":{"left_top":{"x":-0.4,"y":-0.4},"right_bottom":{"x":0.4,"y":0.4}},
            "selection_box":{"left_top":{"x":-0.5,"y":-0.5},"right_bottom":{"x":0.5,"y":0.5}},
            "inserter_pickup_position":[0,-reach], "inserter_drop_position":[0,reach+0.2],
            "quality_stats":{"normal":{"inserter_rotation_speed":rotation,"inserter_extension_speed":extension}}
        })).unwrap()
    }

    #[test]
    fn matches_measured_normal_quality_cycles_and_keeps_pickup_separate() -> Result<()> {
        let mut belt = arm(0.04, 0.1, 1.0);
        belt.prototype
            .properties
            .insert("belt_speed".into(), json!(0.03125));
        for (rotation, extension, reach, cycle) in [
            (0.014, 0.035, 1.0, 70.0),
            (0.02, 0.05, 2.0, 48.0),
            (0.04, 0.1, 1.0, 24.0),
        ] {
            let r = rates(&arm(rotation, extension, reach), &belt)?;
            assert!((r.output - 60.0 / cycle).abs() < 1e-10);
            assert!(r.input_both > r.output * 0.7 && r.input_both < r.output * 1.3);
            assert!(r.input_single > 0.0 && r.input_single <= 7.5);
        }
        Ok(())
    }

    #[test]
    fn extension_overlaps_rotation_and_slow_belts_cap_each_direction() -> Result<()> {
        let mut belt = arm(0.04, 0.1, 1.0);
        belt.prototype
            .properties
            .insert("belt_speed".into(), json!(0.03125));
        // 0.2 tile radial change takes 19 ticks; rotation takes 12, concurrently.
        assert!((rates(&arm(0.04, 0.01, 1.0), &belt)?.output - 60.0 / 38.0).abs() < 1e-10);
        belt.prototype
            .properties
            .insert("belt_speed".into(), json!(0.001));
        let r = rates(&arm(0.04, 0.1, 1.0), &belt)?;
        assert_eq!(r.output, 0.24);
        assert!(r.input_single <= 0.24 && r.input_both <= 0.48);
        belt.prototype
            .properties
            .insert("belt_speed".into(), json!(0));
        assert!(rates(&arm(0.04, 0.1, 1.0), &belt).is_err());
        Ok(())
    }
}
